//! `cargo run -p clark --example password`
//!
//! A text field that draws a mask instead of what was typed.

use clark::{ClackError, password};

fn main() -> Result<(), ClackError> {
	clark::intro("Bleep");

	let secret = password("Password for your Bleep account")
		.mask("▪")
		.clear_on_error(true) // start over rather than edit a rejected password
		.validate(|value: Option<&String>| {
			value
				.filter(|value| value.len() >= 8)
				.is_none()
				.then(|| "At least eight characters, please.".to_owned())
		})
		.interact_opt()?;

	match secret {
		Some(secret) => clark::outro(format!("Signed in ({} characters).", secret.len())),
		None => clark::outro("Never mind, then."),
	}

	Ok(())
}
