//! `cargo run -p clark --example multiline`
//!
//! A text field where `enter` is a newline, and the Prompt is submitted with the key the footer
//! names.

use clark::{ClackError, multiline};

fn main() -> Result<(), ClackError> {
	clark::intro("Bleep — write a review");

	let review = multiline("Say something about Sus Dog")
		.placeholder("Thom Yorke sings on three of these…")
		.default_value("No notes.")
		.show_submit(true) // the default; `false` hides the footer
		.validate(|value: Option<&String>| {
			value
				.filter(|value| value.lines().count() > 5)
				.map(|_| "Five lines is plenty.".to_owned())
		})
		.interact_opt()?;

	match review {
		Some(review) => clark::note(review, "Your review"),
		None => clark::cancel("Never mind, then."),
	}

	Ok(())
}
