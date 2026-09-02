//! `cargo run -p clark --example select_key`
//!
//! No arrow keys: the list settles on the first keypress that matches an option's value, so the
//! values want to be one character each.

use clark::{ClackError, select_key};

fn main() -> Result<(), ClackError> {
	clark::intro("Now playing: Clark — Peak Magnetic");

	let action = select_key("What next?")
		.labelled("p", "Play it again")
		.labelled("n", "Next track")
		.labelled("q", "Queue the whole of Death Peak")
		.labelled("x", "Stop")
		.initial_value("p")
		.case_sensitive(false) // the default; `true` makes `P` and `p` two options
		.interact_opt()?;

	match action {
		Some("x") | None => clark::outro("Stopped."),
		Some(key) => clark::outro(format!("Pressed {key}.")),
	}

	Ok(())
}
