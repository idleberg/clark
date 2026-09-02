//! Parity for the renderers that are driven by calls: `spinner` and `progress`.
//!
//! The fourth Recorder's corpus, and the first whose claim is about a *sequence* of writes rather
//! than one. A spinner erases the row it wrote before writing the next, so a port that draws the right
//! row and walks the cursor back over the wrong number of them leaves the terminal wrong while
//! agreeing about every character it put on the wire. Both are checked:
//!
//! - **The Grid**, over the whole run's bytes at once — the one that can see a mis-counted
//!   `cursor.up`, because that is where the debris of the previous row shows up.
//! - **The characters of each step**, with SGR stripped and the cursor escapes kept. A step is one
//!   `start`, one turn of the interval, or one `stop`, so a disagreement names the tick it began at
//!   instead of printing the whole run.
//!
//! # The clock is an argument
//!
//! `clackatui_core` reads no clock, so a tick takes the time since `start` as a parameter and the
//! Fixture carries what the Recorder's fake clock said at every step. A case with
//! `indicator: 'timer'` is therefore recorded at a `delay` that puts whole seconds on the row.

mod grid;
use grid::{Grid, difference};

use std::time::Duration;

use clackatui_core::frame::{Line, Span};
use clackatui_core::progress::{BarStyle, Progress};
use clackatui_core::prompt::Status;
use clackatui_core::spinner::{Indicator, Options, Spinner, StyleFrame};
use clackatui_core::theme::Theme;
use ratatui_core::style::{Color, Style};
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/scripted.json");

struct Step {
	op: String,
	/// `None` where the case left it out — which is `advance`'s live fallback to the message before
	/// it, and nothing at all anywhere else.
	message: Option<String>,
	/// `advance`'s, and `1` where a case did not say.
	step: usize,
	elapsed: Duration,
	bytes: String,
}

impl Step {
	fn message(&self) -> &str {
		self.message.as_deref().unwrap_or_default()
	}
}

struct Case {
	name: String,
	kind: String,
	columns: usize,
	rows: usize,
	options: Value,
	steps: Vec<Step>,
	bytes: String,
}

fn cases() -> (Value, Vec<Case>) {
	let json: Value = serde_json::from_str(FIXTURE).expect("fixtures/scripted.json parses");
	let cases = json["cases"]
		.as_array()
		.expect("the fixture carries cases")
		.iter()
		.map(|case| Case {
			name: case["name"].as_str().expect("a name").to_owned(),
			kind: case["kind"].as_str().expect("a kind").to_owned(),
			columns: case["columns"].as_u64().expect("a width") as usize,
			rows: case["rows"].as_u64().expect("a height") as usize,
			options: case["options"].clone(),
			steps: case["steps"]
				.as_array()
				.expect("a script")
				.iter()
				.map(|step| Step {
					op: step["op"].as_str().expect("an op").to_owned(),
					message: step["message"].as_str().map(str::to_owned),
					step: step["step"].as_u64().unwrap_or(1) as usize,
					elapsed: Duration::from_millis(
						step["elapsed"].as_u64().expect("the clock at this step"),
					),
					bytes: step["bytes"].as_str().expect("the bytes").to_owned(),
				})
				.collect(),
			bytes: case["bytes"].as_str().expect("the bytes").to_owned(),
		})
		.collect();
	(json, cases)
}

/// A case's options, with upstream's defaults for whatever it did not set.
fn options(case: &Case) -> Options<'static> {
	let options = &case.options;
	let defaults = Options::default();
	Options {
		indicator: match options["indicator"].as_str() {
			Some("timer") => Indicator::Timer,
			_ => Indicator::Dots,
		},
		frames: options["frames"].as_array().map(|frames| {
			frames
				.iter()
				.map(|frame| frame.as_str().expect("a frame is a string").to_owned())
				.collect()
		}),
		with_guide: options["withGuide"].as_bool().unwrap_or(true),
		ci: options["ci"].as_bool().unwrap_or(false),
		style_frame: options["styleFrame"]
			.as_str()
			.map_or(defaults.style_frame, formatter),
	}
}

/// The `styleFrame` a case names, as a Line-returning one rather than upstream's string one.
///
/// `red` adds no columns and `stars` adds two, which is the difference that matters: the row is
/// wrapped after the formatter has had it.
fn formatter(named: &str) -> StyleFrame<'static> {
	match named {
		"red" => &|frame| Line::from(Span::styled(frame, Style::new().fg(Color::Red))),
		"stars" => &|frame| Line::from(Span::raw(format!("*{frame}*"))),
		other => panic!("the fixture names a formatter the test does not have: {other}"),
	}
}

/// A case's progress options, with upstream's defaults for whatever it did not set.
fn progress_options(case: &Case) -> clackatui_core::progress::Options<'static> {
	let json = &case.options;
	let defaults = clackatui_core::progress::Options::default();
	clackatui_core::progress::Options {
		style: match json["style"].as_str() {
			Some("light") => BarStyle::Light,
			Some("block") => BarStyle::Block,
			_ => BarStyle::Heavy,
		},
		max: json["max"]
			.as_u64()
			.map_or(defaults.max, |max| max as usize),
		size: json["size"]
			.as_u64()
			.map_or(defaults.size, |size| size as usize),
		spinner: options(case),
	}
}

/// Replays a case's script, returning what the port wrote at each step.
fn replay(case: &Case) -> Vec<String> {
	match case.kind.as_str() {
		"spinner" => {
			let mut spinner = Spinner::new(Theme::clack(), case.columns, options(case));
			case.steps
				.iter()
				.map(|step| match step.op.as_str() {
					"start" => spinner.start(step.message()),
					"tick" => spinner.tick(step.elapsed),
					"message" => {
						spinner.set_message(step.message());
						String::new()
					}
					"stop" => spinner.stop(step.message(), Status::Submit, step.elapsed),
					"cancel" => spinner.stop(step.message(), Status::Cancel, step.elapsed),
					"error" => spinner.stop(step.message(), Status::Error, step.elapsed),
					"clear" => spinner.clear(),
					other => panic!("{}: no such step: {other}", case.name),
				})
				.collect()
		}
		"progress" => {
			let mut progress = Progress::new(Theme::clack(), case.columns, progress_options(case));
			case.steps
				.iter()
				.map(|step| match step.op.as_str() {
					"start" => progress.start(step.message()),
					"tick" => progress.tick(step.elapsed),
					"advance" => {
						progress.advance(step.step, step.message.as_deref());
						String::new()
					}
					"message" => {
						progress.set_message(step.message());
						String::new()
					}
					"stop" => progress.stop(step.message(), Status::Submit, step.elapsed),
					"cancel" => progress.stop(step.message(), Status::Cancel, step.elapsed),
					"error" => progress.stop(step.message(), Status::Error, step.elapsed),
					"clear" => progress.clear(),
					other => panic!("{}: no such step: {other}", case.name),
				})
				.collect()
		}
		other => panic!("{}: no renderer for kind {other}", case.name),
	}
}

/// The one that matters.
#[test]
fn every_scripted_run_leaves_the_terminal_the_way_clack_left_it() {
	let (_, cases) = cases();
	let mut failures = Vec::new();

	for case in &cases {
		let ours: String = replay(case).concat();
		let theirs = Grid::of(&case.bytes, case.columns, case.rows);
		let ours = Grid::of(&ours, case.columns, case.rows);

		if ours != theirs {
			failures.push(format!("  {}\n{}", case.name, difference(&theirs, &ours)));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {} scripted runs leave the terminal in a different state.\n\n{}",
		failures.len(),
		cases.len(),
		failures.join("\n"),
	);

	assert!(
		cases.len() >= 66,
		"only {} cases were compared; the fixture has stopped carrying them",
		cases.len()
	);
}

/// Step by step, and with the cursor escapes left in.
///
/// The Grid above cannot tell a `cursor.up` that was not written from one that was written and had
/// nothing to undo, and it cannot see trailing space at all. This can see both, and it says which
/// step went wrong.
#[test]
fn every_step_of_every_run_is_written_the_way_clack_wrote_it() {
	let (_, cases) = cases();
	let mut failures = Vec::new();

	for case in &cases {
		for (index, (step, ours)) in case.steps.iter().zip(replay(case)).enumerate() {
			let theirs = strip(&step.bytes);
			let ours = strip(&ours);
			if theirs != ours {
				failures.push(format!(
					"  {} — step {index} ({})\n       clack: {theirs:?}\n        port: {ours:?}",
					case.name, step.op
				));
			}
		}
	}

	assert!(
		failures.is_empty(),
		"{} steps put different characters on the wire.\n\n{}",
		failures.len(),
		failures.join("\n"),
	);
}

/// The fixture is a recording of one clack, and says which.
#[test]
fn the_scripted_fixture_is_a_plausible_recording() {
	let (json, cases) = cases();

	assert_eq!(json["tag"], "@clack/prompts@1.7.0");
	assert_eq!(json["generatedBy"], "scripts/harvest-scripted.mjs");

	for kind in ["spinner", "progress"] {
		assert!(
			cases.iter().any(|case| case.kind == kind),
			"nothing in the fixture records a {kind}"
		);
	}

	// Every step either renderer has, and every option that changes what one draws.
	for op in [
		"start", "tick", "message", "advance", "stop", "cancel", "error", "clear",
	] {
		assert!(
			cases
				.iter()
				.any(|case| case.steps.iter().any(|step| step.op == op)),
			"nothing in the fixture records a {op}"
		);
	}
	for option in [
		"indicator",
		"frames",
		"withGuide",
		"ci",
		"styleFrame",
		"style",
		"max",
		"size",
	] {
		assert!(
			cases.iter().any(|case| !case.options[option].is_null()),
			"nothing in the fixture sets {option}"
		);
	}
	// The three bar characters are a `unicodeOr` each, and a corpus with one of them in it would
	// leave the other two to the Theme alone.
	for style in ["light", "block"] {
		assert!(
			cases.iter().any(|case| case.options["style"] == style),
			"nothing in the fixture draws a {style} bar"
		);
	}

	// A timer that never reaches a minute would leave half of `formatTimer` unrecorded.
	assert!(
		cases
			.iter()
			.any(|case| case.steps.iter().any(|step| step.bytes.contains("m "))),
		"nothing in the fixture runs long enough to print a minute"
	);
	// Three dots is the cap, and a run that never reaches it does not record the cap.
	assert!(
		cases
			.iter()
			.any(|case| case.steps.iter().any(|step| step.bytes.contains("..."))),
		"nothing in the fixture reaches three dots"
	);
	// The defect the module docs are about: a row drawn over two rows and walked back over as one.
	assert!(
		cases.iter().any(|case| {
			case.steps.iter().any(|step| {
				step.op == "tick" && step.bytes.contains('\n') && !step.bytes.contains('A')
			})
		}),
		"nothing in the fixture wraps a row without walking back over it"
	);
	assert!(
		cases.iter().any(|case| case.columns != 80),
		"every run in the fixture was written to the same terminal"
	);
}

/// Bytes with their SGR sequences taken out — and nothing else: the cursor movement is the subject.
fn strip(bytes: &str) -> String {
	let mut out = String::new();
	let mut chars = bytes.chars();
	while let Some(c) = chars.next() {
		if c == '\u{1b}' {
			let mut sequence = String::from("\u{1b}");
			for c in chars.by_ref() {
				sequence.push(c);
				if c.is_ascii_alphabetic() {
					break;
				}
			}
			if !sequence.ends_with('m') {
				out.push_str(&sequence);
			}
		} else {
			out.push(c);
		}
	}
	out
}
