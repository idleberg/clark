//! Ported from `@clack/core`'s `utils/cursor.ts` — where the cursor of a list Prompt lands next.
//!
//! One function, shared by every Prompt with a list in it. `findTextCursor`, the other half of
//! upstream's file, belongs to `multiline` and is not here yet.

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
}
