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
//! # What is still missing
//!
//! The Scenarios themselves. Thirteen are harvested and ten can be replayed, but every one of them
//! runs at 80 columns and none resizes, so the wrap and the re-layout paths — the two places
//! `session.rs` records a known divergence — reach this comparison untested. The hand-authored
//! Scenarios README promises are what close that, and this file is what they will be run through.

mod scenarios;
use scenarios::fixture;

use avt::{Cell, Vt};

/// The observable terminal state: characters, styles and cursor position.
///
/// Deliberately the whole terminal rather than the rows the Prompt happens to occupy. Where a
/// Prompt *stops* writing is part of what is being checked — a port that erased one row too few
/// would leave a difference below its own output, and cropping to its own output would hide it.
#[derive(PartialEq)]
struct Grid {
	rows: Vec<Vec<Cell>>,
	cursor: (usize, usize),
	cursor_visible: bool,
}

impl Grid {
	/// Everything on the terminal, as text. For the emptiness guard and for failure messages, not
	/// for the comparison — a Grid is more than its characters.
	fn text(&self) -> String {
		self.rows
			.iter()
			.map(|cells| cells.iter().map(Cell::char).collect::<String>())
			.collect::<Vec<_>>()
			.join("\n")
	}
}

/// A byte stream, replayed into the terminal it was written for.
fn grid(stream: &str, columns: usize, rows: usize) -> Grid {
	let mut vt = Vt::new(columns, rows);
	vt.feed_str(stream);

	let cursor = vt.cursor();
	Grid {
		rows: vt.view().map(|line| line.cells().to_vec()).collect(),
		cursor: (cursor.col, cursor.row),
		cursor_visible: cursor.visible,
	}
}

/// The one that matters.
#[test]
fn every_scenario_leaves_the_terminal_the_way_clack_left_it() {
	let (_, scenarios) = fixture();
	let mut failures = Vec::new();
	let mut compared = 0;

	for scenario in &scenarios {
		if !scenario.is_replayable() {
			continue;
		}

		let theirs = grid(&scenario.recorded(), scenario.columns, scenario.rows);
		let ours = grid(&scenario.replay(), scenario.columns, scenario.rows);

		// Two blank terminals are equal, so a Scenario whose stream never reached the emulator
		// would agree for free. clack's side is asserted to have drawn the message it was given.
		assert!(
			theirs.text().contains(&scenario.message),
			"{}: clack's stream left no message on the terminal, so there is nothing to compare",
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
		compared >= 10,
		"only {compared} Scenarios were compared; the fixture has stopped carrying them"
	);
}

/// Two Grids, with the parts that agree left out.
///
/// A failure here is read by someone who has to work out which write caused it, so it reports the
/// rows that differ and how, rather than two screens to be diffed by eye.
fn difference(theirs: &Grid, ours: &Grid) -> String {
	let mut out = Vec::new();

	if theirs.cursor != ours.cursor {
		out.push(format!(
			"     cursor: clack left it at {:?}, the port at {:?}",
			theirs.cursor, ours.cursor
		));
	}
	if theirs.cursor_visible != ours.cursor_visible {
		out.push(format!(
			"     cursor: clack left it {}, the port {}",
			shown(theirs.cursor_visible),
			shown(ours.cursor_visible),
		));
	}

	for (index, (theirs, ours)) in theirs.rows.iter().zip(&ours.rows).enumerate() {
		if theirs == ours {
			continue;
		}
		out.push(format!("     row {index}"));
		out.push(format!("       clack: {}", row(theirs)));
		out.push(format!("        port: {}", row(ours)));

		// Where the two rows say the same thing but wear it differently, the text above is
		// identical and useless. Name the first cell that differs instead.
		if row(theirs) == row(ours) {
			if let Some((column, theirs, ours)) = theirs
				.iter()
				.zip(ours)
				.enumerate()
				.find(|(_, (a, b))| a != b)
				.map(|(column, (a, b))| (column, a, b))
			{
				out.push(format!(
					"       column {column} is styled {:?} by clack and {:?} by the port",
					theirs.pen(),
					ours.pen()
				));
			}
		}
	}

	out.join("\n")
}

/// A row as text, with the trailing blanks cut off so that a failure fits on a line.
fn row(cells: &[Cell]) -> String {
	let text: String = cells.iter().map(Cell::char).collect();
	format!("{:?}", text.trim_end())
}

fn shown(visible: bool) -> &'static str {
	if visible { "visible" } else { "hidden" }
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
