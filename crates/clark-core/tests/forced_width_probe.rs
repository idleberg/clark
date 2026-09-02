//! M0 — the `ForcedWidth` probe.
//!
//! ADR-0005 rejects Ratatui's width model in favour of a port of `fast-string-width`. ADR-0006 keeps
//! the `ratatui-core` dependency anyway, for `BufferDiff` alone. Those two decisions are compatible
//! only if [`CellDiffOption::ForcedWidth`] makes `Buffer::diff_iter` skip trailing columns according
//! to *our* measurement rather than re-deriving Ratatui's.
//!
//! This file tests that claim, and nothing else. If it fails, ADR-0006 reverses and clark owns
//! its own cell grid.
//!
//! Placing cells individually — never `Buffer::set_string*`, per ADR-0005 — is also exercised here,
//! because that is the only way `ForcedWidth` can be stamped.
//!
//! ## The corpus this was chosen from
//!
//! `fast-string-width@3.0.2` and `ratatui-core` 0.1.2 (over `unicode-width` 0.2.2) were measured
//! against the same strings. They agree on far more than ADR-0005 anticipated — every emoji case
//! the ADR names, including ZWJ sequences, skin-tone modifiers, regional indicators, keycaps and
//! VS16 presentation, now measures identically, because `unicode-width` 0.2.2 implements UAX #51.
//! Three disagreements remain:
//!
//! | codepoints | `fast-string-width` | Ratatui |
//! |---|---|---|
//! | `1100 1161 11A8` conjoining jamo | 6 | 2 |
//! | `200D` lone ZWJ | 1 | 0 |
//! | `0007` BEL | 0 | debug-assert panic |
//!
//! The first is the probe symbol. This does not weaken ADR-0005 — agreement today is coincidence,
//! not construction, and the ADR says so — but it does mean the width port is less urgent than the
//! roadmap implies.

use std::num::NonZeroU16;

use ratatui_core::buffer::{Buffer, Cell, CellDiffOption, CellWidth};
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier};

/// Conjoining Hangul jamo: `U+1100 U+1161 U+11A8`, which compose into a single grapheme cluster
/// rendered as one syllable block, 각.
///
/// `fast-string-width` has no grapheme awareness here — it sums the East Asian Width of all three
/// jamo and reports 6. Ratatui segments the cluster and measures it as one syllable, 2. Ours is the
/// *larger* number, so a diff using Ratatui's would leave four columns of a previous Frame
/// unpainted underneath the syllable.
const JAMO: &str = "\u{1100}\u{1161}\u{11A8}";

/// What clack's measurement, and so our port of it, says [`JAMO`] occupies.
const OUR_WIDTH: u16 = 6;

/// What Ratatui measures [`JAMO`] as.
const RATATUI_WIDTH: u16 = 2;

fn forced(symbol: &str, width: u16) -> Cell {
	let mut cell = Cell::EMPTY;
	cell.set_symbol(symbol);
	cell.set_diff_option(CellDiffOption::ForcedWidth(NonZeroU16::new(width).unwrap()));
	cell
}

fn narrow(symbol: &str) -> Cell {
	forced(symbol, 1)
}

/// A cell whose width Ratatui derives for itself.
fn unstamped(symbol: &str) -> Cell {
	let mut cell = Cell::EMPTY;
	cell.set_symbol(symbol);
	cell
}

/// A blank row of `width` cells, each stamped `ForcedWidth(1)` the way the Emitter would.
fn blank_row(width: u16) -> Buffer {
	let area = Rect::new(0, 0, width, 1);
	let mut buffer = Buffer::empty(area);
	for x in 0..width {
		buffer[(x, 0)] = narrow(" ");
	}
	buffer
}

/// The `(x, y, symbol)` triples a diff yields, which is what the Emitter consumes.
fn diffed(prev: &Buffer, next: &Buffer) -> Vec<(u16, u16, String)> {
	prev.diff_iter(next)
		.map(|(x, y, cell)| (x, y, cell.symbol().to_string()))
		.collect()
}

fn at(x: u16, symbol: &str) -> (u16, u16, String) {
	(x, 0, symbol.to_string())
}

/// The probe is only meaningful while the two models actually disagree on this symbol. If
/// `unicode-width` gains grapheme-aware jamo handling, this fails and the rest of the file stops
/// proving anything — pick a new symbol from the corpus in the module docs.
#[test]
fn ratatui_and_our_width_model_disagree() {
	assert_eq!(
		JAMO.cell_width(),
		RATATUI_WIDTH,
		"Ratatui no longer measures conjoining jamo as {RATATUI_WIDTH}; pick a new probe symbol"
	);
	assert_ne!(RATATUI_WIDTH, OUR_WIDTH);
}

/// Unstamped, the cell is measured Ratatui's way — 2 columns — so the diff walks on at x=2 and
/// repaints cells our model considers hidden underneath the syllable. This is the failure mode
/// `ForcedWidth` has to prevent, asserted here so the contrast in the next test is not a
/// coincidence.
#[test]
fn without_forced_width_the_diff_uses_ratatuis_number() {
	let mut prev = blank_row(8);
	prev[(3, 0)] = narrow("x");

	let mut next = blank_row(8);
	next[(0, 0)] = unstamped(JAMO);

	let seen = diffed(&prev, &next);
	assert_eq!(
		seen,
		vec![at(0, JAMO), at(3, " ")],
		"expected Ratatui's width of 2 to leave x=3 visible and repainted"
	);
}

/// The claim ADR-0006 rests on: stamped with our width, the diff skips five trailing columns and
/// does not repaint anything beneath the syllable.
#[test]
fn forced_width_makes_the_diff_skip_our_number_of_columns() {
	let mut prev = blank_row(8);
	prev[(3, 0)] = narrow("x");

	let mut next = blank_row(8);
	next[(0, 0)] = forced(JAMO, OUR_WIDTH);

	let seen = diffed(&prev, &next);
	assert_eq!(
		seen,
		vec![at(0, JAMO)],
		"the diff must skip x=1..=5, our trailing columns, and so never reach x=3"
	);
}

/// The far side of the same skip: content at our first free column is still reached.
#[test]
fn forced_width_resumes_the_diff_at_our_next_column() {
	let prev = blank_row(8);

	let mut next = blank_row(8);
	next[(0, 0)] = forced(JAMO, OUR_WIDTH);
	next[(6, 0)] = narrow("a");
	next[(7, 0)] = narrow("b");

	let seen = diffed(&prev, &next);
	assert_eq!(seen, vec![at(0, JAMO), at(6, "a"), at(7, "b")]);
}

/// The opposite direction — forcing a width *smaller* than Ratatui's. No grapheme in the corpus
/// measures this way today, but the Emitter must be able to narrow a cell regardless, and the
/// halfwidth katakana sound mark is where Ratatui's model is least like plain `unicode-width`: it
/// adds a correction of its own.
#[test]
fn forced_width_overrides_ratatuis_own_correction() {
	let prev = blank_row(6);

	let mut next = blank_row(6);
	next[(0, 0)] = narrow("你"); // Ratatui measures 2
	next[(1, 0)] = narrow("\u{FF9E}"); // Ratatui measures 1 via its dakuten correction
	next[(2, 0)] = narrow("a");

	let seen = diffed(&prev, &next);
	assert_eq!(
		seen,
		vec![at(0, "你"), at(1, "\u{FF9E}"), at(2, "a")],
		"forcing 1 must skip nothing, whatever Ratatui measures these as"
	);
}

// --- Shrinking: what the Emitter has to compensate for ---------------------------------------
//
// A forced-wide cell replaced by narrow content leaves the columns it covered showing stale glyph
// and stale style. Ratatui's `CellDiffOption::None` path has explicit logic for exactly this — see
// `TrailingState` in `buffer/diff.rs`. The `ForcedWidth` arm has none: it advances `pos` by the
// forced width and returns, with no trailing range.
//
// These tests pin that down as observed behaviour rather than desired behaviour. The Emitter owns
// the fix.

/// The gap: shrinking a forced-wide cell does **not** yield its trailing columns, so a diff-driven
/// Emitter would leave five columns of stale syllable on screen.
#[test]
fn shrinking_a_forced_wide_cell_leaves_its_trailing_columns_unpainted() {
	let mut prev = blank_row(8);
	prev[(0, 0)] = forced(JAMO, OUR_WIDTH);

	let mut next = blank_row(8);
	next[(0, 0)] = narrow("a");

	let seen = diffed(&prev, &next);
	assert_eq!(
		seen,
		vec![at(0, "a")],
		"x=1..=5 are not yielded; the Emitter must repaint them itself"
	);
}

/// The same holds when the wide cell carried a background colour and a modifier visible on blank
/// cells — the case Ratatui force-emits for under `CellDiffOption::None`. `ForcedWidth` does not,
/// so the stale style survives too.
#[test]
fn shrinking_a_styled_forced_wide_cell_leaves_its_style_behind() {
	let mut prev = blank_row(8);
	let mut wide = forced(JAMO, OUR_WIDTH);
	wide.bg = Color::Red;
	wide.modifier = Modifier::REVERSED;
	prev[(0, 0)] = wide;

	let mut next = blank_row(8);
	next[(0, 0)] = narrow("a");

	let seen = diffed(&prev, &next);
	assert_eq!(seen, vec![at(0, "a")]);
}

/// There is no escape hatch by way of the `None` path either — and this is the test that closes off
/// the Emitter's options.
///
/// Leaving the *replacing* cell unstamped does put the diff back on the `None` path, which reads
/// `previous.cell_width()`, and that call honours the previous cell's `ForcedWidth`. But the
/// force-trailing branch is guarded by a second condition: the previous cell must have carried a
/// background colour or a modifier visible on a blank cell. An unstyled wide cell fails it, and the
/// trailing columns stay unpainted.
///
/// Ratatui can afford that guard because printing a wide glyph physically clears its own trailing
/// columns, so only *style* can be left stale. Under a forced width that reasoning does not hold:
/// our width is the one the layout used, not the one the terminal will advance the cursor by.
#[test]
fn an_unstamped_replacement_over_an_unstyled_wide_cell_still_paints_nothing() {
	// Both rows are unstamped apart from the wide cell, so that cells differing only in their
	// `diff_option` cannot be mistaken for trailing-column repaints.
	let mut prev = Buffer::empty(Rect::new(0, 0, 8, 1));
	prev[(0, 0)] = forced(JAMO, OUR_WIDTH);

	let mut next = Buffer::empty(Rect::new(0, 0, 8, 1));
	next[(0, 0)] = unstamped("a");

	let seen = diffed(&prev, &next);
	assert_eq!(
		seen,
		vec![at(0, "a")],
		"no background and no visible-on-blank modifier, so the force-trailing branch never fires"
	);
}

/// The other side of that guard, which locates it precisely: give the previous wide cell a
/// background and the `None` path *does* force-emit all five trailing columns, honouring the
/// forced width when it computes the range.
///
/// So the mechanism understands `ForcedWidth` perfectly well. It simply declines to use it unless
/// style is at stake. The Emitter cannot rely on a condition it does not control.
#[test]
fn an_unstamped_replacement_over_a_styled_wide_cell_forces_the_trailing_columns() {
	let mut prev = Buffer::empty(Rect::new(0, 0, 8, 1));
	let mut wide = forced(JAMO, OUR_WIDTH);
	wide.bg = Color::Red;
	prev[(0, 0)] = wide;

	let mut next = Buffer::empty(Rect::new(0, 0, 8, 1));
	next[(0, 0)] = unstamped("a");

	let seen = diffed(&prev, &next);
	assert_eq!(
		seen,
		vec![
			at(0, "a"),
			at(1, " "),
			at(2, " "),
			at(3, " "),
			at(4, " "),
			at(5, " ")
		],
		"the forced width of 6 sets the trailing range, so x=1..=5 are force-emitted"
	);
}
