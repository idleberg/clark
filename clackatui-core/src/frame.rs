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

	/// The columns this line occupies before any wrapping.
	pub fn width(&self) -> usize {
		self.spans.iter().map(Span::width).sum()
	}
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
	fn rows(&self, columns: u16) -> Vec<Vec<Placed>> {
		let mut rows = Vec::new();
		if columns == 0 {
			return rows;
		}

		for line in &self.lines {
			// One string per line, as clack has. A block is matched across span boundaries because
			// upstream has no span boundaries to stop at. Each span is composed on its own, which
			// differs from composing the join only where a span begins with a combining mark whose
			// base is styled differently — a Frame no widget builds.
			let mut text = String::new();
			let mut styles: Vec<(usize, Style)> = Vec::new();
			for span in &line.spans {
				text.push_str(&wrap::normalize(&span.text));
				styles.push((text.len(), span.style));
			}

			let mut start = 0usize;
			for end in wrap::breaks(&text, columns as usize)
				.into_iter()
				.chain(std::iter::once(text.len()))
			{
				let mut row: Vec<Placed> = Vec::new();
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

/// One unit of text, at the column it was placed at.
struct Placed {
	x: u16,
	/// The unit, and any combining marks that belong in the same cell.
	symbol: String,
	width: u16,
	style: Style,
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
}
