//! `cargo run -p clackatui --example spinner`
//!
//! The one unit whose behaviour a test cannot show you: it moves.

use std::thread::sleep;
use std::time::Duration;

use clackatui::{Indicator, spinner};

fn main() {
	clackatui::intro("clackatui");

	let dots = spinner().start("Installing dependencies");
	sleep(Duration::from_millis(1200));
	dots.message("Resolving versions");
	sleep(Duration::from_millis(1200));
	dots.stop("Installed 42 packages");

	let timer = spinner().indicator(Indicator::Timer).start("Building");
	sleep(Duration::from_millis(2500));
	timer.stop("Built");

	let quiet = spinner().start("Cleaning up");
	sleep(Duration::from_millis(800));
	quiet.clear();

	// A progress bar is a spinner whose message is a bar, so it belongs in the same example.
	let bar = clackatui::progress().max(5).size(20).start("Copying files");
	for file in 1..=5 {
		sleep(Duration::from_millis(400));
		bar.advance(1, Some(&format!("Copying files ({file}/5)")));
	}
	bar.stop("Copied 5 files");

	// A task log is neither, but it is the other renderer a program drives by calling it: rows that
	// go away when the task works, and stay when it does not.
	let mut log = clackatui::task_log("Running tests").limit(3).start();
	for suite in ["frame", "emitter", "wrap", "spinner", "task-log"] {
		sleep(Duration::from_millis(400));
		log.message(format!("{suite} … ok"));
	}
	log.success("5 suites passed");

	clackatui::outro("You're all set!");
}
