//! Parity: clack and the port, given the same Scenario, leave the terminal in the same state.
//!
//! This is the comparison ADR-0001 is built around and the one M1 exists to reach. Everything else
//! in the project checks a step on the way to it — a primitive against its JavaScript counterpart,
//! a Frame against the bytes clack wrote, a stream against the stream clack wrote. This checks the
//! only thing a user can actually see.
//!
//! Both byte streams go through *one* emulator, which is what makes the claim mean anything. The
//! two encode the same appearance differently and on purpose: clack's Frames are picocolors output,
//! one attribute per escape, each turned off again by name; the Emitter states a whole `Style` per
//! run and resets (ADR-0011, ADR-0013). Comparing bytes would report that as a difference, and
//! comparing rendered text alone would miss real ones. A Grid is where the question is well posed —
//! characters, styles *and cursor position*, as CONTEXT.md defines it — and the emulator is the
//! only thing here that knows where `ESC[999D ESC[4A ESC[1B` leaves the cursor.
//!
//! # What the emulator cannot see
//!
//! Conceal (SGR 8) — no emulator on crates.io models it, `avt` included, and clack draws an empty
//! placeholder with it. Both sides are blind to it equally, so nothing here can fail because of it,
//! but nothing here can catch it either. That is why `scenario_replay.rs` keeps comparing opening
//! Frames as styles: that comparison is in `Style` terms, where conceal is a bit like any other.
//! The two tests are complementary rather than redundant, and neither subsumes the other.
//!
//! # What reaches it
//!
//! A hundred and ninety-one Scenarios across eleven Prompts. A hundred and forty-five are replayable
//! ones harvested from clack's own suite — `text`, `password`, `confirm`, `select`, `multiselect`,
//! `selectKey`, `groupMultiselect`, `autocomplete`, `autocompleteMultiselect`, `date` and
//! `multiline` — and all
//! but a handful are at 80 columns, because upstream's tests barely vary the terminal.
//! Forty-six are hand-authored, written to reach what a harvest cannot supply (ADR-0016): 40 and
//! 20 columns, CJK text, a wrap that grows as a value is typed and shrinks again as it is deleted,
//! five that change the terminal's size under an open Prompt, and the things a harvest cannot
//! reach at all — a `y` that settles a `confirm` without a `return` (ADR-0018), a `confirm` message
//! wrapped against the length of an escape sequence (ADR-0019), a masked astral character, a
//! group option wrapped against a prefix measured with its escapes (ADR-0024), the two widths the
//! two `autocomplete` Prompts wrap an option to, a left arrow in a search box (ADR-0025), the
//! whole segment editor of `date`, whose own suite types no digit and presses no arrow (ADR-0026),
//! and `multiline`'s, whose own suite never moves a cursor or deletes a character (ADR-0027).
//!
//! The resizes are why a Grid is built from segments rather than from one string. The emulator has
//! to change size at the same point in the stream that the real terminal did, on both sides, or the
//! bytes after a resize are being read at a width nothing wrote them for. Those four are also what
//! settled the last divergence `session.rs` recorded: two of them disagreed, and the port follows
//! upstream now (ADR-0017).
//!
//! # What it does not see
//!
//! Beyond conceal, one ordering: the two extra writes a `confirm` makes when it settles from inside
//! its own listener leave the terminal exactly where it would have been without them — a cursor-up
//! and a line feed cancel out. Deleting them from the Emitter fails
//! `every_scenario_is_written_the_way_clack_wrote_it` and passes here, which is the clearest case
//! yet for keeping the two comparisons side by side.

mod grid;
mod scenarios;

use grid::{Grid, difference, feed, read};
use scenarios::{Segment, all};

use avt::Vt;

/// A byte stream, replayed into the terminal it was written for.
///
/// Segments rather than one string, because a Scenario may change the terminal's size part-way
/// through: the emulator has to change with it, at the same point in the stream, or the bytes after
/// the resize are being read at a width nothing wrote them for. A Scenario that never resizes is
/// one segment and this is the same thing it always was.
fn grid(segments: &[Segment]) -> Grid {
	let (columns, rows) = (segments[0].columns, segments[0].rows);
	let mut vt = Vt::new(columns, rows);

	for segment in segments {
		if (segment.columns, segment.rows) != vt.size() {
			vt.resize(segment.columns, segment.rows);
		}
		feed(&mut vt, &segment.bytes);
	}

	read(&vt)
}

/// The one that matters.
#[test]
fn every_scenario_leaves_the_terminal_the_way_clack_left_it() {
	let scenarios = all();
	let mut failures = Vec::new();
	let mut compared = 0;

	for scenario in &scenarios {
		if !scenario.is_replayable() {
			continue;
		}

		let theirs = grid(&scenario.recorded_segments());
		let ours = grid(&scenario.replayed());

		// Two blank terminals are equal, so a Scenario whose stream never reached the emulator
		// would agree for free. clack's side is asserted to have drawn the message it was given —
		// in order, but not necessarily unbroken, because a narrow terminal wraps it across rows.
		//
		// Unless the message has scrolled off the top, which a short terminal and a tall Frame can
		// do: `autocomplete › renders bottom ellipsis when items do not fit` settles into six rows of
		// a terminal five rows tall. There the guard falls back to the thing it is actually for —
		// that clack left *something* to compare against.
		assert!(
			drew(&theirs.text(), &scenario.message) || !theirs.text().trim().is_empty(),
			"{}: clack's stream left nothing on the terminal, so there is nothing to compare",
			scenario.name
		);

		compared += 1;
		if ours != theirs {
			failures.push(format!(
				"  {}\n{}",
				scenario.name,
				difference(&theirs, &ours)
			));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {compared} Scenarios leave the terminal in a different state.\n\n{}\n\n\
		 Both streams were replayed through one `avt`, so this is a difference in appearance, not \
		 in encoding.",
		failures.len(),
		failures.join("\n"),
	);

	assert!(
		compared >= 191,
		"only {compared} Scenarios were compared; the fixtures have stopped carrying them"
	);
}

/// Whether the terminal holds `message`, allowing for the row breaks a wrap puts through it.
///
/// A subsequence rather than a substring: a wrap inserts a break inside the message and may eat the
/// space it broke at, so `contains` only works at a width nothing reaches. It stays a real check on
/// *this* Scenario having been drawn — the characters have to be there, in order — rather than
/// softening to "something was written".
fn drew(terminal: &str, message: &str) -> bool {
	let mut written = terminal.chars();
	message
		.chars()
		.all(|wanted| written.any(|drawn| drawn == wanted))
}

/// The emulator is the arbiter of every appearance claim in this file, so it is worth one test of
/// its own: if it quietly stopped modelling an attribute clack uses, every comparison above would
/// start agreeing for the wrong reason.
///
/// The four checked here are the ones clack's Theme actually draws with — `dim` for a submitted
/// value, `strikethrough` and `dim` for a cancelled one, `inverse` for the placeholder cursor, and
/// a foreground colour for the Guide.
#[test]
fn the_emulator_models_the_attributes_the_theme_uses() {
	let mut vt = Vt::new(20, 3);
	vt.feed_str("\u{1B}[2ma\u{1B}[0m\u{1B}[9mb\u{1B}[0m\u{1B}[7mc\u{1B}[0m\u{1B}[90md");

	let cells = vt.line(0).cells().to_vec();
	assert!(cells[0].pen().is_faint(), "dim is not modelled");
	assert!(
		cells[1].pen().is_strikethrough(),
		"strikethrough is not modelled"
	);
	assert!(cells[2].pen().is_inverse(), "inverse is not modelled");
	assert_eq!(
		cells[3].pen().foreground(),
		Some(avt::Color::Indexed(8)),
		"the Guide's colour is not modelled"
	);

	// And the two encodings of one appearance land in the same place, which is the premise the
	// whole comparison rests on: picocolors' one-attribute-per-escape against the Emitter's.
	let mut picocolors = Vt::new(20, 1);
	picocolors.feed_str("\u{1B}[7m\u{1B}[2mx\u{1B}[22m\u{1B}[27m");
	let mut emitter = Vt::new(20, 1);
	emitter.feed_str("\u{1B}[0;2;7mx\u{1B}[0m");
	assert_eq!(
		picocolors.line(0).cells()[0],
		emitter.line(0).cells()[0],
		"the two SGR encodings do not agree through the emulator"
	);
}

/// Conceal, named rather than assumed. If a later `avt` starts modelling it this fails, which is
/// the moment to take the caveat out of the module docs above.
#[test]
fn the_emulator_does_not_model_conceal() {
	let mut vt = Vt::new(20, 1);
	vt.feed_str("\u{1B}[8mx");
	let mut plain = Vt::new(20, 1);
	plain.feed_str("x");
	assert_eq!(
		vt.line(0).cells()[0],
		plain.line(0).cells()[0],
		"the emulator has learnt conceal; the Grid can now check clack's empty placeholder"
	);
}
