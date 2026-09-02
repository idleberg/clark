//! `limitOptions`: an option list cut down to what the terminal has room for.
//!
//! Every list Prompt draws through this — `select`, `multiselect`, `select-key`, and the four that
//! come after them — so it is ported once, on its own, before any of them. It is also the most
//! arithmetic in clack, and none of it is arithmetic anyone would arrive at twice: a window that
//! starts sliding two options before the cursor reaches the bottom of it, two ellipsis rows decided
//! before a trim and decided again after it, and a trim that walks outwards from the cursor in an
//! order that depends on which ellipsis is already there.
//!
//! It is a pure function, so it gets a corpus rather than a recording (ADR-0008):
//! `scripts/harvest-limit-options.mjs` runs the real one over `fixtures/limit-options.json` and
//! `tests/limit_options_parity.rs` asserts against it.
//!
//! # What is different here
//!
//! Upstream returns styled strings, joins them with a prefix and hands the result to a wrap that
//! runs again over the whole Frame. A Frame carries no escapes (ADR-0011), so this returns
//! [`Line`]s. The wrap inside is still done, and has to be: the number of rows an option occupies is
//! what the trim below counts, and an option that wraps is more than one of them.
//!
//! # Two widths, again
//!
//! `columns` and `rows` are read off the Prompt's own output stream upstream, not off
//! `process.stdout` — the split ADR-0019 records for `confirm`'s message. Outside a harness they are
//! the same terminal. The defaults here are `getColumns`/`getRows`' own fallbacks for a stream that
//! is not a terminal, 80 and 20, rather than numbers of ours.

use ratatui_core::style::Style;

use crate::frame::{Line, Span};
use crate::theme::Theme;

/// The row upstream draws in place of the options it left out: three periods, dimmed.
///
/// Not a Theme symbol. Upstream spells it out where it is used rather than putting it through
/// `unicodeOr`, so it is the same three characters in an ASCII terminal as in a Unicode one.
pub const OVERFLOW: &str = "...";

/// The rows upstream leaves for everything that is not the option list.
///
/// `select` overrides it with the height of its own title and footer; this is what a caller that
/// does not gets.
pub const DEFAULT_ROW_PADDING: usize = 4;

/// The shortest list upstream will draw, whatever the terminal or `max_items` say.
///
/// Upstream's comment: "We clamp to minimum 5 because anything less doesn't make sense UX wise." It
/// applies to the terminal too, so a window five options tall is drawn into a terminal with room for
/// two and the rows below it are simply overrun.
pub const MINIMUM_ITEMS: usize = 5;

/// A list of options, and the terminal it has to fit in.
///
/// Built and then asked for [`lines`](Self::lines), rather than taking eight arguments in a row.
/// The defaults are upstream's: 80 columns, 20 rows, no `max_items`, no column padding, and
/// [`DEFAULT_ROW_PADDING`] rows kept back.
#[derive(Clone, Copy, Debug)]
pub struct LimitOptions<'a, T> {
	options: &'a [T],
	cursor: usize,
	columns: usize,
	rows: usize,
	max_items: Option<usize>,
	column_padding: usize,
	row_padding: usize,
	overflow: Style,
}

impl<'a, T> LimitOptions<'a, T> {
	/// `options`, with the active one at `cursor`.
	///
	/// A `cursor` past the end of the list is not an error and is not clamped, because upstream's is
	/// not: the window still slides to the bottom, and no option is drawn as the active one.
	pub fn new(options: &'a [T], cursor: usize) -> Self {
		Self {
			options,
			cursor,
			columns: 80,
			rows: 20,
			max_items: None,
			column_padding: 0,
			row_padding: DEFAULT_ROW_PADDING,
			overflow: Theme::clack().styles.overflow,
		}
	}

	/// The terminal's width. Options are wrapped to it, less [`column_padding`](Self::with_column_padding).
	pub fn with_columns(mut self, columns: usize) -> Self {
		self.columns = columns;
		self
	}

	/// The terminal's height. What is left of it after
	/// [`row_padding`](Self::with_row_padding) is how many rows the list may occupy.
	pub fn with_rows(mut self, rows: usize) -> Self {
		self.rows = rows;
		self
	}

	/// A ceiling on the number of options drawn, over and above what the terminal allows.
	///
	/// Below [`MINIMUM_ITEMS`] it does nothing at all — including at zero, which is why this takes a
	/// number rather than being expressed by passing none.
	pub fn with_max_items(mut self, max_items: usize) -> Self {
		self.max_items = Some(max_items);
		self
	}

	/// Columns taken off the width before an option is wrapped.
	///
	/// `select` passes thirteen: the length of a styled bar and two spaces, which draws three
	/// columns (ADR-0019). A padding at or above the width leaves nothing to wrap into, and upstream
	/// keeps going — see [`wrap::breaks`](crate::wrap::breaks).
	pub fn with_column_padding(mut self, column_padding: usize) -> Self {
		self.column_padding = column_padding;
		self
	}

	/// Rows kept back for everything that is not the list.
	pub fn with_row_padding(mut self, row_padding: usize) -> Self {
		self.row_padding = row_padding;
		self
	}

	/// The Theme the overflow row is drawn in. Nothing else here is styled by this type.
	pub fn with_theme(mut self, theme: &Theme) -> Self {
		self.overflow = theme.styles.overflow;
		self
	}

	/// The rows to draw, in order, with an overflow row at either end where the list is cut.
	///
	/// `style` is handed each option and whether it is the active one, and returns the rows it
	/// occupies *before* wrapping — more than one where a label has a line break in it, which is
	/// upstream's `computeLabel`. An option the list does not actually have — reachable only through
	/// a cursor past the end — is drawn as one blank row, because upstream's `''` wraps to `['']`.
	pub fn lines(&self, style: impl Fn(&T, bool) -> Vec<Line>) -> Vec<Line> {
		let max_width = self.columns.saturating_sub(self.column_padding);
		let output_max_items = self.rows.saturating_sub(self.row_padding);
		let computed_max_items = self
			.max_items
			.unwrap_or(usize::MAX)
			.min(output_max_items)
			.max(MINIMUM_ITEMS);

		// Signed from here down. Upstream's window arithmetic subtracts freely and relies on
		// `Math.max(…, 0)` to catch what goes below zero; doing it in `usize` would mean guessing
		// which of those subtractions can wrap and which cannot.
		let count = self.options.len() as i64;
		let window = computed_max_items as i64;
		let cursor = self.cursor as i64;

		let mut start = 0i64;
		if cursor >= window - 3 {
			// Three, not zero: the window starts sliding two options before the cursor reaches the
			// bottom of it, so there is always something visible below the active option until the
			// list itself runs out.
			start = (cursor - window + 3).min(count - window).max(0);
		}

		let mut top = window < count && start > 0;
		let mut bottom = window < count && start + window < count;

		let end = (start + window).min(count);
		// An ellipsis row takes the place of an option rather than sitting above or below the
		// window, so the window shrinks by one at each end that has one.
		let first = start + i64::from(top);
		let last = end - i64::from(bottom);

		let mut line_count = i64::from(top) + i64::from(bottom);
		let mut groups: Vec<Vec<Line>> = Vec::new();
		for index in first..last {
			let rows = match usize::try_from(index)
				.ok()
				.and_then(|index| self.options.get(index))
			{
				Some(option) => style(option, index == cursor)
					.iter()
					.flat_map(|line| line.wrap(max_width))
					.collect::<Vec<_>>(),
				None => vec![Line::blank()],
			};
			line_count += rows.len() as i64;
			groups.push(rows);
		}

		if line_count > output_max_items as i64 {
			trim(
				&mut groups,
				&mut top,
				&mut bottom,
				line_count,
				cursor - first,
				output_max_items as i64,
			);
		}

		let mut out = Vec::new();
		if top {
			out.push(self.overflow_line());
		}
		out.extend(groups.into_iter().flatten());
		if bottom {
			out.push(self.overflow_line());
		}
		out
	}

	fn overflow_line(&self) -> Line {
		Span::styled(OVERFLOW, self.overflow).into()
	}
}

/// The second pass: the window fits the *list*, but its rows do not fit the terminal.
///
/// Only a multi-line or wrapped option gets here, since otherwise one option is one row and the
/// window already counted them. Groups are dropped whole, outwards from the one the cursor is on,
/// and whichever side gave anything up grows an ellipsis it did not have before.
///
/// The two branches are not symmetrical and the asymmetry is upstream's. With a top ellipsis already
/// there, the rows above the cursor go first and the rows below only if that was not enough. Without
/// one, the rows below go first — and the budget is docked a row before they do, to pay for the
/// bottom ellipsis that is about to appear, unless one is already there. On the second pass through
/// either branch the budget is docked again, for the other ellipsis.
fn trim(
	groups: &mut Vec<Vec<Line>>,
	top: &mut bool,
	bottom: &mut bool,
	line_count: i64,
	cursor_group: i64,
	output_max_items: i64,
) {
	let mut preceding = 0i64;
	let mut following = 0i64;
	let mut lines = line_count;
	let mut budget = output_max_items;

	if *top {
		(lines, preceding) = drop_groups(groups, lines, 0, cursor_group, budget, false);
		if lines > budget {
			if !*bottom {
				budget -= 1;
			}
			(_, following) = drop_groups(
				groups,
				lines,
				cursor_group + 1,
				groups.len() as i64,
				budget,
				true,
			);
		}
	} else {
		if !*bottom {
			budget -= 1;
		}
		(lines, following) = drop_groups(
			groups,
			lines,
			cursor_group + 1,
			groups.len() as i64,
			budget,
			true,
		);
		if lines > budget {
			budget -= 1;
			(_, preceding) = drop_groups(groups, lines, 0, cursor_group, budget, false);
		}
	}

	// Upstream splices both ends of one array, the second on what the first left behind, and neither
	// call can run off it. The forward walk takes at most `cursor_group` steps and the backwards one
	// at most `len - cursor_group - 1`, so together they never ask for more groups than there are;
	// and `cursor_group` cannot go below -1, because reaching that would need a window narrower than
	// three and the floor is five. Both are written as a subtraction that would rather panic than
	// quietly hand back a shorter list.
	if preceding > 0 {
		*top = true;
		groups.drain(..preceding as usize);
	}
	if following > 0 {
		*bottom = true;
		let keep = groups
			.len()
			.checked_sub(following as usize)
			.expect("the two trims together removed more groups than the window held");
		groups.truncate(keep);
	}
}

/// Upstream's `trimLines`: drop whole groups until the budget is met, counting every step.
///
/// Returns the rows left and the number of steps taken. A step over an index the list does not have
/// costs nothing and still counts, because upstream's `groups[i]` is `undefined` there and its
/// `removals++` runs anyway.
fn drop_groups(
	groups: &[Vec<Line>],
	line_count: i64,
	start: i64,
	end: i64,
	max_lines: i64,
	from_end: bool,
) -> (i64, i64) {
	let mut lines = line_count;
	let mut removals = 0i64;

	let step = |index: i64, lines: &mut i64, removals: &mut i64| -> bool {
		if let Some(group) = usize::try_from(index).ok().and_then(|i| groups.get(i)) {
			*lines -= group.len() as i64;
		}
		*removals += 1;
		*lines <= max_lines
	};

	if from_end {
		let mut index = end - 1;
		while index >= start {
			if step(index, &mut lines, &mut removals) {
				break;
			}
			index -= 1;
		}
	} else {
		let mut index = start;
		while index < end {
			if step(index, &mut lines, &mut removals) {
				break;
			}
			index += 1;
		}
	}

	(lines, removals)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// `n` options named the way upstream's own suite names them.
	fn items(n: usize) -> Vec<String> {
		(1..=n).map(|i| format!("Item {i}")).collect()
	}

	/// Each row as plain text, with the overflow row spelled out. The corpus test compares styles;
	/// these are about the arithmetic.
	fn text(lines: &[Line]) -> Vec<String> {
		lines
			.iter()
			.map(|line| line.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	fn plain(option: &String, _active: bool) -> Vec<Line> {
		vec![Span::raw(option).into()]
	}

	#[test]
	fn a_list_that_fits_comes_back_whole() {
		let options = items(3);
		let lines = LimitOptions::new(&options, 0)
			.with_max_items(5)
			.lines(plain);
		assert_eq!(text(&lines), ["Item 1", "Item 2", "Item 3"]);
	}

	#[test]
	fn an_empty_list_draws_nothing() {
		let options: Vec<String> = Vec::new();
		assert!(LimitOptions::new(&options, 0).lines(plain).is_empty());
	}

	#[test]
	fn max_items_below_the_floor_is_ignored() {
		let options = items(7);
		let lines = LimitOptions::new(&options, 0)
			.with_max_items(3)
			.lines(plain);
		assert_eq!(
			text(&lines),
			["Item 1", "Item 2", "Item 3", "Item 4", OVERFLOW]
		);
	}

	#[test]
	fn the_window_slides_two_options_before_the_cursor_reaches_the_bottom() {
		let options = items(10);
		let at = |cursor| {
			text(
				&LimitOptions::new(&options, cursor)
					.with_max_items(5)
					.lines(plain),
			)
		};

		// Five rows, so three options between two ellipses once it has moved. The cursor is on the
		// third of them at index 2 and the window has not moved; at index 3 it has.
		assert_eq!(at(1), ["Item 1", "Item 2", "Item 3", "Item 4", OVERFLOW]);
		assert_eq!(at(2), ["Item 1", "Item 2", "Item 3", "Item 4", OVERFLOW]);
		assert_eq!(at(3), [OVERFLOW, "Item 3", "Item 4", "Item 5", OVERFLOW]);
	}

	#[test]
	fn the_window_stops_at_the_end_of_the_list() {
		let options = items(10);
		let lines = LimitOptions::new(&options, 9)
			.with_max_items(5)
			.lines(plain);
		assert_eq!(
			text(&lines),
			[OVERFLOW, "Item 7", "Item 8", "Item 9", "Item 10"]
		);
	}

	#[test]
	fn a_cursor_past_the_end_still_slides_the_window() {
		let options = items(10);
		let lines = LimitOptions::new(&options, 12)
			.with_max_items(5)
			.lines(plain);
		assert_eq!(
			text(&lines),
			[OVERFLOW, "Item 7", "Item 8", "Item 9", "Item 10"]
		);
	}

	#[test]
	fn the_style_callback_is_told_which_option_is_active() {
		let options = items(3);
		let lines = LimitOptions::new(&options, 1).lines(|option, active| {
			vec![
				Span::raw(if active {
					format!("> {option}")
				} else {
					format!("  {option}")
				})
				.into(),
			]
		});
		assert_eq!(text(&lines), ["  Item 1", "> Item 2", "  Item 3"]);
	}

	#[test]
	fn a_multi_line_option_is_several_rows() {
		let options = vec!["Item 1\nContinued".to_string(), "Item 2".to_string()];
		let lines = LimitOptions::new(&options, 0).lines(|option, _| {
			option
				.split('\n')
				.map(|line| Span::raw(line).into())
				.collect()
		});
		assert_eq!(text(&lines), ["Item 1", "Continued", "Item 2"]);
	}

	#[test]
	fn an_option_wider_than_the_terminal_is_wrapped_before_it_is_counted() {
		let options = vec!["a rather long option".to_string()];
		let lines = LimitOptions::new(&options, 0).with_columns(10).lines(plain);
		assert_eq!(text(&lines), ["a rather ", "long ", "option"]);
	}

	#[test]
	fn a_padding_as_wide_as_the_terminal_leaves_nothing_to_wrap_into() {
		// Upstream divides by zero here rather than refusing, and lays every code point on a row of
		// its own with a blank in front. Reachable from `select` in a terminal thirteen columns wide.
		let options = vec!["ab".to_string()];
		let lines = LimitOptions::new(&options, 0)
			.with_columns(10)
			.with_column_padding(10)
			.lines(plain);
		assert_eq!(text(&lines), ["", "a", "b"]);
	}

	#[test]
	fn the_overflow_row_carries_the_themes_style() {
		let options = items(9);
		let lines = LimitOptions::new(&options, 0)
			.with_theme(&Theme::clack())
			.with_max_items(5)
			.lines(plain);
		let last = lines.last().expect("a row");
		assert_eq!(last.spans[0].text, OVERFLOW);
		assert_eq!(last.spans[0].style, Theme::clack().styles.overflow);
	}

	#[test]
	fn a_terminal_too_short_for_the_floor_gets_the_floor_anyway() {
		// `Math.max(…, 5)` applies to the terminal as well as to `max_items`, so a list five rows
		// tall is drawn into three rows of room and simply overruns.
		let options = items(7);
		let lines = LimitOptions::new(&options, 0).with_rows(3).lines(plain);
		assert_eq!(text(&lines), ["Item 1", OVERFLOW]);
	}
}
