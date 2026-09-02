//! Ported from `@clack/prompts`' `task-log.ts` — a log that clears on success and stays on failure.
//!
//! The third kind of renderer in this crate. A Prompt diffs Frames through [`crate::emitter`]; a
//! [`spinner`](crate::spinner) walks back over the one row it wrote; a task log writes whole `log`
//! calls, remembers the *text* it wrote, and erases as many rows as that text was worth the next
//! time anything changes. Nothing here draws a Frame it has drawn before — every write is a fresh
//! [`crate::message::log`], and the erase in front of it is counted rather than diffed.
//!
//! Like the spinner, this module returns bytes and does no I/O.
//!
//! # The row count is a string length, not a width
//!
//! `Math.ceil((line.length + barSize) / columns)` is how many rows a line is assumed to have taken.
//! `line.length` is a count of UTF-16 code units, so a wide character counts one where the terminal
//! gave it two, an emoji counts two where the terminal gave it two but for the other reason, and a
//! message with an SGR escape still in it counts every character of the escape. `barSize` is the
//! three columns `│  ` takes. All of it is reproduced (ADR-0013): the erase is one of the few things
//! in clack a terminal can be left visibly wrong by, and it is wrong here for messages that are
//! perfectly ordinary.
//!
//! # The erase is written whether or not anything was drawn
//!
//! `message()` clears before it appends, and only *prints* when the output is a TTY. In CI the
//! printing is skipped and the clearing is not — so the second message of a CI run erases rows that
//! belong to the title, and the third erases whatever is above that. Upstream's own snapshots
//! record it. Reproduced, and the corpus has it.
//!
//! # An empty row keeps its bar and its two spaces
//!
//! Everything a task log prints goes through `log.message`'s `string[]` overload with every row
//! already styled — `styleText('dim', line)`. `log.message` drops a row's prefix when the row is
//! empty, and a dim-styled empty string is not empty: it is four escape characters. So a blank line
//! inside a task log is `│  ` where a blank line inside a plain `log` is `│`. See
//! [`crate::message::log_lines`], which is that overload.
//!
//! # The `withGuide` a task log takes is not the one it draws with
//!
//! `taskLog` accepts a `withGuide` — it is in `CommonOptions` — and never passes it to any of the
//! `log.message` calls it makes. So the option does nothing, and what those calls read is the
//! global `settings.withGuide`. [`Options::with_guide`] here is therefore the *global*, which is
//! what the driver hands it, and a caller's own `withGuide` has nowhere to go. The corpus records
//! both: a script that sets the option and keeps its bars, and scripts that turn the global off.
//!
//! The title is not affected either way — those three writes do not go through `log.message`.
//!
//! # One divergence: an escape a message smuggles past the strip
//!
//! `stripDestructiveANSI` takes out the escapes that move or erase and leaves the rest, so a
//! message with colour in it reaches the terminal coloured. A [`Frame`] carries no escapes
//! (ADR-0011) and the renderer drops any it is handed, so here it reaches the terminal as nothing
//! at all: the characters of the sequence are neither drawn nor obeyed. This is the only place in
//! the crate where clack can be handed something this port cannot say, and it is deliberate — the
//! way to colour a task log's message is to have a [`Line`] of it, which is what
//! [`crate::message::log_lines`] already takes. There is no case in the corpus for it, because a
//! recording of it would be a recording of a disagreement.
//!
//! # `success` hides the log and `error` shows it
//!
//! Both take the same `showLog` option and read it differently: `error` renders the buffer unless it
//! is `false`, `success` renders it only if it is `true`. Two defaults, one name.

use crate::emitter::{erase_lines, write_once};
use crate::frame::{Frame, Line, Span};
use crate::message::{log, log_lines};
use crate::theme::Theme;
use ratatui_core::style::{Modifier, Style};

/// `barSize`: the three columns `│  ` takes, added to every line before it is divided by the width.
const BAR_SIZE: usize = 3;

/// How a task log, or one group in it, ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
	Success,
	Error,
}

pub struct Options {
	pub title: String,
	/// The most rows kept. Older ones are dropped, or moved to the retained log.
	pub limit: Option<usize>,
	/// Rows above the title and above each printed message. Upstream's default is one.
	pub spacing: usize,
	/// Keep the rows `limit` drops, and print them when the log is shown.
	pub retain_log: bool,
	/// `settings.withGuide`, and not the `withGuide` a caller passes to `taskLog` — see the module
	/// docs. It reaches the messages and never the title.
	pub with_guide: bool,
	/// `!isCI() && isTTY(output)`, which the driver computes. False means nothing is drawn between
	/// the title and the ending — but see the module docs: the erases are still written.
	pub is_tty: bool,
}

impl Default for Options {
	fn default() -> Self {
		Self {
			title: String::new(),
			limit: None,
			spacing: 1,
			retain_log: false,
			with_guide: true,
			is_tty: true,
		}
	}
}

/// A group made by [`TaskLog::group`].
///
/// Upstream returns an object closing over the buffer; this is its index. The difference shows in
/// one place only: an ending drops every group, so a `GroupId` used after one names a buffer that is
/// no longer there. Upstream keeps writing to the detached object and never prints it again, which
/// is the same nothing — see [`TaskLog::group_message`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupId(usize);

#[derive(Default)]
struct Buffer {
	/// A group's name. Empty for the buffer every task log starts with, which has none.
	header: String,
	value: String,
	/// What `limit` dropped, kept only when `retainLog` is set.
	full: String,
	result: Option<(Outcome, String)>,
}

pub struct TaskLog {
	theme: Theme,
	columns: usize,
	options: Options,
	/// Never empty: the first is the one `message()` writes to and the rest are groups.
	buffers: Vec<Buffer>,
	last_message_was_raw: bool,
}

impl TaskLog {
	/// The title, and the bars around it — which upstream writes from the constructor, so this
	/// returns them.
	///
	/// `withGuide` is not consulted here. These three writes do not go through `log.message`, so a
	/// task log opens with a bar and a symbol whether or not the Guide is on, and only its messages
	/// lose theirs.
	pub fn new(theme: Theme, columns: usize, options: Options) -> (Self, String) {
		let mut frame = Frame::new();
		frame.push(Line::from(Span::styled(
			theme.symbols.bar,
			theme.styles.guide,
		)));
		frame.push(Line::from_iter([
			Span::styled(theme.symbols.step_submit, theme.styles.log_step),
			Span::raw("  "),
			Span::raw(options.title.clone()),
		]));
		for _ in 0..options.spacing {
			frame.push(Line::from(Span::styled(
				theme.symbols.bar,
				theme.styles.guide,
			)));
		}
		let out = write_once(&frame);

		let log = Self {
			theme,
			columns,
			options,
			buffers: vec![Buffer::default()],
			last_message_was_raw: false,
		};
		(log, out)
	}

	/// A line of output. `raw` is upstream's `{ raw: true }`: two raw messages in a row are joined
	/// without a newline between them, which is how a stream that arrives in pieces is logged.
	pub fn message(&mut self, message: &str, raw: bool) -> String {
		self.write_message(0, message, raw)
	}

	/// A named group, printed above its own messages in bold.
	pub fn group(&mut self, name: &str) -> GroupId {
		self.buffers.push(Buffer {
			header: name.to_owned(),
			..Buffer::default()
		});
		GroupId(self.buffers.len() - 1)
	}

	/// A line of output inside a group. A `GroupId` from before an ending writes the erase and the
	/// reprint and records nothing, per the type's docs.
	pub fn group_message(&mut self, group: GroupId, message: &str, raw: bool) -> String {
		self.write_message(group.0, message, raw)
	}

	/// A group's ending: its messages are replaced by one line with a symbol.
	pub fn complete_group(&mut self, group: GroupId, outcome: Outcome, message: &str) -> String {
		let mut out = self.clear(false);
		if let Some(buffer) = self.buffers.get_mut(group.0) {
			buffer.result = Some((outcome, message.to_owned()));
		}
		if self.options.is_tty {
			out.push_str(&self.print_buffers());
		}
		out
	}

	/// The whole log's ending. `show_log` defaults to *true* here and *false* in
	/// [`success`](Self::success) — see the module docs.
	pub fn error(&mut self, message: &str, show_log: bool) -> String {
		self.complete(Outcome::Error, message, show_log)
	}

	pub fn success(&mut self, message: &str, show_log: bool) -> String {
		self.complete(Outcome::Success, message, show_log)
	}

	fn complete(&mut self, outcome: Outcome, message: &str, show_log: bool) -> String {
		let mut out = self.clear(true);
		out.push_str(&self.completion(outcome, message, 1));
		if show_log {
			out.push_str(&self.render_buffer());
		}
		// An ending is an ending: the groups go and the first buffer is emptied.
		self.buffers.truncate(1);
		self.buffers[0].value.clear();
		self.buffers[0].full.clear();
		out
	}

	fn write_message(&mut self, index: usize, message: &str, raw: bool) -> String {
		let mut out = self.clear(false);

		let limit = self.options.limit;
		let retain_log = self.options.retain_log;
		let last_was_raw = self.last_message_was_raw;
		if let Some(buffer) = self.buffers.get_mut(index) {
			// A raw message that follows a raw message is appended to the row it is on.
			if (!raw || !last_was_raw) && !buffer.value.is_empty() {
				buffer.value.push('\n');
			}
			buffer.value.push_str(&strip_destructive_ansi(message));

			if let Some(limit) = limit {
				let lines: Vec<&str> = buffer.value.split('\n').collect();
				let to_remove = lines.len().saturating_sub(limit);
				let kept = lines[to_remove..].join("\n");
				let removed = lines[..to_remove].join("\n");
				if to_remove > 0 && retain_log {
					if !buffer.full.is_empty() {
						buffer.full.push('\n');
					}
					buffer.full.push_str(&removed);
				}
				buffer.value = kept;
			}
		}
		self.last_message_was_raw = raw;

		if self.options.is_tty {
			out.push_str(&self.print_buffers());
		}
		out
	}

	/// `clear`: how many rows everything currently held was worth, and the escape that erases them.
	///
	/// The count is of what *would* have been printed, which in CI is nothing. See the module docs.
	fn clear(&self, clear_title: bool) -> String {
		let mut lines = 0;

		if clear_title {
			lines += self.options.spacing + 2;
		}

		for buffer in &self.buffers {
			let mut text = match &buffer.result {
				Some((_, message)) => message.clone(),
				None => buffer.value.clone(),
			};
			if text.is_empty() {
				continue;
			}
			// A group that has not ended is printed under its name, so the name is a row too. Where
			// it goes does not matter to a count, and upstream puts it last.
			if buffer.result.is_none() && !buffer.header.is_empty() {
				text.push('\n');
				text.push_str(&buffer.header);
			}
			lines += text
				.split('\n')
				.map(|line| rows_of(line, self.columns))
				.sum::<usize>();
		}

		if lines == 0 {
			return String::new();
		}
		// The extra row is the one the cursor is sitting on, below everything that was written.
		erase_lines(lines + 1)
	}

	/// Everything held, printed tight — what a TTY sees after every message.
	fn print_buffers(&self) -> String {
		let mut out = String::new();
		for buffer in &self.buffers {
			if let Some((outcome, message)) = &buffer.result {
				out.push_str(&self.completion(*outcome, message, 0));
			} else if !buffer.value.is_empty() {
				out.push_str(&self.print_buffer(buffer, 0, false));
			}
		}
		out
	}

	/// Everything held, printed with the spacing an ending uses — `renderBuffer`.
	fn render_buffer(&self) -> String {
		let mut out = String::new();
		for buffer in &self.buffers {
			if buffer.header.is_empty() && buffer.value.is_empty() {
				continue;
			}
			let full = self.options.retain_log && !buffer.full.is_empty();
			out.push_str(&self.print_buffer(buffer, self.options.spacing, full));
		}
		out
	}

	/// One buffer: its name in bold, then its rows dimmed.
	fn print_buffer(&self, buffer: &Buffer, spacing: usize, full: bool) -> String {
		let mut out = String::new();
		if !buffer.header.is_empty() {
			let header = styled(&buffer.header, Style::new().add_modifier(Modifier::BOLD));
			out.push_str(&write_once(&log_lines(
				&header,
				self.bar(),
				self.bar(),
				0,
				self.options.with_guide,
			)));
		}
		let messages = if full {
			format!("{}\n{}", buffer.full, buffer.value)
		} else {
			buffer.value.clone()
		};
		let messages = styled(&messages, Style::new().add_modifier(Modifier::DIM));
		out.push_str(&write_once(&log_lines(
			&messages,
			self.bar(),
			self.bar(),
			spacing,
			self.options.with_guide,
		)));
		out
	}

	/// `log.success` or `log.error` — the one line an ending leaves behind.
	fn completion(&self, outcome: Outcome, message: &str, spacing: usize) -> String {
		let (symbol, style) = match outcome {
			Outcome::Success => (self.theme.symbols.success, self.theme.styles.log_success),
			Outcome::Error => (self.theme.symbols.error, self.theme.styles.log_error),
		};
		write_once(&log(
			message,
			Span::styled(symbol, style),
			self.bar(),
			spacing,
			self.options.with_guide,
		))
	}

	fn bar(&self) -> Span {
		Span::styled(self.theme.symbols.bar, self.theme.styles.guide)
	}
}

/// Every row of `text` as one styled [`Line`] — empty rows included, which is the point. See the
/// module docs.
fn styled(text: &str, style: Style) -> Vec<Line> {
	text.split('\n')
		.map(|line| Line::from(Span::styled(line, style)))
		.collect()
}

/// `line === '' ? 1 : Math.ceil((line.length + barSize) / columns)`, in UTF-16 code units because
/// that is what `String.length` counts. See the module docs.
fn rows_of(line: &str, columns: usize) -> usize {
	if line.is_empty() {
		return 1;
	}
	(line.encode_utf16().count() + BAR_SIZE).div_ceil(columns.max(1))
}

/// The terminators `stripDestructiveANSI` matches: cursor movement, erasing, scrolling, and the
/// save/restore pair.
const TERMINATORS: &[u8] = b"ABCDEFGHfJKSTsu";

/// `stripDestructiveANSI`: the escapes that would move or erase, taken out of a message.
///
/// The expression has a second alternative, `\x1b\[(s|u)`, that the first already covers —
/// `(?:\d+;)*\d*` matches nothing at all quite happily — so it never fires. What is *not* in the
/// terminator set is `m`, so an SGR escape survives; a message with colour in it therefore reaches
/// a Line as text, and is drawn as the characters it is rather than as the colour it meant. That is
/// ADR-0011's price and this is the one place a caller can pay it.
fn strip_destructive_ansi(input: &str) -> String {
	let bytes = input.as_bytes();
	let mut out = String::new();
	let mut kept = 0;
	let mut at = 0;
	while at < bytes.len() {
		if bytes[at] == 0x1b
			&& bytes.get(at + 1) == Some(&b'[')
			&& let Some(length) = escape_length(&bytes[at..])
		{
			out.push_str(&input[kept..at]);
			at += length;
			kept = at;
			continue;
		}
		at += 1;
	}
	out.push_str(&input[kept..]);
	out
}

/// The length of the escape at the front of `bytes`, if it is one of the destructive ones.
///
/// `(?:\d+;)*\d*` and then a terminator. Digits, semicolons and terminators are three disjoint sets,
/// so there is nothing here for a backtracking engine to do differently.
fn escape_length(bytes: &[u8]) -> Option<usize> {
	let mut at = 2;
	loop {
		let digits = bytes[at..]
			.iter()
			.take_while(|byte| byte.is_ascii_digit())
			.count();
		if digits > 0 && bytes.get(at + digits) == Some(&b';') {
			at += digits + 1;
			continue;
		}
		at += digits;
		break;
	}
	match bytes.get(at) {
		Some(byte) if TERMINATORS.contains(byte) => Some(at + 1),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn task_log(options: Options) -> (TaskLog, String) {
		TaskLog::new(Theme::clack(), 80, options)
	}

	fn opened(title: &str) -> (TaskLog, String) {
		task_log(Options {
			title: title.to_owned(),
			..Options::default()
		})
	}

	#[test]
	fn the_title_is_written_between_bars() {
		let (_, out) = opened("build");
		assert_eq!(strip(&out), "│\n◇  build\n│\n");
	}

	/// The one divergence, stated as a test so that it is a decision and not a surprise. See the
	/// module docs.
	#[test]
	fn an_escape_the_strip_leaves_behind_is_dropped_by_the_frame() {
		let (mut log, _) = opened("build");
		let out = log.message("one\u{1b}[31mtwo", false);
		assert!(!out.contains("31m"), "{out:?}");
		assert_eq!(strip(&out), "│  onetwo\n");
	}

	#[test]
	fn the_title_is_written_with_the_guide_off_globally() {
		let (_, out) = task_log(Options {
			title: "build".to_owned(),
			with_guide: false,
			..Options::default()
		});
		assert_eq!(strip(&out), "│\n◇  build\n│\n");
	}

	#[test]
	fn the_first_message_has_nothing_to_erase() {
		let (mut log, _) = opened("build");
		assert_eq!(strip(&log.message("one", false)), "│  one\n");
	}

	#[test]
	fn the_second_message_erases_the_first_and_reprints_both() {
		let (mut log, _) = opened("build");
		log.message("one", false);
		// One row held, plus the row the cursor is on.
		assert_eq!(
			strip(&log.message("two", false)),
			format!("{}│  one\n│  two\n", erase_lines(2))
		);
	}

	/// The empty row between them keeps its bar *and* its two spaces, because it is styled.
	#[test]
	fn a_blank_row_inside_a_message_keeps_its_two_spaces() {
		let (mut log, _) = opened("build");
		assert_eq!(
			strip(&log.message("one\n\ntwo", false)),
			"│  one\n│  \n│  two\n"
		);
	}

	#[test]
	fn a_raw_message_after_a_raw_message_is_appended_to_the_row() {
		let (mut log, _) = opened("build");
		log.message("one", true);
		let out = strip(&log.message("two", true));
		assert!(out.ends_with("│  onetwo\n"), "{out}");
	}

	#[test]
	fn a_raw_message_after_a_plain_one_still_starts_a_row() {
		let (mut log, _) = opened("build");
		log.message("one", false);
		let out = strip(&log.message("two", true));
		assert!(out.ends_with("│  one\n│  two\n"), "{out}");
	}

	#[test]
	fn a_limit_drops_the_oldest_rows() {
		let (mut log, _) = task_log(Options {
			title: "build".to_owned(),
			limit: Some(2),
			..Options::default()
		});
		log.message("one", false);
		log.message("two", false);
		let out = strip(&log.message("three", false));
		assert!(out.ends_with("│  two\n│  three\n"), "{out}");
	}

	#[test]
	fn a_retained_log_keeps_what_the_limit_dropped_and_shows_it_at_the_end() {
		let (mut log, _) = task_log(Options {
			title: "build".to_owned(),
			limit: Some(1),
			retain_log: true,
			..Options::default()
		});
		log.message("one", false);
		log.message("two", false);
		let out = strip(&log.success("done", true));
		assert!(out.contains("◆  done"), "{out}");
		assert!(out.ends_with("│\n│  one\n│  two\n"), "{out}");
	}

	/// The two endings read the same option with opposite defaults; the driver supplies them, so
	/// what is checked here is that each one obeys what it is given.
	#[test]
	fn an_ending_shows_the_log_only_when_it_is_asked_to() {
		let (mut log, _) = opened("build");
		log.message("one", false);
		assert!(!strip(&log.success("finished", false)).contains("one"));

		let (mut log, _) = opened("build");
		log.message("one", false);
		assert!(strip(&log.error("failed", true)).contains("one"));
	}

	#[test]
	fn an_ending_empties_the_log() {
		let (mut log, _) = opened("build");
		log.message("one", false);
		log.success("done", false);
		// Nothing is held, so the next message has only the cursor's own row to erase — nothing.
		assert_eq!(strip(&log.message("two", false)), "│  two\n");
	}

	#[test]
	fn a_group_is_printed_under_its_name() {
		let (mut log, _) = opened("build");
		let group = log.group("install");
		let out = strip(&log.group_message(group, "one", false));
		assert_eq!(out, "│  install\n│  one\n");
	}

	#[test]
	fn a_completed_group_is_one_line_with_a_symbol() {
		let (mut log, _) = opened("build");
		let group = log.group("install");
		log.group_message(group, "one", false);
		let out = strip(&log.complete_group(group, Outcome::Error, "install failed"));
		assert!(out.ends_with("■  install failed\n"), "{out}");
	}

	/// In CI nothing is printed and everything is still erased. Two messages, and the second one
	/// walks over rows that belong to the title.
	#[test]
	fn in_ci_the_erase_is_written_and_the_message_is_not() {
		let (mut log, _) = task_log(Options {
			title: "build".to_owned(),
			is_tty: false,
			..Options::default()
		});
		assert_eq!(log.message("one", false), "");
		assert_eq!(log.message("two", false), erase_lines(2));
	}

	#[test]
	fn a_row_is_counted_by_its_length_plus_the_bar() {
		// Seventy-seven characters and three for the bar is exactly one row of eighty.
		assert_eq!(rows_of(&"x".repeat(77), 80), 1);
		assert_eq!(rows_of(&"x".repeat(78), 80), 2);
		// A wide character is one UTF-16 unit and two columns; upstream counts the unit.
		assert_eq!(rows_of(&"ば".repeat(77), 80), 1);
		// An astral character is two units and two columns, and here the two agree by accident.
		assert_eq!(rows_of(&"🎉".repeat(38), 80), 1);
		assert_eq!(rows_of("", 80), 1);
	}

	#[test]
	fn the_destructive_escapes_are_the_ones_that_go() {
		assert_eq!(strip_destructive_ansi("a\u{1b}[2Jb"), "ab");
		assert_eq!(strip_destructive_ansi("a\u{1b}[1;2Hb"), "ab");
		assert_eq!(strip_destructive_ansi("a\u{1b}[sb\u{1b}[u"), "ab");
		// Colour is not destructive and stays — as characters, per the module docs.
		assert_eq!(strip_destructive_ansi("a\u{1b}[31mb"), "a\u{1b}[31mb");
		// Not an escape this expression recognises, so every character of it is kept.
		assert_eq!(strip_destructive_ansi("a\u{1b}[1;b"), "a\u{1b}[1;b");
		assert_eq!(strip_destructive_ansi("a\u{1b}["), "a\u{1b}[");
		// Multibyte text either side of one that goes.
		assert_eq!(strip_destructive_ansi("ばん\u{1b}[2Kは"), "ばんは");
	}

	/// The bytes carry SGR; these tests are about the characters.
	fn strip(bytes: &str) -> String {
		let mut out = String::new();
		let mut chars = bytes.chars();
		while let Some(c) = chars.next() {
			if c == '\u{1b}' {
				let mut escape = String::new();
				for c in chars.by_ref() {
					escape.push(c);
					if c.is_ascii_alphabetic() {
						break;
					}
				}
				// Only SGR goes; the erases are what these tests are checking.
				if !escape.ends_with('m') {
					out.push('\u{1b}');
					out.push_str(&escape);
				}
			} else {
				out.push(c);
			}
		}
		out
	}
}
