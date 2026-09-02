//! `cargo run -p clark --example path`
//!
//! The one Prompt that reads something outside the terminal. Type a path and the list keeps up;
//! the directory under the cursor is read again after every keystroke.

use clark::{ClackError, intro, outro, path};

fn main() -> Result<(), ClackError> {
	intro("clark");

	let directory = path("Which directory?")
		.root(".")
		.directory(true)
		.interact_opt()?;

	let Some(directory) = directory else {
		outro("Never mind, then.");
		return Ok(());
	};

	let file = path("And which file in it?")
		.root(format!("{directory}/"))
		.interact_opt()?;

	match file {
		Some(file) => outro(format!("{file} it is.")),
		None => outro("Never mind, then."),
	}

	Ok(())
}
