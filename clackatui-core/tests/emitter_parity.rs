//! Conformance: the Emitter against `@clack/core`'s own `Prompt.render`.
//!
//! Same arrangement as `wrap_parity.rs` and `width_parity.rs`, for the reason ADR-0008 gives:
//! `upstream/` is not committed, so `scripts/harvest-emitter.mjs` drives the real `Prompt` class
//! through each case and records every byte it wrote, and this asserts against the recording.
//!
//! What is asserted is byte equality, which is stricter than the Grid parity the project actually
//! claims (ADR-0001) — deliberately so. The Emitter's output is almost entirely cursor arithmetic
//! and erasure, where a Grid comparison would happily accept a repaint that lands the cursor in a
//! different column and only fail later, in some Scenario, for reasons nothing points at. The
//! corpus is colourless so that byte equality is a fair thing to ask: matching clack's *styling*
//! bytes would be asserting picocolors' encoding rather than clack's algorithm, and a Frame carries
//! styling as a `Style` per span instead of as escapes (ADR-0011).
//!
//! Every frame is a list of code points, never a string literal, for the reason `width_parity.rs`
//! gives.

use std::collections::BTreeSet;

use clackatui_core::emitter::Emitter;
use clackatui_core::frame::{Frame, Line, Span};

const FIXTURE: &str = include_str!("fixtures/emitter.json");

struct Fixture {
	version: String,
	cases: Vec<Case>,
}

struct Case {
	name: String,
	columns: u16,
	rows: u16,
	frames: Vec<Vec<u32>>,
	writes: Vec<String>,
}

impl Case {
	/// The Frames as clack had them: a line per `\n`, no styling anywhere.
	fn frames(&self) -> Vec<Frame> {
		self.frames
			.iter()
			.map(|codepoints| {
				let text: String = codepoints
					.iter()
					.map(|cp| {
						char::from_u32(*cp).unwrap_or_else(|| {
							panic!("{}: {cp:#X} is not a scalar value", self.name)
						})
					})
					.collect();
				Frame {
					lines: text
						.split('\n')
						.map(|line| Line::from(Span::raw(line)))
						.collect(),
				}
			})
			.collect()
	}
}

fn fixture() -> Fixture {
	let json: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixtures/emitter.json");
	let cases = json["cases"]
		.as_array()
		.expect("cases")
		.iter()
		.map(|case| Case {
			name: case["name"].as_str().expect("name").to_owned(),
			columns: case["columns"].as_u64().expect("columns") as u16,
			rows: case["rows"].as_u64().expect("rows") as u16,
			frames: case["frames"]
				.as_array()
				.expect("frames")
				.iter()
				.map(|frame| {
					frame
						.as_array()
						.expect("frame")
						.iter()
						.map(|cp| cp.as_u64().expect("code point") as u32)
						.collect()
				})
				.collect(),
			writes: case["writes"]
				.as_array()
				.expect("writes")
				.iter()
				.map(|write| write.as_str().expect("write").to_owned())
				.collect(),
		})
		.collect();

	Fixture {
		version: json["version"].as_str().expect("version").to_owned(),
		cases,
	}
}

/// Escapes made legible, so a failure reads as a sequence rather than as a wall of `\u{1b}`.
fn readable(bytes: &str) -> String {
	bytes
		.replace('\u{1b}', "ESC")
		.replace('\n', "\\n")
		.replace('\r', "\\r")
}

#[test]
fn every_frame_is_written_the_way_clack_writes_it() {
	let fixture = fixture();
	let mut wrong = Vec::new();

	for case in &fixture.cases {
		let mut emitter = Emitter::new();
		for (step, (frame, expected)) in case.frames().iter().zip(&case.writes).enumerate() {
			let ours = emitter.frame(frame, case.columns, case.rows);
			if &ours != expected {
				wrong.push(format!(
					"  {} (step {step}, {} columns, {} rows)\n    clack: {}\n    ours:  {}",
					case.name,
					case.columns,
					case.rows,
					readable(expected),
					readable(&ours),
				));
			}
		}
	}

	assert!(
		wrong.is_empty(),
		"{} of {} cases are written differently from @clack/core {}:\n{}",
		wrong.len(),
		fixture.cases.len(),
		fixture.version,
		wrong.join("\n"),
	);
}

/// The Emitter is a state machine, and a fixture that only ever showed it one Frame would say
/// nothing about the part of it that matters.
#[test]
fn the_fixture_exercises_every_branch_of_the_algorithm() {
	let fixture = fixture();

	assert!(
		fixture.cases.len() >= 35,
		"only {} cases recorded; the corpus has been truncated",
		fixture.cases.len()
	);

	let names: BTreeSet<&str> = fixture
		.cases
		.iter()
		.map(|case| case.name.as_str())
		.collect();
	assert_eq!(names.len(), fixture.cases.len(), "duplicate case names");

	for required in [
		"an empty first frame is not a frame",
		"the same frame twice",
		"the middle line changes",
		"two lines change",
		"the frame shrinks by a line",
		"a line wraps",
		"the change is above the terminal",
		"a tall frame shrinks below the terminal",
		"a text prompt is typed into",
	] {
		assert!(
			names.contains(required),
			"the fixture is missing `{required}`"
		);
	}

	// Each of `render`'s reachable exits, identified by what it leaves on the wire.
	let steps: Vec<&str> = fixture
		.cases
		.iter()
		.flat_map(|case| case.writes.iter().map(String::as_str))
		.collect();

	let count =
		|predicate: &dyn Fn(&str) -> bool| steps.iter().filter(|step| predicate(step)).count();

	assert!(
		count(&|step| step.is_empty()) >= 4,
		"no Frame was left unwritten"
	);
	assert!(
		count(&|step| step.starts_with("\u{1b}[?25l")) >= 10,
		"too few opening Frames"
	);
	assert!(
		count(&|step| step.contains("\u{1b}[2K")) >= 10,
		"too few single-row repaints"
	);
	assert!(
		count(&|step| step.contains("\u{1b}[J")) >= 8,
		"too few erase-below repaints"
	);
	assert!(
		steps.contains(&"\u{1b}[999D\u{1b}[4A"),
		"no case walks the cursor back and then writes nothing"
	);
	assert!(
		steps.iter().any(|step| step.contains("undefined")),
		"no case reaches the missing-row defect"
	);
}

/// The one place the port reproduces a defect rather than a decision.
///
/// A Frame that loses exactly its last row leaves upstream indexing past the end of its own line
/// array and writing the string `undefined` into the terminal. It is recorded here by name so that
/// the day it is fixed upstream, `mise run drift` reports it as a Fixture that moved rather than as
/// a mystery.
#[test]
fn a_frame_that_loses_its_last_row_still_says_undefined() {
	let fixture = fixture();
	let case = fixture
		.cases
		.iter()
		.find(|case| case.name == "the frame shrinks by a line")
		.expect("the case is in the corpus");

	assert_eq!(
		case.writes[1], "\u{1b}[999D\u{1b}[2A\u{1b}[2B\u{1b}[2K\u{1b}[Gundefined\u{1b}[1A",
		"@clack/core {} no longer writes `undefined` here; see ADR-0013",
		fixture.version,
	);
}
