//! `cargo run -p clark --example text`
//!
//! The first thing in this project that is a program rather than a test.

use clark::{ClackError, text};

fn main() -> Result<(), ClackError> {
	let album = text("Which Clark album would you hand a newcomer?")
		.placeholder("Body Riddle")
		.validate(|value: Option<&String>| {
			value
				.filter(|value| !value.trim().is_empty())
				.is_none()
				.then(|| "Please name one.".to_owned())
		})
		.interact_opt()?;

	match album {
		Some(album) => println!("{album}, then."),
		None => println!("Never mind, then."),
	}

	Ok(())
}
