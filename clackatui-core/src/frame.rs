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
	/// Wrapping is the terminal's behaviour, reproduced rather than avoided. A unit that does not
	/// fit in the columns left moves to the next row whole, which is what a terminal with `DECAWM`
	/// on does with a wide glyph at the right margin — it does not split it.
	fn rows(&self, columns: u16) -> Vec<Vec<Placed<'_>>> {
		let mut rows = Vec::new();
		if columns == 0 {
			return rows;
		}

		for line in &self.lines {
			let mut row: Vec<Placed<'_>> = Vec::new();
			let mut x = 0u16;

			for span in &line.spans {
				for Segment { text, width } in width::segments(&span.text) {
					if width == 0 {
						// No column of its own. A combining mark belongs to the unit before it and
						// goes into the same cell; an escape or a control character is not text we
						// draw at all, since a Frame carries its styling as a `Style`.
						if is_mark(text) {
							if let Some(last) = row.last_mut() {
								last.trailing.push(text);
							}
						}
						continue;
					}

					let width = u16::try_from(width).unwrap_or(u16::MAX);
					if width > columns {
						// Wider than the terminal. There is no column arrangement that holds it, and
						// guessing one would put every later unit at the wrong place; a terminal
						// would smear it across the margin instead. Left for the narrow-terminal
						// Scenarios to adjudicate rather than invented here.
						continue;
					}
					if x + width > columns {
						rows.push(std::mem::take(&mut row));
						x = 0;
					}

					row.push(Placed {
						x,
						text,
						trailing: Vec::new(),
						width,
						style: span.style,
					});
					x += width;
				}
			}

			rows.push(row);
		}

		rows
	}
}

/// One unit of text, at the column it was placed at.
struct Placed<'a> {
	x: u16,
	text: &'a str,
	/// Zero-width text that belongs to the same cell — combining marks.
	trailing: Vec<&'a str>,
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
				let mut symbol = String::from(unit.text);
				for mark in &unit.trailing {
					symbol.push_str(mark);
				}
				buf[(x, y)] = cell(&symbol, unit.style, unit.width);
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

	/// The jamo of M0: six columns under clack's model, two under Ratatui's. The stamp has to say
	/// six, and the three units have to sit two columns apart.
	#[test]
	fn a_unit_is_stamped_with_clacks_width_not_ratatuis() {
		let buf = drawn(&frame(&["\u{1100}\u{1161}\u{11A8}"]), 8, 1);
		assert_eq!(buf[(0, 0)].symbol(), "\u{1100}");
		assert_eq!(buf[(2, 0)].symbol(), "\u{1161}");
		assert_eq!(buf[(4, 0)].symbol(), "\u{11A8}");
		for x in [0, 2, 4] {
			assert_eq!(
				buf[(x, 0)].diff_option,
				CellDiffOption::ForcedWidth(NonZeroU16::new(2).unwrap())
			);
		}
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

	#[test]
	fn a_combining_mark_joins_the_cell_before_it() {
		let buf = drawn(&frame(&["e\u{0301}x"]), 4, 1);
		assert_eq!(buf[(0, 0)].symbol(), "e\u{0301}");
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

	#[test]
	fn a_unit_wider_than_the_terminal_is_left_out() {
		// A tab is eight columns under clack's model — a fixed width, not a tab stop — so it does
		// not fit a four-column terminal at all. Placing it would put everything after it at a
		// column no model agrees on.
		let buf = drawn(&frame(&["\tx"]), 4, 1);
		assert_eq!(row(&buf, 0), "x   ");
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
