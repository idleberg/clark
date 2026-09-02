//! `cargo run -p clark --example spinner`
//!
//! The one unit whose behaviour a test cannot show you: it moves.

use std::thread::sleep;
use std::time::Duration;

use clark::{Indicator, spinner};

fn main() {
	clark::intro("Bleep");

	let dots = spinner().start("Reading the CD table of contents");
	sleep(Duration::from_millis(1200));
	dots.message("Clark — Body Riddle, 11 tracks");
	sleep(Duration::from_millis(1200));
	dots.stop("Disc identified");

	let timer = spinner()
		.indicator(Indicator::Timer)
		.start("Ripping to FLAC");
	sleep(Duration::from_millis(2500));
	timer.stop("Ripped");

	let quiet = spinner().start("Ejecting");
	sleep(Duration::from_millis(800));
	quiet.clear();

	clark::outro("Enjoy the record.");
}
