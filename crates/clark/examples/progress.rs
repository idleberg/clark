//! `cargo run -p clark --example progress`
//!
//! A spinner whose message is a bar.

use std::thread::sleep;
use std::time::Duration;

use clark::{BarStyle, progress};

const TRACKS: [&str; 7] = [
	"Spring But Dark",
	"Butterfly Prowler",
	"Peak Magnetic",
	"Slap Drones",
	"Hoova",
	"Aftermath",
	"Catastrophe Anthem",
];

fn main() {
	clark::intro("Bleep");

	let bar = progress()
		.style(BarStyle::Heavy)
		.max(TRACKS.len())
		.size(24)
		.start("Downloading Clark — Death Peak");

	for (index, track) in TRACKS.iter().enumerate() {
		sleep(Duration::from_millis(500));
		bar.advance(
			1,
			Some(&format!("{track} ({}/{})", index + 1, TRACKS.len())),
		);
	}

	bar.stop("7 tracks downloaded");

	clark::outro("Filed under Warp.");
}
