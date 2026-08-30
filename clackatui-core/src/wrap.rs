//! A port of `fast-wrap-ansi@0.2.0`, which is where clack's line breaks actually come from.
//!
//! `@clack/core`'s `render` does not hand a Frame to the terminal and let it wrap. It calls
//! `wrapAnsi(frame, process.stdout.columns, { hard: true, trim: false })` and writes the result, so
//! every break clack's output contains was computed in the process before a byte was written, and
//! `restoreCursor` counts rows off the same string. The terminal's own autowrap is never reached:
//! at the width clack wrapped to, no line is long enough to trigger it.
//!
//! That makes wrapping a *word* wrap rather than a column fill, which is the difference this module
//! exists to erase (ADR-0012). Text runs to the next row at the last space that fits; only a word
//! too long for a row on its own is broken mid-word, and `hard: true` is what asks for that.
//!
//! # What is ported, and what is not
//!
//! Upstream's `exec` branches on `trim` and `wordWrap`. clack passes `trim: false` and leaves
//! `wordWrap` at its default, and no other configuration is reachable from a Prompt, so only that
//! one is ported. What it removes:
//!
//! - the empty-string early return, which is `trim !== false`;
//! - `stringVisibleTrimSpacesRight`, applied to every row under `trim !== false`;
//! - the row-leading `trimStart`, likewise;
//! - the two `wordWrap === false` fallbacks into the mid-word break.
//!
//! With `trim: false` every separating space survives, so wrapping a line neither adds nor removes a
//! character: it is exactly a set of positions to break at, which is what [`breaks`] returns and why
//! a Frame can wrap a line without disturbing the spans that make it up.
//!
//! One piece of the original is deliberately absent: **the escape bookkeeping.** Upstream tracks the
//! open SGR code and OSC 8 URL so it can close them at the end of a row and reopen them at the start
//! of the next — the one place it is not a pure insertion. A Frame holds no escapes at all
//! (ADR-0011): it carries styling as a `Style` per span, and the Emitter re-states that style per
//! cell, so styling survives a break structurally rather than by patching the byte stream. Escaped
//! text passed here would break at the right columns but lose the reopening sequences, so don't.
//!
//! # `String(string).normalize()`
//!
//! `wrapAnsi` composes its input to NFC before it does anything else, and clack writes what comes
//! back — so this is not a detail of wrapping but the last thing that happens to a Frame's text
//! before it becomes bytes, and [`normalize`] is part of the port.
//!
//! It is worth being loud about, because it reaches further than it looks. `U+1100 U+1161 U+11A8`,
//! the conjoining jamo the M0 probe was built on, compose to the single syllable `U+AC01` — which
//! both width models measure as two columns. So the disagreement M0 was chosen to demonstrate can
//! never arrive at a terminal *through this path*: clack precomposes it away. `ForcedWidth` is not
//! thereby unnecessary — the models still part company over emoji sequences, tabs, and jamo that
//! have no composed form — but the specific example does not survive contact with `wrapAnsi`, and
//! the port only found that out by disagreeing with the harvest.

use std::borrow::Cow;

use unicode_normalization::{UnicodeNormalization, is_nfc_quick};

use crate::width::width;

/// `String.prototype.normalize()` with its default argument: compose to NFC.
///
/// A Frame's text passes through here before it is wrapped or placed, because upstream's does. The
/// borrowed case is the overwhelmingly common one — clack's own strings and ordinary typed input are
/// already composed — so the allocation is paid only by text that actually moves.
pub fn normalize(input: &str) -> Cow<'_, str> {
	match is_nfc_quick(input.chars()) {
		unicode_normalization::IsNormalized::Yes => Cow::Borrowed(input),
		_ => Cow::Owned(input.nfc().collect()),
	}
}

/// The byte offsets in `line` at which it breaks when wrapped to `columns` wide.
///
/// Ascending, each the start of a row; the first row starts at 0 and is not listed. `line` is one
/// line — a `\n` in it is text like any other, and [`wrap`] is what splits on them. It is expected
/// to be [`normalize`]d already: upstream composes the whole string before it splits it, so doing it
/// here would mean handing back offsets into a string the caller does not hold.
///
/// A `columns` of zero yields no breaks. Upstream reaches that case only through IEEE-754
/// infinities, and lays every character out on a row of its own; a terminal of no columns is not a
/// state a Prompt can be drawn in, and reproducing the arithmetic would mean carrying a division by
/// zero to do it.
pub fn breaks(line: &str, columns: usize) -> Vec<usize> {
	let mut at_rows = Vec::new();
	if columns == 0 {
		return at_rows;
	}

	// `row_start` is the offset the current row begins at, and `row_len` is upstream's `rowLength`:
	// an accumulator, not a measurement of the row. The two are not the same number — a row is
	// measured in blocks and the accumulator is a sum of parts — and upstream only re-measures after
	// a mid-word break. Accumulating anywhere it accumulates is the point.
	let mut row_start = 0usize;
	let mut row_len = 0usize;
	let mut at = 0usize;

	for (index, word) in line.split(' ').enumerate() {
		if index != 0 {
			// `at` is the space this word was split from. A full row breaks *before* it, so the
			// space begins the next row rather than trailing the one that filled up.
			if row_len >= columns {
				row_start = at;
				at_rows.push(at);
				row_len = 0;
			}
			row_len += 1;
			at += 1;
		}

		let word_width = width(word);

		if word_width > columns {
			// Too long for any row, so it is broken mid-word. Upstream first decides whether
			// starting it on the next row costs fewer breaks than starting it here.
			// Signed, because the columns left on this row can be negative once the separating space
			// has pushed it past the margin, and because upstream's `Math.floor` rounds towards
			// negative infinity where Rust's integer division would round towards zero.
			let (word_width, per_row, left) = (
				word_width as i64,
				columns as i64,
				columns as i64 - row_len as i64,
			);
			let starting_here = 1 + (word_width - left - 1).div_euclid(per_row);
			let starting_next = (word_width - 1).div_euclid(per_row);
			if starting_next < starting_here {
				row_start = at;
				at_rows.push(at);
			}

			hard_wrap(&mut at_rows, &mut row_start, line, at, word, columns);
			at += word.len();
			// The one place upstream re-measures rather than accumulating.
			row_len = width(&line[row_start..at]);
			continue;
		}

		if row_len + word_width > columns && row_len != 0 && word_width != 0 {
			row_start = at;
			at_rows.push(at);
			row_len = 0;
		}

		row_len += word_width;
		at += word.len();
	}

	at_rows
}

/// Upstream's `wrapWord`: break a single word wherever the row runs out, one code point at a time.
///
/// Per code point, not per [`Segment`](crate::width::Segment) — upstream measures each character on
/// its own, so an emoji sequence long enough to need breaking is broken inside, and its parts are
/// then measured as the parts they became. A Frame segments each row *after* wrapping for exactly
/// this reason: doing it the other way round would leave the two disagreeing about a row it had
/// already laid out.
fn hard_wrap(
	at_rows: &mut Vec<usize>,
	row_start: &mut usize,
	line: &str,
	word_at: usize,
	word: &str,
	columns: usize,
) {
	// Measured off the row rather than taken from the caller's accumulator, as upstream does.
	let mut visible = width(&line[*row_start..word_at]);
	let mut characters = word.char_indices().peekable();

	while let Some((offset, character)) = characters.next() {
		let at = word_at + offset;
		let mut buffer = [0u8; 4];
		let character_width = width(character.encode_utf8(&mut buffer));

		if visible + character_width > columns {
			// A character too wide for a row of its own still starts one, leaving the row it did not
			// fit on empty. That empty row is a row: it is what a Frame draws a blank line for.
			*row_start = at;
			at_rows.push(at);
			visible = 0;
		}

		visible += character_width;

		// A row filled exactly to the margin ends there, but only if something follows it.
		if visible == columns && characters.peek().is_some() {
			let next = at + character.len_utf8();
			*row_start = next;
			at_rows.push(next);
			visible = 0;
		}
	}

	// A last row of nothing but zero-width text — combining marks — is folded back into the one
	// before it, so that a mark cannot end up on a row apart from what it modifies.
	if visible == 0 && *row_start < word_at + word.len() && !at_rows.is_empty() {
		at_rows.pop();
		*row_start = at_rows.last().copied().unwrap_or(0);
	}
}

/// The rows `line` occupies at `columns` wide, in order.
///
/// The rows partition the line: joined back together they are the line again, since nothing is
/// trimmed and no break inserts a character.
pub fn rows(line: &str, columns: usize) -> Vec<&str> {
	let at_rows = breaks(line, columns);
	let mut out = Vec::with_capacity(at_rows.len() + 1);
	let mut start = 0;
	for at in at_rows {
		out.push(&line[start..at]);
		start = at;
	}
	out.push(&line[start..]);
	out
}

/// `wrapAnsi(input, columns, { hard: true, trim: false })`.
///
/// The input is composed to NFC first, as upstream's is, so this is the one entry point that can
/// hand back text it was not given. Line endings are upstream's too: `\r\n` and `\n` both separate,
/// and both come back as `\n`.
pub fn wrap(input: &str, columns: usize) -> String {
	let input = normalize(input);
	let mut out = String::with_capacity(input.len());
	for (index, line) in input.split('\n').enumerate() {
		if index != 0 {
			out.push('\n');
		}
		let line = line.strip_suffix('\r').unwrap_or(line);
		for (index, row) in rows(line, columns).into_iter().enumerate() {
			if index != 0 {
				out.push('\n');
			}
			out.push_str(row);
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The corpus in `tests/wrap_parity.rs` is what says these answers are clack's. What is written
	/// out here is the reasoning each one stands for, which a fixture of 47 rows cannot show.
	fn at(line: &str, columns: usize) -> Vec<&str> {
		rows(line, columns)
	}

	#[test]
	fn a_line_that_fits_is_one_row() {
		assert_eq!(at("hello", 10), ["hello"]);
		assert_eq!(at("hello", 5), ["hello"]);
		assert_eq!(at("", 10), [""]);
	}

	#[test]
	fn a_word_that_does_not_fit_moves_to_the_next_row_whole() {
		assert_eq!(at("ab cd", 4), ["ab ", "cd"]);
	}

	/// The break falls *before* the space, not after it, so a row that is already full hands its
	/// separator on to the row below rather than trailing it past the margin.
	#[test]
	fn a_full_row_gives_the_space_to_the_next_one() {
		assert_eq!(at("abc de", 3), ["abc", " de"]);
	}

	/// Two spaces means an empty word between them, which is what clack's `step + "  " + message`
	/// puts in front of every question.
	#[test]
	fn a_double_space_is_a_word_of_nothing() {
		assert_eq!(at("a  b", 3), ["a  ", "b"]);
	}

	#[test]
	fn a_word_longer_than_a_row_is_broken_inside() {
		assert_eq!(at("abcdefghij", 4), ["abcd", "efgh", "ij"]);
	}

	/// Before breaking a long word, upstream asks whether starting it on the next row would cost
	/// fewer breaks than starting it here, and moves it down if so. Six letters at four columns take
	/// two rows either way, so they start here; seven would take three from column two and only two
	/// from column zero, so they start below.
	#[test]
	fn a_long_word_starts_on_whichever_row_costs_fewer_breaks() {
		assert_eq!(at("a abcdef", 4), ["a ab", "cdef"]);
		assert_eq!(at("a abcdefg", 4), ["a ", "abcd", "efg"]);
	}

	/// A unit too wide for any row still takes one of its own, leaving the row it overflowed empty.
	/// A tab is eight columns under clack's model — a fixed width, not a tab stop.
	#[test]
	fn a_unit_wider_than_the_terminal_takes_a_row_and_leaves_one_empty() {
		assert_eq!(at("\tx", 4), ["", "\t", "x"]);
	}

	#[test]
	fn wide_text_breaks_before_it_straddles_the_margin() {
		assert_eq!(at("a\u{4F60}", 2), ["a", "\u{4F60}"]);
	}

	/// A row filled exactly to the margin ends there, and a combining mark on the other side of that
	/// break is separated from the character it modifies. Upstream does this — the rule fires on the
	/// width of the character just placed, before it has looked at what follows — and the fold at the
	/// end of a mid-word break is the only place it takes any of it back. Not a case to fix here: a
	/// Frame that drew it otherwise would put the rest of the line at columns clack did not use.
	#[test]
	fn a_mark_can_be_left_on_the_far_side_of_a_break() {
		assert_eq!(at("ab\u{0301}c", 2), ["ab", "\u{0301}c"]);
	}

	/// The exception, and upstream's only one: a mid-word break that would end the word on a row of
	/// nothing but marks is folded back into the row before it.
	#[test]
	fn a_word_ending_in_marks_is_not_left_on_a_row_of_its_own() {
		assert_eq!(at("a \u{0301}\u{0301}", 1), ["a", " \u{0301}\u{0301}"]);
	}

	#[test]
	fn line_endings_are_upstreams() {
		assert_eq!(wrap("ab\r\ncd", 4), "ab\ncd");
		assert_eq!(wrap("ab\n", 4), "ab\n");
		assert_eq!(wrap("ab\rcd", 4), "ab\rcd");
	}

	/// The conjoining jamo of M0 compose to one syllable, so nothing here is left to wrap. See the
	/// module docs: this is `wrapAnsi`'s doing, not ours.
	#[test]
	fn text_is_composed_before_it_is_wrapped() {
		assert_eq!(wrap("\u{1100}\u{1161}\u{11A8}", 4), "\u{AC01}");
		assert_eq!(normalize("e\u{0301}"), "\u{00E9}");
		assert!(matches!(normalize("already"), Cow::Borrowed(_)));
	}

	#[test]
	fn a_terminal_of_no_columns_is_left_alone() {
		assert_eq!(at("abc", 0), ["abc"]);
	}
}
