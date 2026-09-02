//! `cargo run -p clark --example confirm`
//!
//! Two options and a yes/no answer.

use clark::{ClackError, confirm};

fn main() -> Result<(), ClackError> {
	clark::intro("Body Riddle");

	let repeat = confirm("Play it again?")
		.active("Of course")
		.inactive("Move on to Turning Dragon")
		.initial_value(true)
		.interact_opt()?;

	match repeat {
		Some(true) => clark::outro("Side A again, then."),
		Some(false) => clark::outro("Turning Dragon it is."),
		None => clark::outro("Never mind, then."),
	}

	// `vertical` puts the two options on their own lines instead of side by side.
	let vinyl = confirm("Order the vinyl too?")
		.vertical(true)
		.initial_value(false)
		.interact()?;

	if vinyl {
		clark::log::success("Added one 2×LP to the basket.");
	}

	Ok(())
}
