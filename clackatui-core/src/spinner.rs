//! Ported from `@clack/prompts`' `spinner.ts` — the first renderer that draws more than once.
//!
//! Everything in [`crate::message`], [`crate::note`] and [`crate::box`] writes once and is done. A
//! spinner writes a row, walks the cursor back over it, and writes another — for as long as the
//! caller lets it. That makes it the first thing here with a *clock* in it, and the clock is the one
//! part that does not come across: this module takes the elapsed time as an argument and returns the
//! bytes. Ticking is the driver's job.
//!
//! # The cursor is hidden by something this crate does not have
//!
//! `start` calls `block()` and `_stop` calls the `unblock` it returned. `block` belongs to
//! `@clack/core`: it puts the input in raw mode and swallows keypresses so that typing during a
//! spinner leaves no echo — all of which is the driver's business, none of which is here. But the
//! first thing it writes is `cursor.hide` and the last thing `unblock` writes is `cursor.show`, and
//! those two are on the terminal at exactly those points. So they are written here, and the driver
//! must not write them again.
//!
//! # Not the Emitter
//!
//! [`crate::emitter::Emitter`] is `Prompt.render`, which diffs two Frames and rewrites the rows that
//! changed. `spinner.ts` shares none of that: it keeps the previous *message*, walks up over however
//! many rows that message wrapped to, erases everything below, and writes the new row whole. No
//! diff, no hidden cursor — the cursor is hidden by `block()`, which belongs to the terminal and so
//! to the driver. So the escapes are written here rather than reached for, and there are only three.
//!
//! # The cursor is walked back over the wrong string
//!
//! `_prevMessage` is the caller's message. What was *written* is the frame symbol, two spaces, and
//! then the message — three columns wider, and possibly a row taller as a result. `clearPrevMessage`
//! wraps the bare message to decide how far up to go, so a message that lands within three columns
//! of the terminal's width leaves a row of debris behind on every tick. Reproduced (ADR-0013), and
//! recorded: clack's own suite has a case for it at `columns + 10`.
//!
//! # `stop()` clears the message
//!
//! `_message = msg ?? _message` looks like "keep the message if none is given", and cannot be: the
//! parameter defaults to `''`, not to `undefined`, so the fallback is unreachable and `stop()` with
//! no argument prints the step symbol and nothing else. The same expression is in `message()` and is
//! dead there too. Both are written here as the plain assignment they are.
//!
//! # An error is red here and yellow everywhere else
//!
//! Every Prompt colours `S_STEP_ERROR` yellow, through the same `symbol(state)` helper
//! ([`Theme::step`](crate::theme::Theme::step)). The spinner spells its own out and picks red. So
//! this module does not call that helper — it names the symbol and the colour separately, which is
//! the only way to say a thing upstream says twice differently.

use std::time::Duration;

use ratatui_core::style::{Color, Style};

use crate::emitter::{write_once, write_wrapped};
use crate::frame::{Frame, Line, Span};
use crate::prompt::Status;
use crate::theme::Theme;
use crate::wrap::wrap;

const CSI: &str = "\u{1b}[";

/// What goes after the message on every tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Indicator {
	/// Up to three dots, one more every eight frames.
	#[default]
	Dots,
	/// `[1m 3s]` since [`Spinner::start`].
	Timer,
}

/// A caller's `styleFrame`, applied to the frame symbol.
///
/// Upstream's is `(frame: string) => string`; this returns a drawn [`Line`] for the reason ADR-0030
/// gives about a `note`'s formatter. It is applied before the row is wrapped, so a formatter that
/// adds characters moves the break — which is what upstream's does too.
///
/// `Send + Sync` where [`crate::note`]'s and [`crate::box`]'s formatters are neither, and for a
/// reason that is not aesthetic: a spinner is the only renderer here that is drawn from a thread
/// the caller does not own, so its formatter is the only one that crosses one.
pub type StyleFrame<'a> = &'a (dyn Fn(&str) -> Line + Send + Sync);

fn magenta(frame: &str) -> Line {
	Line::from(Span::styled(frame, Style::new().fg(Color::Magenta)))
}

static MAGENTA: fn(&str) -> Line = magenta;

pub struct Options<'a> {
	pub indicator: Indicator,
	/// The symbols cycled through, one per tick. `None` is the Theme's.
	pub frames: Option<Vec<String>>,
	pub with_guide: bool,
	/// `isCI()`: no animation, a newline before each clear, and a tick that writes nothing at all
	/// unless the message changed.
	pub ci: bool,
	pub style_frame: StyleFrame<'a>,
}

impl Default for Options<'_> {
	fn default() -> Self {
		Self {
			indicator: Indicator::Dots,
			frames: None,
			with_guide: true,
			ci: false,
			style_frame: &MAGENTA,
		}
	}
}

/// A spinner, without a clock. See the module docs.
pub struct Spinner<'a> {
	options: Options<'a>,
	theme: Theme,
	columns: usize,
	frames: Vec<String>,
	active: bool,
	/// A [`Line`] rather than a string, because [`crate::progress`] sets a message that is drawn —
	/// a coloured bar and then the caller's text. Upstream's is a string with escapes in it, which
	/// is the same thing said the way ADR-0011 says not to.
	message: Line,
	/// The message the last tick wrote, and what `clearPrevMessage` measures. `None` until a tick
	/// has happened — and never cleared, so a restarted spinner still walks back over it.
	previous: Option<Line>,
	frame_index: usize,
	/// Counts eighths of a dot. Upstream's `indicatorTimer`, and it is a float there too.
	indicator_timer: f64,
}

impl<'a> Spinner<'a> {
	pub fn new(theme: Theme, columns: usize, options: Options<'a>) -> Self {
		let frames = options.frames.clone().unwrap_or_else(|| {
			theme
				.symbols
				.spinner_frames
				.iter()
				.map(|frame| (*frame).to_owned())
				.collect()
		});
		Self {
			options,
			theme,
			columns,
			frames,
			active: false,
			message: Line::blank(),
			previous: None,
			frame_index: 0,
			indicator_timer: 0.0,
		}
	}

	pub fn is_active(&self) -> bool {
		self.active
	}

	/// `start`: hide the cursor, then the bar above the spinner if there is a Guide.
	///
	/// The first row comes from the first [`tick`](Self::tick) — upstream draws nothing until the
	/// interval fires either.
	pub fn start(&mut self, message: impl Into<Line>) -> String {
		self.active = true;
		self.message = remove_trailing_dots(message.into());
		self.frame_index = 0;
		self.indicator_timer = 0.0;
		// `block()`'s, not the spinner's — see the module docs.
		let mut out = format!("{CSI}?25l");
		if self.options.with_guide {
			out.push_str(&write_once(&Frame {
				lines: vec![Line::from(Span::styled(
					self.theme.symbols.bar,
					self.theme.styles.guide,
				))],
			}));
		}
		out
	}

	/// One turn of the interval: erase the last row and write the next one.
	///
	/// `elapsed` is the time since `start`, and is read only by [`Indicator::Timer`].
	pub fn tick(&mut self, elapsed: Duration) -> String {
		// In CI a tick that would write the same message writes nothing — which is what keeps a CI log
		// from filling up with one line per frame. Note it is the *message* that is compared, so the
		// frame symbol and the dots never advance there.
		if self.options.ci && self.previous.as_ref() == Some(&self.message) {
			return String::new();
		}

		let mut out = self.clear_previous();
		self.previous = Some(self.message.clone());

		let suffix = if self.options.ci {
			"...".to_owned()
		} else if self.options.indicator == Indicator::Timer {
			format!(" {}", format_timer(elapsed))
		} else {
			// `'.'.repeat(Math.floor(indicatorTimer)).slice(0, 3)` — the count is taken before the
			// increment below, so the first tick has no dots and the fifth has three.
			".".repeat((self.indicator_timer as usize).min(3))
		};

		// The only wrapped write in the module, and the reason [`write_wrapped`] is not `write_once`:
		// upstream calls `wrapAnsi` here and nowhere else in `spinner.ts`, and there is no newline
		// after it — the row stays where the next tick will erase it.
		let row = self.row(&self.frames[self.frame_index].clone(), &suffix);
		out.push_str(&write_wrapped(
			&row,
			self.columns.min(u16::MAX as usize) as u16,
		));

		self.frame_index = if self.frame_index + 1 < self.frames.len() {
			self.frame_index + 1
		} else {
			0
		};
		self.indicator_timer = if self.indicator_timer < 4.0 {
			self.indicator_timer + 0.125
		} else {
			0.0
		};
		out
	}

	/// `message`: what the next tick will draw. Writes nothing by itself.
	pub fn set_message(&mut self, message: impl Into<Line>) {
		self.message = remove_trailing_dots(message.into());
	}

	/// `stop`, `cancel` and `error`, which differ only in the symbol they leave behind.
	///
	/// [`Status::Submit`] is `stop`, [`Status::Cancel`] is `cancel`, and anything else is `error`.
	/// The message is *not* stripped of its trailing dots — only [`start`](Self::start) and
	/// [`set_message`](Self::set_message) do that — and an empty one is an empty one, per the module
	/// docs.
	pub fn stop(&mut self, message: impl Into<Line>, status: Status, elapsed: Duration) -> String {
		let Some(mut out) = self.settle(message) else {
			return String::new();
		};

		let (symbol, style) = match status {
			Status::Submit => (
				self.theme.symbols.step_submit,
				self.theme.styles.step_submit,
			),
			Status::Cancel => (
				self.theme.symbols.step_cancel,
				self.theme.styles.step_cancel,
			),
			// Red, not the yellow `symbol(state)` gives every Prompt. See the module docs.
			_ => (self.theme.symbols.step_error, self.theme.styles.step_cancel),
		};

		let suffix = if self.options.indicator == Indicator::Timer {
			format!(" {}", format_timer(elapsed))
		} else {
			String::new()
		};
		// Not wrapped, where a tick's row is: upstream hands this one to the terminal whole.
		out.push_str(&write_once(
			&self.row_with(Span::styled(symbol, style), &suffix),
		));
		// `unblock()`, which is the last thing `_stop` does.
		out.push_str(&format!("{CSI}?25h"));
		out
	}

	/// `clear`: stop without writing the closing row.
	///
	/// Leaves the bar `start` wrote behind, which upstream has a `TODO` about and does not do
	/// anything about.
	pub fn clear(&mut self) -> String {
		match self.settle("") {
			Some(mut out) => {
				out.push_str(&format!("{CSI}?25h"));
				out
			}
			None => String::new(),
		}
	}

	/// The half of `_stop` that both endings share: the erase, and the message it settles on.
	/// `None` when there was nothing to stop.
	fn settle(&mut self, message: impl Into<Line>) -> Option<String> {
		if !self.active {
			return None;
		}
		self.active = false;
		let out = self.clear_previous();
		self.message = message.into();
		Some(out)
	}

	/// `clearPrevMessage`: back to the top-left of what was written, then erase everything below.
	///
	/// The row count comes from the bare message rather than from the row that was drawn. See the
	/// module docs — that is upstream's, and it is off by the width of the prefix.
	fn clear_previous(&self) -> String {
		let Some(previous) = &self.previous else {
			return String::new();
		};
		let mut out = String::new();
		if self.options.ci {
			out.push('\n');
		}
		// The text of the message and not its styling: `wrapAnsi` skips the escapes upstream puts in
		// the same place, so the rows are decided by what is visible either way.
		let rows = wrap(&plain(previous), self.columns).split('\n').count();
		if rows > 1 {
			// `cursor.up(n)`.
			out.push_str(&format!("{CSI}{}A", rows - 1));
		}
		// `cursor.to(0)`, which sisteransi writes as a one-based column, and `erase.down()`.
		out.push_str(&format!("{CSI}1G{CSI}J"));
		out
	}

	/// A tick's row: the frame symbol, formatted, then the message and the suffix.
	fn row(&self, frame: &str, suffix: &str) -> Frame {
		self.finish_row((self.options.style_frame)(frame), suffix)
	}

	/// The same row with a fixed symbol, for the one a `stop` leaves behind.
	fn row_with(&self, symbol: Span, suffix: &str) -> Frame {
		self.finish_row(Line::from(symbol), suffix)
	}

	/// `${head}  ${message}${suffix}`, split into Lines wherever the message is.
	fn finish_row(&self, mut head: Line, suffix: &str) -> Frame {
		head.push(Span::raw("  "));
		let mut head = Some(head);
		let mut lines = Vec::new();
		for part in self.message.paragraphs() {
			let mut line = head.take().unwrap_or_else(Line::blank);
			line.spans.extend(part.spans);
			lines.push(line);
		}
		// The suffix goes after the message, which is after the last of its rows.
		if let Some(last) = lines.last_mut() {
			last.push(Span::raw(suffix));
		}
		Frame { lines }
	}
}

/// `msg.replace(/\.+$/, '')` — across the spans, since the regular expression does not know they
/// are there.
fn remove_trailing_dots(mut message: Line) -> Line {
	while let Some(last) = message.spans.last_mut() {
		last.text.truncate(last.text.trim_end_matches('.').len());
		if !last.text.is_empty() {
			break;
		}
		message.spans.pop();
	}
	message
}

/// A Line's text with its styling dropped — what upstream would have measured.
fn plain(line: &Line) -> String {
	line.spans.iter().map(|span| span.text.as_str()).collect()
}

/// `formatTimer`: whole seconds, and whole minutes once there are any.
fn format_timer(elapsed: Duration) -> String {
	let seconds = elapsed.as_secs_f64();
	let minutes = (seconds / 60.0).floor() as u64;
	let rest = (seconds % 60.0).floor() as u64;
	if minutes > 0 {
		format!("[{minutes}m {rest}s]")
	} else {
		format!("[{rest}s]")
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn spinner<'a>(options: Options<'a>) -> Spinner<'a> {
		Spinner::new(Theme::clack(), 80, options)
	}

	/// The bytes with their SGR taken out; the escapes that move the cursor stay.
	fn strip(bytes: &str) -> String {
		let mut out = String::new();
		let mut chars = bytes.chars();
		while let Some(c) = chars.next() {
			if c == '\u{1b}' {
				let mut sequence = String::from("\u{1b}");
				for c in chars.by_ref() {
					sequence.push(c);
					if c.is_ascii_alphabetic() {
						break;
					}
				}
				if !sequence.ends_with('m') {
					out.push_str(&sequence);
				}
			} else {
				out.push(c);
			}
		}
		out
	}

	#[test]
	fn a_tick_writes_a_frame_and_the_next_one_erases_it() {
		let mut spinner = spinner(Options::default());
		assert_eq!(strip(&spinner.start("Loading")), "\u{1b}[?25l│\n");
		assert_eq!(strip(&spinner.tick(Duration::ZERO)), "◒  Loading");
		// The second tick walks back to the first column, erases, and draws the next symbol. One row,
		// so there is no `cursor.up` at all.
		assert_eq!(
			strip(&spinner.tick(Duration::ZERO)),
			"\u{1b}[1G\u{1b}[J◐  Loading"
		);
	}

	#[test]
	fn the_dots_arrive_one_every_eight_ticks_and_stop_at_three() {
		let mut spinner = spinner(Options::default());
		spinner.start("Loading");
		let dots = |spinner: &mut Spinner| {
			let row = strip(&spinner.tick(Duration::ZERO));
			row.chars().filter(|&c| c == '.').count()
		};
		let counts: Vec<usize> = (0..40).map(|_| dots(&mut spinner)).collect();
		assert_eq!(counts[0], 0);
		assert_eq!(counts[7], 0);
		assert_eq!(counts[8], 1);
		assert_eq!(counts[24], 3);
		// Four dots' worth, sliced to three — and then back to none.
		assert_eq!(counts[32], 3);
		assert_eq!(counts[33], 0);
	}

	#[test]
	fn a_timer_counts_minutes_once_there_are_any() {
		assert_eq!(format_timer(Duration::from_millis(1500)), "[1s]");
		assert_eq!(format_timer(Duration::from_secs(59)), "[59s]");
		assert_eq!(format_timer(Duration::from_secs(60)), "[1m 0s]");
		assert_eq!(format_timer(Duration::from_secs(125)), "[2m 5s]");
	}

	#[test]
	fn stopping_with_no_message_leaves_the_symbol_alone() {
		let mut spinner = spinner(Options::default());
		spinner.start("Loading");
		spinner.tick(Duration::ZERO);
		assert_eq!(
			strip(&spinner.stop("", Status::Submit, Duration::ZERO)),
			"\u{1b}[1G\u{1b}[J◇  \n\u{1b}[?25h"
		);
	}

	#[test]
	fn a_stopped_spinner_cannot_be_stopped_again() {
		let mut spinner = spinner(Options::default());
		spinner.start("Loading");
		spinner.tick(Duration::ZERO);
		assert!(
			!spinner
				.stop("done", Status::Submit, Duration::ZERO)
				.is_empty()
		);
		assert_eq!(spinner.stop("done", Status::Submit, Duration::ZERO), "");
	}

	#[test]
	fn trailing_dots_are_taken_off_a_message_but_not_off_a_stop() {
		let mut spinner = spinner(Options::default());
		spinner.start("Loading...");
		assert_eq!(strip(&spinner.tick(Duration::ZERO)), "◒  Loading");
		assert!(
			strip(&spinner.stop("Done.", Status::Submit, Duration::ZERO)).contains("◇  Done.\n")
		);
	}

	#[test]
	fn cancelling_and_erroring_differ_only_in_the_symbol() {
		let ended = |status| {
			let mut spinner = spinner(Options::default());
			spinner.start("Loading");
			spinner.tick(Duration::ZERO);
			strip(&spinner.stop("stopped", status, Duration::ZERO))
		};
		assert!(ended(Status::Cancel).contains("■  stopped\n"));
		assert!(ended(Status::Error).contains("▲  stopped\n"));
	}

	/// The message wraps to two rows, so the next clear walks up one.
	#[test]
	fn a_wrapped_message_is_walked_back_over_row_by_row() {
		let mut spinner = Spinner::new(Theme::clack(), 20, Options::default());
		spinner.start("aaaa bbbb cccc dddd eeee");
		spinner.tick(Duration::ZERO);
		assert!(strip(&spinner.tick(Duration::ZERO)).starts_with("\u{1b}[1A\u{1b}[1G\u{1b}[J"));
	}

	/// A message that fits in the terminal but not once the prefix is on it. Upstream walks back over
	/// the message's rows, not the drawn row's — see the module docs.
	#[test]
	fn the_walk_back_ignores_the_three_columns_the_prefix_costs() {
		let mut spinner = Spinner::new(Theme::clack(), 20, Options::default());
		spinner.start("x".repeat(19));
		let first = strip(&spinner.tick(Duration::ZERO));
		// Drawn over two rows...
		assert!(first.contains('\n'), "{first:?}");
		// ...and walked back over one.
		assert!(
			!strip(&spinner.tick(Duration::ZERO)).contains('A'),
			"the cursor should not have gone up at all"
		);
	}

	#[test]
	fn ci_writes_a_newline_and_only_when_the_message_changes() {
		let mut spinner = spinner(Options {
			ci: true,
			..Options::default()
		});
		spinner.start("Loading");
		assert_eq!(strip(&spinner.tick(Duration::ZERO)), "◒  Loading...");
		assert_eq!(spinner.tick(Duration::ZERO), "");
		spinner.set_message("Still loading");
		assert_eq!(
			strip(&spinner.tick(Duration::ZERO)),
			"\n\u{1b}[1G\u{1b}[J◐  Still loading..."
		);
	}

	#[test]
	fn a_timer_is_drawn_after_the_message_on_every_row_it_ends_on() {
		let mut spinner = spinner(Options {
			indicator: Indicator::Timer,
			..Options::default()
		});
		spinner.start("Working");
		assert_eq!(
			strip(&spinner.tick(Duration::from_secs(3))),
			"◒  Working [3s]"
		);
		assert!(
			strip(&spinner.stop("Done", Status::Submit, Duration::from_secs(65)))
				.contains("◇  Done [1m 5s]\n")
		);
	}

	#[test]
	fn a_multi_line_message_puts_the_suffix_on_its_last_row() {
		let mut spinner = spinner(Options::default());
		spinner.start("foo\nbar");
		assert_eq!(strip(&spinner.tick(Duration::ZERO)), "◒  foo\nbar");
	}

	#[test]
	fn clear_writes_the_erase_and_nothing_else() {
		let mut spinner = spinner(Options::default());
		spinner.start("Loading");
		spinner.tick(Duration::ZERO);
		assert_eq!(spinner.clear(), "\u{1b}[1G\u{1b}[J\u{1b}[?25h");
	}

	#[test]
	fn custom_frames_are_cycled_in_order() {
		let mut spinner = spinner(Options {
			frames: Some(vec!["a".into(), "b".into()]),
			..Options::default()
		});
		spinner.start("x");
		let row = |spinner: &mut Spinner| strip(&spinner.tick(Duration::ZERO));
		assert!(row(&mut spinner).ends_with("a  x"));
		assert!(row(&mut spinner).ends_with("b  x"));
		assert!(row(&mut spinner).ends_with("a  x"));
	}

	/// The regular expression upstream does not know the spans are there, so neither does this: a
	/// span that is all dots is taken off and the one before it is trimmed in turn. Nothing in
	/// clack can reach it — a `progress` bar's message ends with the caller's text — but a caller
	/// of this crate can hand over any Line at all.
	#[test]
	fn trailing_dots_are_taken_off_across_the_spans_they_span() {
		let dotted: Line = [
			Span::raw("Loading."),
			Span::styled("..", Style::new().fg(Color::Red)),
			Span::raw("."),
		]
		.into_iter()
		.collect();
		let trimmed = remove_trailing_dots(dotted);
		assert_eq!(trimmed.spans.len(), 1);
		assert_eq!(trimmed.spans[0].text, "Loading");
	}

	#[test]
	fn without_the_guide_start_only_hides_the_cursor() {
		let mut spinner = spinner(Options {
			with_guide: false,
			..Options::default()
		});
		assert_eq!(spinner.start("x"), "\u{1b}[?25l");
	}
}
