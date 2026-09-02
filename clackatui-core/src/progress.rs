//! Ported from `@clack/prompts`' `progress-bar.ts` — a [`spinner`](crate::spinner) whose message is
//! a bar.
//!
//! There is no drawing here that the spinner does not do. `progress()` upstream makes a spinner,
//! keeps a number, and on every `advance` hands the spinner a *message* that happens to be a row of
//! block characters with the caller's text after it. So the frame still turns, the dots still
//! arrive, the walk back is still measured the way [`crate::spinner`] measures it — and everything
//! this module owns is the string in the middle.
//!
//! # The message is drawn, so the spinner's message is a Line
//!
//! Upstream's bar is `styleText('magenta', …) + styleText('dim', …) + ' ' + msg`: a string with
//! escapes in it, handed to `spin.message` and written out as-is. A Frame carries no escapes
//! (ADR-0011), so what is handed over here is a [`Line`]. That is the one change this unit made to
//! [`crate::spinner`] — its message went from a `String` to a `Line` — and it is why
//! `clearPrevMessage` now measures the Line's text rather than the Line: `wrapAnsi` skips escapes
//! when it counts columns, so the rows have always been decided by what is visible.
//!
//! # The bar has four colours and only ever uses one
//!
//! `activeStyle(state)` switches on a `State` — magenta while active, red for `error` and `cancel`,
//! green for `submit`. `drawProgress` is called from exactly two places, with `'initial'` and
//! `'active'`, and both of those are the magenta branch. The other three are unreachable: a
//! progress bar's ending is the spinner's closing row, which has no bar on it at all. So the port
//! has one colour, and this note instead of three dead branches.
//!
//! # What `advance` clamps, and what it does not
//!
//! `value = Math.min(max, step + value)` has a ceiling and no floor, so upstream can be walked
//! backwards past zero — and then `String.prototype.repeat` is called with a negative count and
//! throws. A step here is a `usize`, which is that defect made unreachable rather than reproduced:
//! ADR-0013 is about defects a terminal can see, and this one is an exception thrown before
//! anything reaches the terminal.

use std::time::Duration;

use ratatui_core::style::{Color, Modifier, Style};

use crate::frame::{Line, Span};
use crate::prompt::Status;
use crate::spinner::{self, Spinner};
use crate::theme::Theme;

/// Which character the bar is drawn out of. `S_PROGRESS_CHAR`'s three keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarStyle {
	/// `─`, or `-` without Unicode.
	Light,
	/// `━`, or `=`. Upstream's default.
	#[default]
	Heavy,
	/// `█`, or `#`.
	Block,
}

impl BarStyle {
	fn symbol(self, theme: &Theme) -> &'static str {
		theme.symbols.progress[self as usize]
	}
}

pub struct Options<'a> {
	pub style: BarStyle,
	/// The value that fills the bar. Upstream's `Math.max(1, max)` is applied here too.
	pub max: usize,
	/// The bar's width in columns, before the message is added to it.
	pub size: usize,
	/// The spinner underneath, whose options are the rest of `ProgressOptions`.
	pub spinner: spinner::Options<'a>,
}

impl Default for Options<'_> {
	fn default() -> Self {
		Self {
			style: BarStyle::Heavy,
			max: 100,
			size: 40,
			spinner: spinner::Options::default(),
		}
	}
}

/// A progress bar: a [`Spinner`] and the number that decides its message.
pub struct Progress<'a> {
	spinner: Spinner<'a>,
	symbol: &'static str,
	max: usize,
	size: usize,
	value: usize,
	/// The text after the bar, kept so that an `advance` with no message of its own can redraw the
	/// last one.
	previous_message: String,
}

impl<'a> Progress<'a> {
	pub fn new(theme: Theme, columns: usize, options: Options<'a>) -> Self {
		let symbol = options.style.symbol(&theme);
		Self {
			spinner: Spinner::new(theme, columns, options.spinner),
			symbol,
			max: options.max.max(1),
			size: options.size.max(1),
			value: 0,
			previous_message: String::new(),
		}
	}

	pub fn is_active(&self) -> bool {
		self.spinner.is_active()
	}

	pub fn start(&mut self, message: &str) -> String {
		self.previous_message = message.to_owned();
		let bar = self.bar(message);
		self.spinner.start(bar)
	}

	/// `advance`: move the bar on and set the message the next tick will draw. Writes nothing.
	///
	/// `message` of `None` is upstream's `msg ?? previousMessage` — a live fallback, unlike the
	/// dead one in `spinner.ts`'s `_stop`.
	pub fn advance(&mut self, step: usize, message: Option<&str>) {
		self.value = (self.value + step).min(self.max);
		if let Some(message) = message {
			self.previous_message = message.to_owned();
		}
		let bar = self.bar(&self.previous_message);
		self.spinner.set_message(bar);
	}

	/// `message`: an `advance` of nothing. The bar is redrawn where it already was.
	pub fn set_message(&mut self, message: &str) {
		self.advance(0, Some(message));
	}

	/// One turn of the interval, straight through to the spinner.
	pub fn tick(&mut self, elapsed: Duration) -> String {
		self.spinner.tick(elapsed)
	}

	/// The closing row, which has the message on it and no bar.
	pub fn stop(&mut self, message: &str, status: Status, elapsed: Duration) -> String {
		self.spinner.stop(message, status, elapsed)
	}

	pub fn clear(&mut self) -> String {
		self.spinner.clear()
	}

	/// `drawProgress`: the filled part, the unfilled part, a space, and the message.
	fn bar(&self, message: &str) -> Line {
		// `Math.floor((value / max) * size)`, in floating point because that is what it is upstream:
		// a division and a multiplication that integer arithmetic would not always round the same
		// way.
		let filled = ((self.value as f64 / self.max as f64) * self.size as f64).floor() as usize;
		[
			Span::styled(self.symbol.repeat(filled), Style::new().fg(Color::Magenta)),
			Span::styled(
				self.symbol.repeat(self.size - filled),
				Style::new().add_modifier(Modifier::DIM),
			),
			Span::raw(format!(" {message}")),
		]
		.into_iter()
		.collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn progress<'a>(options: Options<'a>) -> Progress<'a> {
		Progress::new(Theme::clack(), 80, options)
	}

	/// The bar's text, with the styles dropped.
	fn text(line: &Line) -> String {
		line.spans.iter().map(|span| span.text.as_str()).collect()
	}

	#[test]
	fn an_empty_bar_is_all_unfilled() {
		let progress = progress(Options {
			size: 4,
			..Options::default()
		});
		assert_eq!(text(&progress.bar("go")), "━━━━ go");
		assert_eq!(progress.bar("go").spans[0].text, "");
	}

	#[test]
	fn the_bar_fills_by_the_floor_of_the_fraction() {
		let mut progress = progress(Options {
			size: 4,
			max: 10,
			..Options::default()
		});
		progress.start("go");
		// 2/10 of four columns is 0.8, which floors to none.
		progress.advance(2, None);
		assert_eq!(progress.bar("go").spans[0].text, "");
		// 5/10 is two.
		progress.advance(3, None);
		assert_eq!(progress.bar("go").spans[0].text, "━━");
	}

	#[test]
	fn a_bar_cannot_be_advanced_past_its_max() {
		let mut progress = progress(Options {
			size: 4,
			max: 10,
			..Options::default()
		});
		progress.advance(99, None);
		assert_eq!(text(&progress.bar("")), "━━━━ ");
		assert_eq!(progress.bar("").spans[1].text, "");
	}

	#[test]
	fn a_max_or_a_size_of_zero_is_one() {
		let progress = progress(Options {
			size: 0,
			max: 0,
			..Options::default()
		});
		assert_eq!(text(&progress.bar("")), "━ ");
	}

	#[test]
	fn each_style_draws_a_different_character() {
		for (style, symbol) in [
			(BarStyle::Light, "─"),
			(BarStyle::Heavy, "━"),
			(BarStyle::Block, "█"),
		] {
			let progress = progress(Options {
				style,
				size: 1,
				..Options::default()
			});
			assert_eq!(text(&progress.bar("")), format!("{symbol} "));
		}
	}

	/// `advance` with no message of its own keeps the one before it; `message` is an advance of
	/// nothing.
	#[test]
	fn the_message_is_kept_between_advances() {
		let mut progress = progress(Options {
			size: 2,
			max: 2,
			..Options::default()
		});
		progress.start("first");
		progress.advance(1, None);
		assert_eq!(text(&progress.bar(&progress.previous_message)), "━━ first");
		progress.set_message("second");
		assert_eq!(progress.value, 1, "a message advances nothing");
		assert_eq!(text(&progress.bar(&progress.previous_message)), "━━ second");
		assert_eq!(
			progress.bar(&progress.previous_message).spans[0].text,
			"━",
			"and the bar is where it was"
		);
	}

	/// The bar is in the message, so it is drawn by the tick and not by `advance`.
	#[test]
	fn the_bar_reaches_the_terminal_on_a_tick() {
		let mut progress = progress(Options {
			size: 2,
			max: 2,
			..Options::default()
		});
		progress.start("go");
		progress.advance(1, None);
		let row = progress.tick(Duration::ZERO);
		assert!(row.contains("go"), "{row:?}");
	}
}
