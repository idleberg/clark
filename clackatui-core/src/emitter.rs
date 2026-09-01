//! The Emitter: a sequence of Frames turned into terminal writes.
//!
//! This is a port of `@clack/core`'s `Prompt.render`, not an independent reconciler, and the
//! difference matters more than it looks. ADR-0002 assumed the Emitter would consume
//! `Buffer::diff_iter` and turn cell updates into cursor movement. Upstream does nothing of the
//! kind: it compares the two Frames **line by line** and rewrites whole lines, and it derives every
//! cursor movement from line counts. Since the cursor is part of the Grid a parity claim is made
//! about (ADR-0001), reproducing the branch that clack took is not an implementation detail — see
//! ADR-0013.
//!
//! The algorithm, given the previous Frame's rows and the new ones:
//!
//! 1. Identical? Write nothing at all, and stay in whatever state you were in.
//! 2. First Frame? Hide the cursor and write the whole thing.
//! 3. Otherwise walk the cursor back to the top-left of the previous Frame, then:
//!    - exactly one row differs — step down to it, erase it, write it, step back down to the
//!      bottom;
//!    - more than one differs — step down to the first, erase everything below, and write every
//!      row from there on.
//!
//! The two step-downs end the cursor in different *columns* — the end of the changed row in the
//! first case, the end of the last row in the second — which is why the branch has to be the same
//! branch and not merely an equivalent repaint.
//!
//! # Rows, not cells
//!
//! The diff is over [`frame::Row`]s rather than `Buffer` rows. A `Buffer` row is padded to the
//! terminal width, so a line ending in a space and a line ending where the padding begins are the
//! same row in a `Buffer` and different strings to clack. Diffing the Frame's own rows keeps the
//! comparison exactly as discriminating as upstream's.
//!
//! One consequence is that `CellDiffOption::ForcedWidth` — M0's whole subject — is not on this
//! path. It still governs the `Widget` impl, which is what makes a clackatui Prompt drawable inside
//! someone else's Ratatui application (ADR-0002), and the shrink-repaint gap ADR-0007 found is
//! simply not reachable here: clack erases a row before rewriting it, so no column is ever left
//! holding an older, wider glyph.
//!
//! # Styling
//!
//! clack's Frames arrive at the writer as picocolors output — escapes already in the string. Here a
//! Frame carries a [`Style`] per span (ADR-0011) and this module states it per cell, resetting at
//! the end of every row so that an erase never paints with a background someone else set. The
//! resulting bytes are not picocolors' bytes, and are not meant to be: the claim is about the Grid
//! an emulator arrives at, which is why the conformance corpus for this module is deliberately
//! colourless.

use ratatui_core::style::{Color, Modifier, Style};

use crate::frame::{Frame, Row};
use crate::wrap::breaks;

const CSI: &str = "\u{1b}[";

/// What upstream writes when it is asked for a row that is not there.
///
/// Not a placeholder: it is the literal string clack puts on the wire. `lines[diffLine]` is
/// `undefined` whenever a Frame loses exactly its last row, and `output.write` stringifies it. See
/// ADR-0013 — this is reproduced on purpose, and it is an upstream defect.
const MISSING_ROW: &str = "undefined";

/// A Frame written once, for the renderers that never draw a second one.
///
/// `log`, `intro`, `outro`, `cancel` and the rest of `@clack/prompts`' static output do not go
/// through `Prompt.render` at all: they build a string and hand it to `output.write`. There is no
/// previous Frame to diff against, no cursor to hide, and — the part that matters — no wrap. clack
/// wraps a Prompt's Frame itself so it can count the rows it walks back over (ADR-0012); nothing
/// walks back over these, so a long line is left for the terminal to soft-wrap exactly as upstream
/// leaves it.
///
/// Rows joined with `\n`, and one `\n` after the last, which is the newline every one of those
/// writes ends with. A trailing blank row is therefore how a blank line after the output is written
/// down — see [`crate::message`].
pub fn write_once(frame: &Frame) -> String {
	let mut out = write_rows(&frame.rows(u16::MAX), 0);
	out.push('\n');
	out
}

/// Turns Frames into terminal writes, holding the previous Frame between calls.
///
/// Produces the bytes rather than writing them, so that `clackatui-core` stays free of I/O. The
/// driver in `clackatui` is what puts them on a terminal.
#[derive(Clone, Debug)]
pub struct Emitter {
	/// The previous Frame, laid out. Starts as one empty row, because upstream's `_prevFrame`
	/// starts as `''` — which is one empty line, not no lines.
	previous: Vec<Row>,
	/// Whether the opening Frame is still to come. Upstream reads `state === 'initial'`, which the
	/// first Frame that actually writes clears; a Frame identical to `''` therefore leaves it set.
	initial: bool,
}

impl Default for Emitter {
	fn default() -> Self {
		Self::new()
	}
}

impl Emitter {
	pub fn new() -> Self {
		Self {
			previous: vec![Row::new()],
			initial: true,
		}
	}

	/// The bytes that move the terminal from the previous Frame to this one.
	///
	/// `columns` is the width the Frame is wrapped to and `rows` the terminal's height. They are
	/// two parameters because upstream reads them from two places: the wrap width comes from the
	/// global `process.stdout.columns` and the height from the Prompt's own output stream. A Prompt
	/// given a non-terminal output therefore wraps to the real terminal's width and reasons about a
	/// height of 20.
	pub fn frame(&mut self, frame: &Frame, columns: u16, rows: u16) -> String {
		let next = frame.rows(columns);
		if next == self.previous {
			return String::new();
		}

		let mut out = String::new();

		if self.initial {
			out.push_str(&hide_cursor());
			out.push_str(&write_rows(&next, 0));
			self.previous = next;
			self.initial = false;
			return out;
		}

		// Back to where the previous Frame began. Column -999 rather than column 0 is upstream's:
		// `cursor.move` emits a relative `CUB`, and 999 is further left than any terminal is wide.
		//
		// The row count is upstream's `restoreCursor`, which is not `before`. It re-wraps the
		// previous Frame at the terminal's *current* width, while the diff below splits it at the
		// newlines it was written with. The two are the same number unless the terminal narrowed
		// between the Frames, and when it has, upstream walks the cursor further up than it drew.
		// See ADR-0017: this is reproduced because the terminal can see it.
		let before = self.previous.len();
		let after = next.len();
		out.push_str(&cursor_move(-999, -(self.restored(columns) as i64 - 1)));

		let changed: Vec<usize> = (0..before.max(after))
			.filter(|&index| self.previous.get(index) != next.get(index))
			.collect();

		// `changed` cannot be empty: the Frames differ, so some row does. Upstream's fallback for
		// an empty diff — erase down and rewrite everything — is therefore unreachable, and is left
		// out rather than carried as a branch no test can enter.
		let offset_after = after.saturating_sub(rows as usize);
		let offset_before = before.saturating_sub(rows as usize);

		let Some(mut first) = changed.iter().copied().find(|&row| row >= offset_after) else {
			// Everything that changed has already scrolled off the top. Upstream returns here
			// having walked the cursor back and written nothing else, which leaves the cursor at
			// the top of the Frame rather than the bottom.
			self.previous = next;
			return out;
		};

		if changed.len() == 1 {
			out.push_str(&cursor_move(0, first as i64 - offset_before as i64));
			out.push_str(&erase_row());
			out.push_str(&write_row(next.get(first)));
			out.push_str(&cursor_move(0, after as i64 - first as i64 - 1));
			self.previous = next;
			return out;
		}

		if offset_after < offset_before {
			first = offset_after;
		} else {
			let adjusted = first as i64 - offset_before as i64;
			if adjusted > 0 {
				out.push_str(&cursor_move(0, adjusted));
			}
		}
		out.push_str(&erase_below());
		out.push_str(&write_rows(&next, first));
		self.previous = next;
		out
	}

	/// How many rows upstream's `restoreCursor` walks the cursor back over.
	///
	/// Upstream keeps the previous Frame as the wrapped *string* it wrote, and re-wraps that string
	/// at whatever the terminal is now. A row that already fits cannot wrap again at the same width
	/// or a greater one, so this is `self.previous.len()` in every case but one: the terminal
	/// narrowed since the previous Frame, and rows laid out for the old width now need more than
	/// one each.
	///
	/// Whether that is *right* is a separate question — it walks back over rows the terminal has
	/// re-flowed rather than over the rows that are there — and not one this port gets to answer.
	fn restored(&self, columns: u16) -> usize {
		self.previous
			.iter()
			.map(|row| breaks(&row_text(row), columns as usize).len() + 1)
			.sum()
	}

	/// `cursor.move(0, -1)`: one row up and nothing else.
	///
	/// Not part of drawing a Frame. `ConfirmPrompt` writes it straight to the output from inside its
	/// `confirm` listener, before anything has been re-drawn — ADR-0018.
	pub fn cursor_up(&self) -> String {
		cursor_move(0, -1)
	}

	/// The newline upstream writes when a Prompt closes, leaving the Frame in the scrollback.
	pub fn finish(&self) -> String {
		"\n".to_owned()
	}

	/// Shows the cursor again. Not part of `render` — upstream hides the cursor in the Emitter and
	/// restores it from the surrounding program — but it belongs to whoever owns the hiding.
	pub fn show_cursor(&self) -> String {
		format!("{CSI}?25h")
	}
}

/// A laid-out row back as the text it was laid out from, for measuring it again at another width.
fn row_text(row: &Row) -> String {
	row.iter().map(|placed| placed.symbol.as_str()).collect()
}

/// Every row from `first` on, joined the way upstream joins them.
fn write_rows(rows: &[Row], first: usize) -> String {
	let mut out = String::new();
	for (index, row) in rows.iter().skip(first).enumerate() {
		if index != 0 {
			out.push('\n');
		}
		out.push_str(&write_row(Some(row)));
	}
	out
}

/// One row's worth of bytes: the units, with a style stated whenever it changes.
///
/// Ends at the default style, so that an erase that follows paints with the terminal's own
/// background rather than the last cell's.
fn write_row(row: Option<&Row>) -> String {
	let Some(row) = row else {
		return MISSING_ROW.to_owned();
	};

	let mut out = String::new();
	let mut current = Style::default();
	for unit in row {
		if unit.style != current {
			out.push_str(&sgr(unit.style));
			current = unit.style;
		}
		out.push_str(&unit.symbol);
	}
	if current != Style::default() {
		out.push_str(&sgr(Style::default()));
	}
	out
}

/// A style, stated from scratch. Always resets first, so that no attribute survives from the cell
/// before by accident — a Frame's spans say what they are, never what to add to what came before.
fn sgr(style: Style) -> String {
	let mut codes = vec![0u16];
	let modifier = style.add_modifier;
	for (bit, code) in [
		(Modifier::BOLD, 1),
		(Modifier::DIM, 2),
		(Modifier::ITALIC, 3),
		(Modifier::UNDERLINED, 4),
		(Modifier::SLOW_BLINK, 5),
		(Modifier::RAPID_BLINK, 6),
		(Modifier::REVERSED, 7),
		(Modifier::HIDDEN, 8),
		(Modifier::CROSSED_OUT, 9),
	] {
		if modifier.contains(bit) {
			codes.push(code);
		}
	}

	let mut out = String::from(CSI);
	for (index, code) in codes.iter().enumerate() {
		if index != 0 {
			out.push(';');
		}
		out.push_str(&code.to_string());
	}
	if let Some(color) = style.fg {
		out.push(';');
		out.push_str(&color_codes(color, false));
	}
	if let Some(color) = style.bg {
		out.push(';');
		out.push_str(&color_codes(color, true));
	}
	out.push('m');
	out
}

/// The SGR parameters for one colour, foreground or background.
fn color_codes(color: Color, background: bool) -> String {
	let shift = u16::from(background) * 10;
	let base = |code: u16| (code + shift).to_string();
	match color {
		Color::Reset => base(39),
		Color::Black => base(30),
		Color::Red => base(31),
		Color::Green => base(32),
		Color::Yellow => base(33),
		Color::Blue => base(34),
		Color::Magenta => base(35),
		Color::Cyan => base(36),
		Color::Gray => base(37),
		Color::DarkGray => base(90),
		Color::LightRed => base(91),
		Color::LightGreen => base(92),
		Color::LightYellow => base(93),
		Color::LightBlue => base(94),
		Color::LightMagenta => base(95),
		Color::LightCyan => base(96),
		Color::White => base(97),
		Color::Indexed(index) => format!("{};5;{index}", base(38)),
		Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", base(38)),
	}
}

/// `sisteransi`'s `cursor.move`: horizontal first, then vertical, each omitted when zero.
fn cursor_move(x: i64, y: i64) -> String {
	let mut out = String::new();
	match x.signum() {
		-1 => out.push_str(&format!("{CSI}{}D", -x)),
		1 => out.push_str(&format!("{CSI}{x}C")),
		_ => {}
	}
	match y.signum() {
		-1 => out.push_str(&format!("{CSI}{}A", -y)),
		1 => out.push_str(&format!("{CSI}{y}B")),
		_ => {}
	}
	out
}

/// `sisteransi`'s `erase.lines(1)`: erase the row, then return to its first column.
fn erase_row() -> String {
	format!("{CSI}2K{CSI}G")
}

/// `sisteransi`'s `erase.down()`.
fn erase_below() -> String {
	format!("{CSI}J")
}

fn hide_cursor() -> String {
	format!("{CSI}?25l")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::frame::{Line, Span};

	/// A Frame from text, the way the fixtures describe one: a line per `\n`, no styling.
	fn frame(text: &str) -> Frame {
		Frame {
			lines: text
				.split('\n')
				.map(|line| Line::from(Span::raw(line)))
				.collect(),
		}
	}

	#[test]
	fn the_first_frame_hides_the_cursor_and_is_written_whole() {
		let mut emitter = Emitter::new();
		assert_eq!(emitter.frame(&frame("a\nb"), 20, 10), "\u{1b}[?25la\nb");
	}

	#[test]
	fn an_unchanged_frame_is_not_written_at_all() {
		let mut emitter = Emitter::new();
		emitter.frame(&frame("a\nb"), 20, 10);
		assert_eq!(emitter.frame(&frame("a\nb"), 20, 10), "");
	}

	#[test]
	fn an_empty_opening_frame_leaves_the_cursor_shown() {
		// `_prevFrame` starts as the empty string, so an opening Frame of one blank line is not a
		// change and the Prompt stays `initial`. The *next* Frame is the one that hides the cursor.
		let mut emitter = Emitter::new();
		assert_eq!(emitter.frame(&frame(""), 20, 10), "");
		assert!(
			emitter
				.frame(&frame("a"), 20, 10)
				.starts_with("\u{1b}[?25l")
		);
	}

	#[test]
	fn one_changed_row_is_erased_and_rewritten_in_place() {
		let mut emitter = Emitter::new();
		emitter.frame(&frame("a\nb\nc"), 20, 10);
		assert_eq!(
			emitter.frame(&frame("a\nB\nc"), 20, 10),
			"\u{1b}[999D\u{1b}[2A\u{1b}[1B\u{1b}[2K\u{1b}[GB\u{1b}[1B"
		);
	}

	#[test]
	fn two_changed_rows_erase_everything_below_the_first() {
		let mut emitter = Emitter::new();
		emitter.frame(&frame("a\nb\nc"), 20, 10);
		assert_eq!(
			emitter.frame(&frame("A\nb\nC"), 20, 10),
			"\u{1b}[999D\u{1b}[2A\u{1b}[JA\nb\nC"
		);
	}

	#[test]
	fn losing_the_last_row_writes_the_word_undefined() {
		// Upstream's defect, reproduced deliberately. See ADR-0013.
		let mut emitter = Emitter::new();
		emitter.frame(&frame("a\nb\nc"), 20, 10);
		assert!(emitter.frame(&frame("a\nb"), 20, 10).contains("undefined"));
	}

	#[test]
	fn a_change_that_has_scrolled_away_only_moves_the_cursor() {
		let mut emitter = Emitter::new();
		emitter.frame(&frame("1\n2\n3\n4\n5"), 20, 2);
		assert_eq!(
			emitter.frame(&frame("X\n2\n3\n4\n5"), 20, 2),
			"\u{1b}[999D\u{1b}[4A"
		);
	}

	#[test]
	fn the_cursor_walks_back_over_wrapped_rows_not_lines() {
		// Two lines, four rows: the wrap decides how far up the cursor goes, not the newlines.
		let mut emitter = Emitter::new();
		emitter.frame(&frame("abcdefgh\nx"), 4, 10);
		assert!(
			emitter
				.frame(&frame("abcdefgh\ny"), 4, 10)
				.starts_with("\u{1b}[999D\u{1b}[2A")
		);
	}

	#[test]
	fn a_narrowed_terminal_walks_the_cursor_back_over_rows_that_are_not_there() {
		// One row at 8 columns; the terminal is then 4, where that row would have been two. Upstream
		// re-wraps the previous Frame at the current width and walks back over the two rows it now
		// computes, rather than over the one it drew. That is what a terminal sees, so it is what
		// this emits (ADR-0017).
		let mut emitter = Emitter::new();
		emitter.frame(&frame("abcdefgh"), 8, 10);
		assert!(
			emitter
				.frame(&frame("abcdefgz"), 4, 10)
				.starts_with("\u{1b}[999D\u{1b}[1A"),
			"a narrowed terminal should walk back over the re-wrapped row count"
		);

		// And widening cannot change it: a row that already fits is still one row.
		let mut emitter = Emitter::new();
		emitter.frame(&frame("abcdefgh"), 8, 10);
		assert!(
			emitter
				.frame(&frame("abcdefgz"), 40, 10)
				.starts_with("\u{1b}[999D\u{1b}[2K"),
			"a widened terminal should walk back over the one row it drew, which is no rows at all"
		);
	}

	#[test]
	fn a_style_is_stated_from_scratch_and_given_back() {
		let line = Line::from_iter([
			Span::raw("a"),
			Span::styled("b", Style::new().fg(Color::Cyan)),
			Span::raw("c"),
		]);
		let mut emitter = Emitter::new();
		let out = emitter.frame(&Frame { lines: vec![line] }, 20, 10);
		assert_eq!(out, "\u{1b}[?25la\u{1b}[0;36mb\u{1b}[0mc");
	}

	#[test]
	fn a_row_that_ends_styled_resets_before_the_next_one() {
		let line = Line::from(Span::styled("a", Style::new().add_modifier(Modifier::DIM)));
		let mut emitter = Emitter::new();
		let out = emitter.frame(
			&Frame {
				lines: vec![line, Line::from(Span::raw("b"))],
			},
			20,
			10,
		);
		assert_eq!(out, "\u{1b}[?25l\u{1b}[0;2ma\u{1b}[0m\nb");
	}

	#[test]
	fn a_trailing_space_is_a_difference_a_buffer_would_have_lost() {
		// The reason the diff is over the Frame's rows and not a padded `Buffer`.
		let mut emitter = Emitter::new();
		emitter.frame(&frame("a\nb"), 20, 10);
		assert_ne!(emitter.frame(&frame("a\nb "), 20, 10), "");
	}

	#[test]
	fn colours_and_modifiers_carry_their_ansi_codes() {
		assert_eq!(sgr(Style::default()), "\u{1b}[0m");
		assert_eq!(sgr(Style::new().fg(Color::DarkGray)), "\u{1b}[0;90m");
		assert_eq!(sgr(Style::new().bg(Color::Red)), "\u{1b}[0;41m");
		assert_eq!(
			sgr(Style::new().fg(Color::Indexed(200))),
			"\u{1b}[0;38;5;200m"
		);
		assert_eq!(
			sgr(Style::new().fg(Color::Rgb(1, 2, 3))),
			"\u{1b}[0;38;2;1;2;3m"
		);
		assert_eq!(
			sgr(Style::new().add_modifier(Modifier::CROSSED_OUT.union(Modifier::DIM))),
			"\u{1b}[0;2;9m"
		);
	}

	#[test]
	fn a_cursor_that_does_not_move_writes_nothing() {
		assert_eq!(cursor_move(0, 0), "");
		assert_eq!(cursor_move(-999, 0), "\u{1b}[999D");
		assert_eq!(cursor_move(0, -2), "\u{1b}[2A");
		assert_eq!(cursor_move(3, 4), "\u{1b}[3C\u{1b}[4B");
	}
}
