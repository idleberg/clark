//! `cargo run -p clark --example message`
//!
//! Everything that prints without asking: the two ends of a run, the boxes, and the log lines
//! beside the Guide's bar.

fn main() {
	clark::intro("Bleep — order 4412");

	clark::log::message("Basket:");
	clark::log::step("Clark — Body Riddle (2×LP)");
	clark::log::step("Clark — Death Peak (CD)");
	clark::log::info("Both ship from Sheffield.");
	clark::log::warn("Playground in a Lake is out of stock.");
	clark::log::success("Payment taken.");
	clark::log::error("The signed sleeve could not be reserved.");

	// A note is a box beside the Guide; a box stands on its own.
	clark::note("Body Riddle\nWarp Records, 2006\nWARPCD140", "Now shipping");
	clark::r#box(
		"Downloads stay in your account forever.\nRe-download them any time.",
		"Bleep",
	);

	clark::outro("Thanks — enjoy the records.");

	// The other way a run ends: `cancel` in place of `outro`.
	clark::intro("Bleep — order 4413");
	clark::cancel("Basket abandoned.");
}
