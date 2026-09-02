//! `cargo run -p clark --example select`
//!
//! One answer out of a list, with the arrow keys.

use clark::{ClackError, SelectOption, select};

fn main() -> Result<(), ClackError> {
	clark::intro("Body Riddle");

	let track = select("Pick your favourite track")
		// an option labelled by its own value
		.option("01. Herr Bar")
		// one built by hand, so it can carry a hint or be disabled
		.choice(SelectOption::new("02. Ted").with_hint("the single"))
		.option("03. Springtime Epic")
		.option("04. Vengeance Drools")
		.choice(SelectOption::new("05. Roulette Thrift Run").with_hint("the one everyone names"))
		.option("06. Herzog")
		.option("07. Matthew Unburdened")
		.option("08. Night Knuckles")
		.option("09. Frau Wav")
		.option("10. The Autumnal Crush")
		.choice(
			SelectOption::new("11. Dew on the Mouth")
				.with_hint("scratched on this copy")
				.with_disabled(true),
		)
		.initial_value("05. Roulette Thrift Run")
		.max_items(6)
		.interact_opt()?;

	match track {
		Some(track) => clark::outro(format!("{track} it is.")),
		None => clark::outro("Never mind, then."),
	}

	Ok(())
}
