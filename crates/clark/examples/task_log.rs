//! `cargo run -p clark --example task_log`
//!
//! Rows that go away when the task works, and stay when it does not.

use std::thread::sleep;
use std::time::Duration;

use clark::task_log;

fn main() {
	clark::intro("Bleep");

	let mut log = task_log("Transcoding Clark — Sus Dog")
		.limit(3) // how many rows are on screen at once
		.retain_log(false) // the default; `true` keeps them all after a success
		.start();

	for track in ["Alyosha", "Town Crank", "Medicine", "Cordelia", "Wedding"] {
		sleep(Duration::from_millis(400));
		log.message(format!("{track} → mp3 320"));
	}
	log.success("5 tracks transcoded");

	// A group is a nested log with a heading and an outcome of its own.
	let mut log = task_log("Tagging").start();
	let artwork = log.group("Artwork");
	for size in ["600×600", "1400×1400"] {
		sleep(Duration::from_millis(400));
		log.group_message(&artwork, format!("cover {size}"));
	}
	log.group_success(&artwork, "Cover embedded");
	log.message("Wrote ID3 tags");
	sleep(Duration::from_millis(400));

	// An error keeps its rows on screen, where a success clears them.
	log.error("No catalogue number for this release");

	clark::outro("Two of three steps done.");
}
