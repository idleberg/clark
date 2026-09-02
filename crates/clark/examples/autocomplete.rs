//! `cargo run -p clark --example autocomplete`
//!
//! A select whose list narrows as you type — and its multi-select sibling, which keeps the search
//! box and the ticks at once.

use clark::{ClackError, SelectOption, autocomplete, autocomplete_multiselect};

fn main() -> Result<(), ClackError> {
	clark::intro("Bleep");

	let album = autocomplete("Search the Clark discography")
		.option("Clarence Park")
		.option("Empty the Bones of You")
		.choice(SelectOption::new("Body Riddle").with_hint("2006"))
		.option("Turning Dragon")
		.option("Totems Flare")
		.option("Iradelphic")
		.option("Death Peak")
		.option("Playground in a Lake")
		.choice(SelectOption::new("Sus Dog").with_hint("2023"))
		.placeholder("start typing")
		.max_items(6)
		.interact_opt()?;

	let Some(album) = album else {
		clark::outro("Never mind, then.");
		return Ok(());
	};

	let formats = autocomplete_multiselect::<&str>(format!("Which pressings of {album}?"))
		.labelled("wav", "WAV 24-bit")
		.labelled("flac", "FLAC")
		.labelled("mp3", "MP3 320")
		.labelled("2lp", "2×LP")
		.labelled("cd", "CD")
		// what the search matches, in place of the default (label, then hint)
		.filter(|search, option| {
			option
				.label()
				.to_lowercase()
				.contains(&search.to_lowercase())
		})
		.required(true) // off by default here, unlike `multiselect`
		.interact_opt()?;

	match formats {
		Some(formats) => clark::outro(format!("{album}: {formats:?}")),
		None => clark::outro("Never mind, then."),
	}

	Ok(())
}
