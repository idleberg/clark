//! `cargo run -p clackatui --example text`
//!
//! The first thing in this project that is a program rather than a test.

use clackatui::{ClackError, text};

fn main() -> Result<(), ClackError> {
	let name = text("What is your name?")
		.placeholder("Jan")
		.validate(|value: Option<&String>| {
			value
				.filter(|value| !value.trim().is_empty())
				.is_none()
				.then(|| "Please enter a name.".to_owned())
		})
		.interact_opt()?;

	match name {
		Some(name) => println!("Hello, {name}."),
		None => println!("Never mind, then."),
	}

	Ok(())
}
