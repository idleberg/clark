use clackatui::{ClackError, SelectOption};

fn main() {
	match pick() {
		Ok(toppings) => println!("\nyou chose: {toppings:?}"),
		Err(ClackError::Cancelled) => println!("\nnothing chosen"),
		Err(err) => eprintln!("\n{err:?}"),
	}
}

fn pick() -> Result<Vec<String>, ClackError> {
	clackatui::multiselect("Pick your toppings")
		// an option labelled by its own value
		.option("cheese".to_string())
		// one built by hand, so it can carry a hint or be disabled
		.choice(SelectOption::new("basil".to_string()).with_hint("fresh"))
		.choice(SelectOption::new("olives".to_string()))
		.choice(
			SelectOption::new("pineapple".to_string())
				.with_hint("we're out")
				.with_disabled(true),
		)
		.option("mushrooms".to_string())
		.initial_values(["cheese".to_string()])
		.cursor_at("olives".to_string())
		.max_items(6)
		.required(true) // the default; `false` lets an empty answer through
		.interact()
}
