//! `cargo run -p clark --example date`
//!
//! Three segments the arrow keys move between, and a validator that runs on each of them.

use clark::{CivilDate, ClackError, DateFormat, DateMessages, date};

fn main() -> Result<(), ClackError> {
	clark::intro("Warp");

	let released = date("When was Body Riddle released?")
		.format(DateFormat::Dmy)
		.separator("/")
		.initial_value(CivilDate::new(2006, 10, 1).unwrap())
		.min_date(CivilDate::new(2001, 1, 1).unwrap()) // Clarence Park
		.max_date(CivilDate::new(2023, 5, 26).unwrap()) // Sus Dog
		.messages(DateMessages {
			required: "Have a guess.".into(),
			after_min: "Clark's first album is from {date}.".into(),
			before_max: "Nothing after {date} yet.".into(),
			..DateMessages::default()
		})
		.validate(|value: Option<&CivilDate>| {
			value
				.filter(|date| date.year() != 2006)
				.map(|_| "Close — it was 2006.".to_owned())
		})
		.interact_opt()?;

	match released {
		Some(released) => clark::outro(format!("{} — near enough.", released.iso())),
		None => clark::outro("Never mind, then."),
	}

	Ok(())
}
