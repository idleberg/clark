//! `cargo run -p clark --example group_multiselect`
//!
//! A `multiselect` whose options come under headings — and, unless you turn that off, whose
//! headings can be ticked to take everything under them.

use clark::{ClackError, SelectOption, group_multiselect};

fn main() -> Result<(), ClackError> {
	clark::intro("Warp back catalogue");

	let tracks = group_multiselect("Build a Clark playlist")
		// a group whose options are labelled by themselves
		.group("Clarence Park", ["Lord of the Dance", "Gorgeous Bull"])
		// one built by hand, so an option can carry a hint or be disabled
		.choices(
			"Body Riddle",
			[
				SelectOption::new("Herr Bar"),
				SelectOption::new("Ted").with_hint("the single"),
				SelectOption::new("Herzog"),
			],
		)
		.choices(
			"Death Peak",
			[
				SelectOption::new("Peak Magnetic"),
				SelectOption::new("Hoova"),
				SelectOption::new("Catastrophe Anthem").with_hint("with a children's choir"),
			],
		)
		.choices(
			"Sus Dog",
			[
				SelectOption::new("Alyosha"),
				SelectOption::new("Town Crank"),
				SelectOption::new("Dolgoch Tape")
					.with_hint("not cleared for streaming")
					.with_disabled(true),
			],
		)
		.initial_values(["Herzog"])
		.cursor_at("Peak Magnetic")
		.selectable_groups(true) // the default; `false` makes the headings labels only
		.max_items(10)
		.interact_opt()?;

	match tracks {
		Some(tracks) => clark::outro(format!("{} tracks queued.", tracks.len())),
		None => clark::outro("Never mind, then."),
	}

	Ok(())
}
