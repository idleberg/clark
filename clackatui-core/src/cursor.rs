//! Ported from `@clack/core`'s `utils/cursor.ts` — where a cursor lands next.
//!
//! Two functions with nothing in common but a file. [`find_cursor`] walks an option list and is
//! shared by every Prompt with one in it; [`find_text_cursor`] walks a block of text by rows and
//! columns and belongs to [`multi_line`](crate::multi_line) alone.

/// `findCursor`: the next selectable option in `delta`'s direction, wrapping at either end.
///
/// `disabled` is the only thing asked of an option, so this takes a predicate rather than a type: a
/// list Prompt holds its own kind of option and none of them are this module's business.
///
/// Three behaviours are upstream's and worth naming, because each looks like a bug until it is read
/// beside the others:
///
/// - **A list with nothing selectable in it does not move.** The cursor stays exactly where it was,
///   including on a disabled option, and including where it is past the end of the list.
/// - **The wrap is not modular.** A cursor below zero lands on the *last* option and one past the
///   end lands on the *first*, whatever the step was — so a `delta` of ten from the middle of a
///   short list wraps to the first option rather than counting around.
/// - **The walk keeps the direction, not the distance.** Once the first step has been taken, every
///   later one is a single option, because upstream recurses with `delta < 0 ? -1 : 1`.
///
/// Written as a loop where upstream recurses. The walk is the same one — the recursion is in tail
/// position and its argument list is this loop's state — and a list long enough to matter is a list
/// long enough to overflow a stack.
pub fn find_cursor<T>(
	cursor: usize,
	delta: isize,
	options: &[T],
	disabled: impl Fn(&T) -> bool,
) -> usize {
	if !options.iter().any(|option| !disabled(option)) {
		return cursor;
	}

	// `Math.max(options.length - 1, 0)` — and the list is not empty, since it has an option that is
	// not disabled.
	let max = options.len() - 1;
	let mut at = cursor as isize;
	let mut step = delta;

	loop {
		let next = at + step;
		let clamped = if next < 0 {
			max
		} else if next as usize > max {
			0
		} else {
			next as usize
		};

		// `options[clamped]?.disabled` — an index the list does not have is not disabled, which is
		// unreachable here because `clamped` has just been clamped into it.
		if !disabled(&options[clamped]) {
			return clamped;
		}

		at = clamped as isize;
		step = if step < 0 { -1 } else { 1 };
	}
}

/// `findTextCursor`: where an offset into `value` lands after moving `delta_x` columns and
/// `delta_y` rows.
///
/// The offset is a count of characters, not of bytes — upstream's is a UTF-16 index, and the two
/// agree on everything but astral characters, where upstream counts two and this counts one. That is
/// the same trade [`InputWithCursor`](crate::prompt::InputWithCursor) makes: a cursor here always
/// sits between characters, where upstream's can land inside one.
///
/// Three things it does that a cursor moved by rows and columns would not:
///
/// - **A vertical move remembers no column.** `delta_y` carries the offset *within the current row*
///   to the new one and clamps it to that row's length, so walking down a short line and back up
///   does not come home. Upstream keeps no goal column and neither does this.
/// - **A horizontal move crosses rows.** Left from column zero lands at the end of the row above,
///   because the two `while` loops carry an out-of-range column onto its neighbour — and they run
///   after a `delta_y` too, though a vertical move can never leave a column out of range.
/// - **The ends absorb.** Left at the very start and right at the very end both stay put: the loops
///   stop at the first and last row, and the final clamp takes the rest.
pub fn find_text_cursor(cursor: usize, delta_x: isize, delta_y: isize, value: &str) -> usize {
	// Only the lengths are ever asked for. `split` on a `\n` never yields nothing, so there is
	// always at least one row — an empty `value` is one row of length zero.
	let lines: Vec<isize> = value
		.split('\n')
		.map(|line| line.chars().count() as isize)
		.collect();
	let last = lines.len() - 1;

	// Which row the offset is on, and how far into it. An offset past the end of the text leaves
	// `row` past the last row, which the clamp below is what takes back — so nothing indexes yet.
	let mut row = 0usize;
	let mut column = cursor as isize;
	for length in &lines {
		if column <= *length {
			break;
		}
		column -= *length + 1;
		row += 1;
	}

	// `Math.max(0, Math.min(lines.length - 1, cursorY + deltaY))`. Written as one `clamp` because
	// `last` is never negative, which also means a mutation that splits it back into a `max` and a
	// `min` survives — equivalent, not untested.
	row = (row as isize + delta_y).clamp(0, last as isize) as usize;
	column = column.min(lines[row]) + delta_x;

	while column < 0 && row > 0 {
		row -= 1;
		column += lines[row] + 1;
	}
	while column > lines[row] && row < last {
		column -= lines[row] + 1;
		row += 1;
	}
	column = column.clamp(0, lines[row]);

	lines[..row]
		.iter()
		.map(|length| *length as usize + 1)
		.sum::<usize>()
		+ column as usize
}

#[cfg(test)]
mod tests {
	use super::*;

	/// `true` where the option is disabled, so a case reads as the list it describes.
	fn walk(cursor: usize, delta: isize, disabled: &[bool]) -> usize {
		find_cursor(cursor, delta, disabled, |d| *d)
	}

	#[test]
	fn a_step_moves_one_option() {
		assert_eq!(walk(0, 1, &[false, false, false]), 1);
		assert_eq!(walk(2, -1, &[false, false, false]), 1);
	}

	#[test]
	fn the_ends_wrap_around() {
		assert_eq!(walk(2, 1, &[false, false, false]), 0);
		assert_eq!(walk(0, -1, &[false, false, false]), 2);
	}

	#[test]
	fn a_disabled_option_is_stepped_over_in_the_direction_of_travel() {
		assert_eq!(walk(0, 1, &[false, true, true, false]), 3);
		assert_eq!(walk(3, -1, &[false, true, true, false]), 0);
	}

	#[test]
	fn stepping_over_a_disabled_option_at_the_end_wraps_and_keeps_going() {
		assert_eq!(walk(1, 1, &[false, false, true]), 0);
		assert_eq!(walk(0, -1, &[true, false, false]), 2);
	}

	/// The guard upstream opens with, and the reason the loop above terminates.
	#[test]
	fn a_list_with_nothing_selectable_leaves_the_cursor_alone() {
		assert_eq!(walk(1, 1, &[true, true, true]), 1);
		assert_eq!(walk(0, 1, &[]), 0);
		assert_eq!(walk(9, -1, &[true]), 9);
	}

	/// Not modular arithmetic: anything past the end is the first option, however far past.
	#[test]
	fn a_step_past_the_end_lands_on_the_first_option_rather_than_counting_around() {
		assert_eq!(walk(0, 10, &[false, false, false]), 0);
		assert_eq!(walk(2, -10, &[false, false, false]), 2);
	}

	// --- find_text_cursor ---------------------------------------------------------------------

	/// `ab\ncde`: `a`0 `b`1 `\n`2 `c`3 `d`4 `e`5.
	const TEXT: &str = "ab\ncde";

	fn left(at: usize) -> usize {
		find_text_cursor(at, -1, 0, TEXT)
	}
	fn right(at: usize) -> usize {
		find_text_cursor(at, 1, 0, TEXT)
	}
	fn up(at: usize) -> usize {
		find_text_cursor(at, 0, -1, TEXT)
	}
	fn down(at: usize) -> usize {
		find_text_cursor(at, 0, 1, TEXT)
	}

	#[test]
	fn a_horizontal_move_steps_one_character() {
		assert_eq!(right(0), 1);
		assert_eq!(left(1), 0);
		assert_eq!(right(4), 5);
	}

	/// The newline is a position, not a character to step over: right from the end of a row lands on
	/// the first character of the next.
	#[test]
	fn a_horizontal_move_crosses_a_row_boundary() {
		assert_eq!(right(2), 3);
		assert_eq!(left(3), 2);
	}

	#[test]
	fn the_two_ends_absorb_a_horizontal_move() {
		assert_eq!(left(0), 0);
		assert_eq!(right(6), 6);
	}

	#[test]
	fn a_vertical_move_keeps_the_offset_into_the_row() {
		assert_eq!(down(0), 3);
		assert_eq!(down(1), 4);
		assert_eq!(up(5), 2);
	}

	/// No goal column is remembered, so a row too short to hold the offset clamps it — and coming
	/// back does not restore it.
	#[test]
	fn a_vertical_move_forgets_the_column_it_came_from() {
		assert_eq!(up(5), 2, "e is column 2, and row 0 ends at column 2");
		assert_eq!(down(2), 5);
		// Down from the end of the short row and back up lands one column left of where it started.
		assert_eq!(up(down(2)), 2);
	}

	#[test]
	fn the_first_and_last_rows_absorb_a_vertical_move() {
		assert_eq!(up(1), 1);
		assert_eq!(down(4), 4);
	}

	#[test]
	fn a_text_with_no_break_in_it_is_one_row() {
		assert_eq!(find_text_cursor(1, 0, -1, "abc"), 1);
		assert_eq!(find_text_cursor(1, 0, 1, "abc"), 1);
		assert_eq!(find_text_cursor(3, 1, 0, "abc"), 3);
	}

	#[test]
	fn an_empty_text_has_nowhere_to_go() {
		for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
			assert_eq!(find_text_cursor(0, dx, dy, ""), 0);
		}
	}

	/// A blank row between two others is a row: moving down onto it lands at its only position.
	#[test]
	fn a_blank_row_is_still_a_row() {
		assert_eq!(find_text_cursor(1, 0, 1, "ab\n\ncd"), 3);
		assert_eq!(find_text_cursor(3, 0, 1, "ab\n\ncd"), 4);
	}

	/// Characters, not bytes: an accented letter is one step, and a row holding one is one column
	/// wide as far as a vertical move is concerned.
	#[test]
	fn the_offset_counts_characters_rather_than_bytes() {
		assert_eq!(find_text_cursor(0, 1, 0, "é\nx"), 1);
		assert_eq!(find_text_cursor(1, 1, 0, "é\nx"), 2);
		// Offset 2 is *before* the `x`, because the newline occupies offset 1 — so it is column 0.
		assert_eq!(find_text_cursor(2, 0, -1, "é\nx"), 0);
		assert_eq!(find_text_cursor(3, 0, -1, "é\nx"), 1);
	}
}
