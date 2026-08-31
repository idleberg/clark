//! A Frame: the complete visual state of a Prompt at one instant, and the `Widget` that draws it.
//!
//! clack builds a Frame as a string with ANSI escapes in it and hands it to a writer. Here the two
//! halves are separated: a Frame is styled text with no escapes anywhere in it, and turning it into
//! bytes is the Emitter's job (ADR-0002). What sits between them is this module — the step where
//! text stops being a string and becomes cells at columns.
//!
//! That step is where the width model has to hold. Every cell is stamped with
//! [`CellDiffOption::ForcedWidth`] carrying *our* measurement, which M0 confirmed is the number
//! `Buffer::diff_iter` skips trailing columns by ([ADR-0007]); the unit placed in a cell is one
//! [`width::Segment`], so the columns a line was laid out with are the columns it is drawn at.
//! `Buffer::set_string` is never called, for the reason ADR-0005 gives — it measures under a
//! different model.
//!
//! [ADR-0007]: https://github.com/idleberg/clackatui/blob/main/docs/adr/0007-forced-width-holds-but-the-emitter-owns-shrink-repaints.md

use std::num::NonZeroU16;

use ratatui_core::buffer::{Buffer, Cell, CellDiffOption};
use ratatui_core::layout::Rect;
use ratatui_core::style::Style;
use ratatui_core::widgets::Widget;
use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};

use crate::width::{self, Segment};
use crate::wrap;

/// A run of text drawn in one style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
	pub text: String,
	pub style: Style,
}

impl Span {
	pub fn raw(text: impl Into<String>) -> Self {
		Self {
			text: text.into(),
			style: Style::default(),
		}
	}

	pub fn styled(text: impl Into<String>, style: impl Into<Style>) -> Self {
		Self {
			text: text.into(),
			style: style.into(),
		}
	}

	/// The columns this span occupies, as clack would measure them.
	pub fn width(&self) -> usize {
		width::width(&self.text)
	}
}

/// One line of a Frame — what clack separates with `\n`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
	pub spans: Vec<Span>,
}

impl Line {
	/// A line with nothing on it. clack emits several per Frame, and they are not the same as no
	/// line at all: each one still occupies a row.
	pub fn blank() -> Self {
		Self::default()
	}

	pub fn push(&mut self, span: Span) -> &mut Self {
		self.spans.push(span);
		self
	}

	/// This line cut into paragraphs wherever one of its spans carried a line break of its own.
	///
	/// `wrapAnsi` breaks on `\n` before it breaks on width, and it styles each of the pieces
	/// separately — so nothing leaks across one, and a widget that wraps a Line has to hand it the
	/// paragraphs one at a time rather than the whole. Never empty: a Line with no break in it comes
	/// back as itself.
	pub fn paragraphs(&self) -> Vec<Line> {
		let mut lines = vec![Line::blank()];
		for span in &self.spans {
			for (index, part) in span.text.split('\n').enumerate() {
				if index > 0 {
					lines.push(Line::blank());
				}
				if !part.is_empty() {
					lines
						.last_mut()
						.expect("a paragraph is pushed before anything is written into it")
						.push(Span::styled(part, span.style));
				}
			}
		}
		lines
	}

	/// The columns this line occupies before any wrapping.
	pub fn width(&self) -> usize {
		self.spans.iter().map(Span::width).sum()
	}

	/// The rows this line occupies at `columns` wide, each a Line of its own.
	///
	/// Never empty: a line with nothing on it comes back as one blank row, because upstream's
	/// `wrapAnsi('')` is `['']` and a blank row is still a row.
	///
	/// This is what upstream does to a *styled* string — `wrapAnsi` skips escapes when it measures
	/// and reopens the style it closed on the far side of a break, so the breaks fall where they
	/// would in the plain text and nothing visible is added. A Frame has no escapes to skip or
	/// reopen (ADR-0011), so the same thing here is a set of break offsets applied to the spans. The
	/// two are the same appearance by construction; `wrap_parity.rs` is what says the offsets are
	/// clack's.
	///
	/// [`Frame::rows`] does not go through this. It has to place cells rather than hand back text,
	/// and it segments each row after the break for a reason spelled out there — but the text and
	/// style bookkeeping either side of the break is deliberately the same, and
	/// `a_wrapped_line_lays_out_the_way_the_frame_lays_it_out` holds the two together.
	pub fn wrap(&self, columns: usize) -> Vec<Line> {
		let (text, styles) = self.composed();

		let mut out = Vec::new();
		let mut start = 0usize;
		for end in wrap::breaks(&text, columns)
			.into_iter()
			.chain(std::iter::once(text.len()))
		{
			out.push(slice(&text, &styles, start, end));
			start = end;
		}
		out
	}

	/// The line's text, composed as clack composes it, and where each span ends in it.
	///
	/// Each span is normalized on its own, which differs from normalizing the join only where a span
	/// begins with a combining mark whose base is styled differently — a Line no widget builds, and
	/// the same compromise [`Frame::rows`] makes for the same reason.
	fn composed(&self) -> (String, Vec<(usize, Style)>) {
		let mut text = String::new();
		let mut styles: Vec<(usize, Style)> = Vec::new();
		for span in &self.spans {
			text.push_str(&wrap::normalize(&span.text));
			styles.push((text.len(), span.style));
		}
		(text, styles)
	}
}

/// `text[start..end]`, cut back into spans along the style boundaries it crosses.
///
/// Adjacent runs of one style are not merged: two spans the caller wrote separately stay separate,
/// so a wrapped Line compares equal to the same Line written out by hand only where it was written
/// the same way. Nothing downstream cares — a Frame lays out per segment and the Emitter states a
/// style per cell — and merging would make a Line that came back from here unlike the one that went
/// in.
fn slice(text: &str, styles: &[(usize, Style)], start: usize, end: usize) -> Line {
	let mut line = Line::blank();
	let mut at = start;
	for (span_end, style) in styles {
		if *span_end <= at {
			continue;
		}
		let cut = (*span_end).min(end);
		if cut > at {
			line.push(Span::styled(&text[at..cut], *style));
			at = cut;
		}
		if at >= end {
			break;
		}
	}
	line
}

impl FromIterator<Span> for Line {
	fn from_iter<I: IntoIterator<Item = Span>>(spans: I) -> Self {
		Self {
			spans: spans.into_iter().collect(),
		}
	}
}

impl From<Span> for Line {
	fn from(span: Span) -> Self {
		Self { spans: vec![span] }
	}
}

/// The complete visual state of a Prompt at one instant.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
	pub lines: Vec<Line>,
}

impl Frame {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn push(&mut self, line: impl Into<Line>) -> &mut Self {
		self.lines.push(line.into());
		self
	}

	/// The widest line, before any wrapping.
	pub fn width(&self) -> usize {
		self.lines.iter().map(Line::width).max().unwrap_or(0)
	}

	/// How many rows this Frame needs at `columns` wide.
	///
	/// The Emitter sizes its `Buffer` with this, because an inline Frame's height is whatever the
	/// content makes it — which is the whole reason `Viewport::Inline` was unusable (ADR-0002).
	pub fn height(&self, columns: u16) -> u16 {
		self.rows(columns).len().try_into().unwrap_or(u16::MAX)
	}

	/// The Frame laid out at `columns` wide: one entry per terminal row.
	///
	/// The rows are [`wrap`]'s, not this module's. clack does not let a terminal wrap its output —
	/// `@clack/core`'s render wraps the Frame itself, writes the result, and counts the cursor back
	/// over the rows that came out (ADR-0012) — so the rows are decided by a word wrap before
	/// anything is placed, and this only fills them.
	///
	/// Segmenting happens after the wrap and never across it, because upstream breaks a long word by
	/// code point: a row can begin part-way through what would otherwise be one unit, and the parts
	/// are then measured as the parts they became.
	pub(crate) fn rows(&self, columns: u16) -> Vec<Row> {
		let mut rows: Vec<Row> = Vec::new();
		if columns == 0 {
			return rows;
		}

		for line in &self.lines {
			// One string per line, as clack has. A block is matched across span boundaries because
			// upstream has no span boundaries to stop at.
			let (text, styles) = line.composed();

			let mut start = 0usize;
			for end in wrap::breaks(&text, columns as usize)
				.into_iter()
				.chain(std::iter::once(text.len()))
			{
				let mut row = Row::new();
				let mut x = 0u16;
				let mut at = start;

				for Segment { text: unit, width } in width::segments(&text[start..end]) {
					let style = style_at(&styles, at);
					at += unit.len();

					if width == 0 {
						// No column of its own. A combining mark belongs to the unit before it and
						// goes into the same cell; an escape or a control character is not text we
						// draw at all, since a Frame carries its styling as a `Style`. A mark that
						// the wrap left at the start of a row has nothing to attach to and is lost,
						// which is the one thing a terminal would do differently.
						if is_mark(unit) {
							if let Some(last) = row.last_mut() {
								last.symbol.push_str(unit);
							}
						}
						continue;
					}

					let width = u16::try_from(width).unwrap_or(u16::MAX);
					if x + width > columns {
						// Does not fit the row the wrap put it on — either wider than the terminal
						// entirely, which is how upstream leaves a tab on a row of its own, or a
						// unit the wrap measured per code point and we measure as a block. Left out
						// rather than moved: another row here would sit between clack's, and clack
						// counts its cursor back over rows.
						continue;
					}

					row.push(Placed {
						x,
						symbol: unit.to_owned(),
						width,
						style,
					});
					x += width;
				}

				rows.push(row);
				start = end;
			}
		}

		rows
	}
}

/// The style of the span covering `offset`, given each span's end offset in the line's text.
fn style_at(styles: &[(usize, Style)], offset: usize) -> Style {
	styles
		.iter()
		.find(|(end, _)| offset < *end)
		.map(|(_, style)| *style)
		.unwrap_or_default()
}

/// One terminal row of a laid-out Frame.
///
/// The Emitter diffs these rather than `Buffer` rows, because a `Buffer` row is padded to the
/// terminal width and so cannot tell a line that ends in a space from one that does not — a
/// distinction clack's own diff makes, since it compares the frame strings (ADR-0013).
pub(crate) type Row = Vec<Placed>;

/// One unit of text, at the column it was placed at.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Placed {
	pub(crate) x: u16,
	/// The unit, and any combining marks that belong in the same cell.
	pub(crate) symbol: String,
	pub(crate) width: u16,
	pub(crate) style: Style,
}

/// Whether a zero-width segment is a combining mark, as opposed to an escape or a control
/// character.
fn is_mark(text: &str) -> bool {
	text.chars()
		.all(|c| c.general_category_group() == GeneralCategoryGroup::Mark)
}

impl Widget for &Frame {
	/// Draws the Frame into `area`, blanking every cell of it first.
	///
	/// The blanks are stamped `ForcedWidth(1)` along with everything else, so that every cell in the
	/// area carries a width we chose. A cell left at `CellDiffOption::None` would be measured by
	/// Ratatui when the next Frame is diffed against it, and the two models disagreeing about one
	/// cell is enough to misplace the rest of the row.
	fn render(self, area: Rect, buf: &mut Buffer) {
		if area.is_empty() {
			return;
		}

		for y in area.top()..area.bottom() {
			for x in area.left()..area.right() {
				buf[(x, y)] = cell(" ", Style::default(), 1);
			}
		}

		for (row, placed) in self.rows(area.width).iter().enumerate() {
			let Ok(row) = u16::try_from(row) else { break };
			let Some(y) = area.top().checked_add(row) else {
				break;
			};
			if y >= area.bottom() {
				break;
			}

			for unit in placed {
				let x = area.left() + unit.x;
				buf[(x, y)] = cell(&unit.symbol, unit.style, unit.width);
				// The columns the unit covers are left as the blanks written above. The diff skips
				// them by the forced width, so what they hold cannot be observed — except when a
				// later Frame narrows this cell, which the Emitter has to notice for itself
				// (ADR-0007).
			}
		}
	}
}

fn cell(symbol: &str, style: Style, width: u16) -> Cell {
	let mut cell = Cell::EMPTY;
	cell.set_symbol(symbol);
	cell.set_style(style);
	cell.set_diff_option(CellDiffOption::ForcedWidth(
		NonZeroU16::new(width).unwrap_or(NonZeroU16::MIN),
	));
	cell
}

#[cfg(test)]
mod tests {
	use ratatui_core::style::{Color, Modifier};

	use super::*;

	/// The symbols of a row, joined — the continuation columns of a wide cell included, so that
	/// where a unit was placed is visible.
	fn row(buf: &Buffer, y: u16) -> String {
		(buf.area.left()..buf.area.right())
			.map(|x| buf[(x, y)].symbol())
			.collect()
	}

	fn drawn(frame: &Frame, columns: u16, rows: u16) -> Buffer {
		let area = Rect::new(0, 0, columns, rows);
		let mut buf = Buffer::empty(area);
		frame.render(area, &mut buf);
		buf
	}

	fn frame(lines: &[&str]) -> Frame {
		Frame {
			lines: lines.iter().map(|l| Line::from(Span::raw(*l))).collect(),
		}
	}

	#[test]
	fn a_line_becomes_a_row() {
		let buf = drawn(&frame(&["hi", "there"]), 8, 2);
		assert_eq!(row(&buf, 0), "hi      ");
		assert_eq!(row(&buf, 1), "there   ");
	}

	#[test]
	fn a_blank_line_still_occupies_a_row() {
		let frame = Frame {
			lines: vec![
				Line::from(Span::raw("a")),
				Line::blank(),
				Span::raw("b").into(),
			],
		};
		assert_eq!(frame.height(8), 3);
		let buf = drawn(&frame, 8, 3);
		assert_eq!(row(&buf, 1), "        ");
		assert_eq!(row(&buf, 2), "b       ");
	}

	#[test]
	fn every_cell_carries_a_width_we_chose() {
		let buf = drawn(&frame(&["hi"]), 4, 1);
		for x in 0..4 {
			assert_eq!(
				buf[(x, 0)].diff_option,
				CellDiffOption::ForcedWidth(NonZeroU16::new(1).unwrap()),
				"column {x} was left for Ratatui to measure"
			);
		}
	}

	/// A tab: eight columns under clack's model — a fixed width, not a tab stop — and one under
	/// Ratatui's. The stamp has to say eight, and what follows has to sit at column eight.
	#[test]
	fn a_unit_is_stamped_with_clacks_width_not_ratatuis() {
		let buf = drawn(&frame(&["\tx"]), 10, 1);
		assert_eq!(buf[(0, 0)].symbol(), "\t");
		assert_eq!(
			buf[(0, 0)].diff_option,
			CellDiffOption::ForcedWidth(NonZeroU16::new(8).unwrap())
		);
		assert_eq!(buf[(8, 0)].symbol(), "x");
	}

	/// The M0 probe symbol never arrives: `wrapAnsi` composes the Frame to NFC before anything is
	/// placed, and the three conjoining jamo are one syllable by then — two columns under both width
	/// models rather than six under one and two under the other. The disagreement `ForcedWidth`
	/// exists for is real, but this is not the example that shows it (ADR-0012).
	#[test]
	fn the_probe_symbol_composes_before_it_is_placed() {
		let buf = drawn(&frame(&["\u{1100}\u{1161}\u{11A8}"]), 8, 1);
		assert_eq!(buf[(0, 0)].symbol(), "\u{AC01}");
		assert_eq!(
			buf[(0, 0)].diff_option,
			CellDiffOption::ForcedWidth(NonZeroU16::new(2).unwrap())
		);
	}

	#[test]
	fn an_emoji_sequence_occupies_one_cell_of_two_columns() {
		let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
		let buf = drawn(&frame(&[family]), 6, 1);
		assert_eq!(buf[(0, 0)].symbol(), family);
		assert_eq!(
			buf[(0, 0)].diff_option,
			CellDiffOption::ForcedWidth(NonZeroU16::new(2).unwrap())
		);
		assert_eq!(buf[(2, 0)].symbol(), " ");
	}

	/// `b` with a combining acute, which has no composed form and so survives NFC as two code
	/// points. `e` with the same mark would not — it would be one character by the time it got here.
	#[test]
	fn a_combining_mark_joins_the_cell_before_it() {
		let buf = drawn(&frame(&["b\u{0301}x"]), 4, 1);
		assert_eq!(buf[(0, 0)].symbol(), "b\u{0301}");
		assert_eq!(buf[(1, 0)].symbol(), "x");
	}

	/// A Frame styles with `Style`, so an escape in its text is not styling — it is a stray control
	/// sequence, and drawing it would put bytes on the screen the Grid would then have to explain.
	#[test]
	fn an_escape_sequence_is_not_drawn() {
		let buf = drawn(&frame(&["\u{1B}[31mred"]), 6, 1);
		assert_eq!(row(&buf, 0), "red   ");
	}

	#[test]
	fn a_span_keeps_its_style_and_the_blanks_do_not() {
		let mut line = Line::blank();
		line.push(Span::styled("ab", Style::default().fg(Color::Cyan)));
		line.push(Span::raw("c"));
		let frame = Frame { lines: vec![line] };

		let buf = drawn(&frame, 4, 1);
		assert_eq!(buf[(0, 0)].fg, Color::Cyan);
		assert_eq!(buf[(1, 0)].fg, Color::Cyan);
		assert_eq!(buf[(2, 0)].fg, Color::Reset);
		assert_eq!(buf[(3, 0)].fg, Color::Reset);
	}

	#[test]
	fn a_line_wraps_at_the_terminal_width() {
		let frame = frame(&["abcdef"]);
		assert_eq!(frame.width(), 6);
		assert_eq!(frame.height(4), 2);

		let buf = drawn(&frame, 4, 2);
		assert_eq!(row(&buf, 0), "abcd");
		assert_eq!(row(&buf, 1), "ef  ");
	}

	/// A terminal with autowrap on moves a wide glyph to the next row whole rather than splitting
	/// it across the margin.
	#[test]
	fn a_wide_unit_wraps_whole() {
		let frame = frame(&["ab\u{4F60}"]);
		let buf = drawn(&frame, 3, 2);
		assert_eq!(row(&buf, 0), "ab ");
		assert_eq!(buf[(0, 1)].symbol(), "\u{4F60}");
	}

	#[test]
	fn wrapping_pushes_the_lines_after_it_down() {
		let frame = frame(&["abcdef", "z"]);
		assert_eq!(frame.height(4), 3);
		let buf = drawn(&frame, 4, 3);
		assert_eq!(row(&buf, 2), "z   ");
	}

	#[test]
	fn rows_beyond_the_area_are_dropped_rather_than_wrapped_around() {
		let buf = drawn(&frame(&["a", "b", "c"]), 4, 2);
		assert_eq!(row(&buf, 0), "a   ");
		assert_eq!(row(&buf, 1), "b   ");
	}

	/// A tab does not fit a four-column terminal at all. Upstream's mid-word break gives it a row of
	/// its own anyway — leaving the row it overflowed empty — so the line takes three rows, of which
	/// the middle one has nothing this can draw on it. The rows are clack's whether or not the unit
	/// on one of them can be placed, because clack counts its cursor back over them.
	#[test]
	fn a_unit_wider_than_the_terminal_is_left_out_but_keeps_its_row() {
		let frame = frame(&["\tx"]);
		assert_eq!(frame.height(4), 3);
		let buf = drawn(&frame, 4, 3);
		assert_eq!(row(&buf, 0), "    ");
		assert_eq!(row(&buf, 1), "    ");
		assert_eq!(row(&buf, 2), "x   ");
	}

	#[test]
	fn an_empty_area_draws_nothing() {
		let mut buf = Buffer::empty(Rect::new(0, 0, 0, 0));
		frame(&["hi"]).render(Rect::new(0, 0, 0, 0), &mut buf);
		assert_eq!(buf.area.area(), 0);
	}

	/// The Frame is the complete visual state, so drawing one over another leaves nothing of the
	/// first behind.
	#[test]
	fn a_frame_blanks_what_it_draws_over() {
		let area = Rect::new(0, 0, 6, 1);
		let mut buf = Buffer::empty(area);
		frame(&["old text"]).render(area, &mut buf);
		frame(&["new"]).render(area, &mut buf);
		assert_eq!(row(&buf, 0), "new   ");
	}

	#[test]
	fn a_frame_renders_inside_a_larger_buffer() {
		let mut buf = Buffer::empty(Rect::new(0, 0, 8, 3));
		frame(&["hi"]).render(Rect::new(2, 1, 4, 1), &mut buf);
		assert_eq!(row(&buf, 0), "        ");
		assert_eq!(row(&buf, 1), "  hi    ");
		assert_eq!(buf[(2, 1)].symbol(), "h");
	}

	#[test]
	fn styles_survive_a_wrap() {
		let mut line = Line::blank();
		line.push(Span::styled(
			"abcd",
			Style::default().add_modifier(Modifier::DIM),
		));
		let frame = Frame { lines: vec![line] };
		let buf = drawn(&frame, 2, 2);
		assert!(buf[(0, 1)].modifier.contains(Modifier::DIM));
	}

	// --- Line::wrap ---------------------------------------------------------------------------

	/// Plain text as a list of strings, for the tests below.
	fn wrapped(line: &Line, columns: usize) -> Vec<String> {
		line.wrap(columns)
			.iter()
			.map(|row| row.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn a_line_that_fits_comes_back_as_one_row() {
		let line = Line::from(Span::raw("hello"));
		assert_eq!(wrapped(&line, 10), ["hello"]);
	}

	#[test]
	fn an_empty_line_is_still_one_row() {
		assert_eq!(wrapped(&Line::blank(), 10), [""]);
	}

	fn text(line: &Line) -> String {
		line.spans.iter().map(|span| span.text.as_str()).collect()
	}

	/// A break splits the line, and the span it fell inside keeps its style on both sides of it.
	#[test]
	fn a_line_break_inside_a_span_starts_a_paragraph() {
		let green = Style::new().fg(Color::Green);
		let mut line = Line::from(Span::raw("a, "));
		line.push(Span::styled("one\ntwo", green));
		line.push(Span::raw(" tail"));

		let paragraphs = line.paragraphs();
		assert_eq!(paragraphs.len(), 2);
		assert_eq!(text(&paragraphs[0]), "a, one");
		assert_eq!(text(&paragraphs[1]), "two tail");
		assert_eq!(paragraphs[1].spans[0].style, green);
	}

	/// A break at either end of a span leaves a paragraph with nothing in it, and it is left with
	/// nothing in it — an empty span would draw the same and compare differently, and `slice` makes
	/// the same promise about not writing spans a caller did not.
	#[test]
	fn a_break_at_the_edge_of_a_span_leaves_an_empty_paragraph_empty() {
		let mut line = Line::from(Span::raw("one\n"));
		line.push(Span::raw("\ntwo"));

		let paragraphs = line.paragraphs();
		assert_eq!(paragraphs.len(), 3);
		assert_eq!(text(&paragraphs[0]), "one");
		assert_eq!(paragraphs[1], Line::blank());
		assert_eq!(text(&paragraphs[2]), "two");
	}

	/// A line with nothing to split comes back as itself rather than as nothing.
	#[test]
	fn a_line_with_no_break_is_one_paragraph() {
		let line = Line::from(Span::raw("hello"));
		assert_eq!(line.paragraphs(), std::slice::from_ref(&line));
		assert_eq!(Line::blank().paragraphs(), [Line::blank()]);
	}

	#[test]
	fn a_break_inside_a_span_splits_it() {
		let line = Line::from(Span::styled("hello world", Style::new().fg(Color::Green)));
		let rows = line.wrap(6);
		assert_eq!(wrapped(&line, 6), ["hello ", "world"]);
		// Both halves keep the style the whole span had.
		for row in &rows {
			assert_eq!(row.spans[0].style, Style::new().fg(Color::Green));
		}
	}

	#[test]
	fn a_break_between_two_spans_leaves_each_where_it_was() {
		let line: Line = [
			Span::styled("aaa ", Style::new().fg(Color::Green)),
			Span::raw("bbb"),
		]
		.into_iter()
		.collect();
		let rows = line.wrap(4);
		assert_eq!(wrapped(&line, 4), ["aaa ", "bbb"]);
		assert_eq!(rows[0].spans[0].style, Style::new().fg(Color::Green));
		assert_eq!(rows[1].spans[0].style, Style::default());
	}

	#[test]
	fn a_break_that_falls_mid_span_keeps_the_spans_either_side_of_it() {
		let line: Line = [
			Span::raw("ab"),
			Span::styled("cdefgh", Style::new().add_modifier(Modifier::DIM)),
		]
		.into_iter()
		.collect();
		let rows = line.wrap(4);
		assert_eq!(wrapped(&line, 4), ["abcd", "efgh"]);
		assert_eq!(rows[0].spans.len(), 2, "the first row keeps both spans");
		assert_eq!(rows[1].spans.len(), 1, "the second is all one style");
	}

	/// Wrapping a Line and laying out a Frame are two paths over the same break offsets, and only
	/// one of them is asserted against `fast-wrap-ansi`. This holds the other to it: a Line wrapped
	/// at a width and then drawn one row per line reaches the same cells as the same Line drawn
	/// whole at that width.
	///
	/// This is what [`crate::limit_options`] relies on. It wraps each option to the terminal less a
	/// padding, and the Frame it goes into is wrapped again to the whole terminal — a second pass
	/// that has to change nothing, or the rows it counted are not the rows that get drawn.
	#[test]
	fn a_wrapped_line_lays_out_the_way_the_frame_lays_it_out() {
		for text in [
			"hello world",
			"a rather long option that does not fit",
			"\u{4f60}\u{597d}\u{4f60}\u{597d}",
			"abcdefghij",
			"",
		] {
			for columns in [2u16, 4, 7, 40] {
				let line = Line::from(Span::raw(text));

				let whole = Frame {
					lines: vec![line.clone()],
				};
				let split = Frame {
					lines: line.wrap(columns as usize),
				};

				assert_eq!(
					whole.rows(columns),
					split.rows(columns),
					"{text:?} at {columns} columns"
				);
			}
		}
	}

	/// The exception to the test above, named rather than left as a gap in its loop.
	///
	/// Where one unit is wider than the whole row, the wrap cannot make it fit and leaves it on a
	/// row of its own that is still too wide. Wrapping that result again breaks it out a second
	/// time, so the two passes do not agree — a wide character at one column turns into a blank row
	/// and the character, and then into two blank rows and the character. Upstream does the same
	/// thing for the same reason; `fast-wrap-ansi` is not idempotent either.
	///
	/// Nothing reaches it: [`crate::limit_options`] wraps to the terminal *less* a padding and the
	/// Frame then wraps to the whole terminal, so the second width is never the narrower one.
	#[test]
	fn wrapping_twice_differs_only_where_a_unit_is_wider_than_the_row() {
		let line = Line::from(Span::raw("\u{4f60}\u{597d}"));

		let once = line.wrap(1);
		let twice: Vec<Line> = once.iter().flat_map(|row| row.wrap(1)).collect();
		assert_ne!(once, twice, "a wide character at one column");

		// One column wider and the character fits a row, so the second pass has nothing left to do.
		let once = line.wrap(2);
		let twice: Vec<Line> = once.iter().flat_map(|row| row.wrap(2)).collect();
		assert_eq!(once, twice);
	}
}
