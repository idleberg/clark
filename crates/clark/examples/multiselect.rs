use clark::{ClackError, SelectOption};

fn main() {
	match pick() {
		Ok(albums) => println!("\nyou chose: {albums:?}"),
		Err(ClackError::Cancelled) => println!("\nnothing chosen"),
		Err(err) => eprintln!("\n{err:?}"),
	}
}

fn pick() -> Result<Vec<String>, ClackError> {
	clark::multiselect("Pick your favourite Clark albums")
		// an option labelled by its own value
		.option("Clarence Park".to_string())
		// one built by hand, so it can carry a hint or be disabled
		.choice(SelectOption::new("Empty the Bones of You".to_string()).with_hint("2003"))
		.choice(SelectOption::new("Body Riddle".to_string()).with_hint("2006"))
		.choice(SelectOption::new("Totems Flare".to_string()).with_hint("2009"))
		.choice(
			SelectOption::new("Feast/Beast".to_string())
				.with_hint("remixes, not an album")
				.with_disabled(true),
		)
		.option("Death Peak".to_string())
		.option("Sus Dog".to_string())
		.initial_values(["Body Riddle".to_string()])
		.cursor_at("Death Peak".to_string())
		.max_items(6)
		.required(true) // the default; `false` lets an empty answer through
		.interact()
}
