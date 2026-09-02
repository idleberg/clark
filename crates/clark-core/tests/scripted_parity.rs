//! Parity for the renderers that are driven by calls: `spinner`, `progress` and `task_log`.
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
//! A task log has no clock and erases by a count of its own, which is the same claim in a different
//! shape: the rows it thinks it wrote are the rows it walks over, and only the Grid can say whether
//! that was the number the terminal used.
//!
//! # The clock is an argument
//!
//! `clark_core` reads no clock, so a tick takes the time since `start` as a parameter and the
//! Fixture carries what the Recorder's fake clock said at every step. A case with
//! `indicator: 'timer'` is therefore recorded at a `delay` that puts whole seconds on the row.

mod grid;
use grid::{Grid, difference};

use std::time::Duration;

use clark_core::frame::{Line, Span};
use clark_core::progress::{BarStyle, Progress};
use clark_core::prompt::Status;
use clark_core::spinner::{Indicator, Options, Spinner, StyleFrame};
use clark_core::task_log::{self, Outcome, TaskLog};
use clark_core::theme::Theme;
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
	/// A task log's `{ raw: true }`.
	raw: bool,
	/// The name a `group` was given.
	name: String,
	/// Which group a step is about, counted in the order the script made them.
	group: usize,
	/// A task log ending's `showLog`. `None` is the default, which is not the same one for both
	/// endings — see [`clark_core::task_log`].
	show_log: Option<bool>,
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
					raw: step["raw"].as_bool().unwrap_or(false),
					name: step["name"].as_str().unwrap_or_default().to_owned(),
					group: step["group"].as_u64().unwrap_or(0) as usize,
					show_log: step["showLog"].as_bool(),
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
fn progress_options(case: &Case) -> clark_core::progress::Options<'static> {
	let json = &case.options;
	let defaults = clark_core::progress::Options::default();
	clark_core::progress::Options {
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

/// A case's task-log options, with upstream's defaults for whatever it did not set.
fn task_log_options(case: &Case) -> task_log::Options {
	let json = &case.options;
	let defaults = task_log::Options::default();
	// `isTTY` is `!isCI() && isTTY(output)`, and the Recorder gives a case a terminal unless it is
	// pretending to be CI.
	let ci = json["ci"].as_bool().unwrap_or(false);
	let tty = json["tty"].as_bool().unwrap_or(!ci);
	task_log::Options {
		title: json["title"].as_str().unwrap_or_default().to_owned(),
		limit: json["limit"].as_u64().map(|limit| limit as usize),
		spacing: json["spacing"]
			.as_u64()
			.map_or(defaults.spacing, |spacing| spacing as usize),
		retain_log: json["retainLog"].as_bool().unwrap_or(false),
		// `guide`, not `withGuide`: the option a task log takes is never passed on, so what its
		// messages read is the global. A case sets the one it means.
		with_guide: json["guide"].as_bool().unwrap_or(true),
		is_tty: !ci && tty,
	}
}

/// Replays a case's script, returning what the port wrote at each step.
fn replay(case: &Case) -> Vec<String> {
	match case.kind.as_str() {
		"task-log" => {
			let mut log = None;
			let mut groups = Vec::new();
			let mut written = Vec::new();
			for step in &case.steps {
				// Every script opens before it says anything, so the `expect` below is the Recorder's
				// contract and not a guess.
				written.push(match step.op.as_str() {
					"open" => {
						let (opened, bytes) =
							TaskLog::new(Theme::clack(), case.columns, task_log_options(case));
						log = Some(opened);
						bytes
					}
					"group" => {
						let log = log.as_mut().expect("a script opens first");
						groups.push(log.group(&step.name));
						String::new()
					}
					op => {
						let log = log.as_mut().expect("a script opens first");
						match op {
							"message" => log.message(step.message(), step.raw),
							"group-message" => {
								log.group_message(groups[step.group], step.message(), step.raw)
							}
							"group-success" => log.complete_group(
								groups[step.group],
								Outcome::Success,
								step.message(),
							),
							"group-error" => log.complete_group(
								groups[step.group],
								Outcome::Error,
								step.message(),
							),
							"success" => {
								log.success(step.message(), step.show_log.unwrap_or(false))
							}
							"error" => log.error(step.message(), step.show_log.unwrap_or(true)),
							other => panic!("{}: no such step: {other}", case.name),
						}
					}
				});
			}
			written
		}
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
		cases.len() >= 114,
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

	for kind in ["spinner", "progress", "task-log"] {
		assert!(
			cases.iter().any(|case| case.kind == kind),
			"nothing in the fixture records a {kind}"
		);
	}

	// Every step any of the three renderers has, and every option that changes what one draws.
	for op in [
		"start",
		"tick",
		"message",
		"advance",
		"stop",
		"cancel",
		"error",
		"clear",
		"open",
		"group",
		"group-message",
		"group-success",
		"group-error",
		"success",
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
		"limit",
		"spacing",
		"retainLog",
		"guide",
		"tty",
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

	// A task log's two live flags: `{ raw: true }`, and a `showLog` said either way — the two endings
	// have opposite defaults, so a corpus that never said it out loud would only record one of them.
	assert!(
		cases
			.iter()
			.any(|case| case.steps.iter().any(|step| step.raw)),
		"nothing in the fixture records a raw message"
	);
	for said in [true, false] {
		assert!(
			cases
				.iter()
				.any(|case| { case.steps.iter().any(|step| step.show_log == Some(said)) }),
			"nothing in the fixture asks an ending for showLog: {said}"
		);
	}
	// The defect the task log's module docs are about: in CI nothing is printed and the erase is
	// written anyway, so a step whose bytes are an erase and nothing else is the recording of it.
	assert!(
		cases.iter().any(|case| {
			case.kind == "task-log"
				&& case
					.steps
					.iter()
					.any(|step| step.op == "message" && step.bytes.ends_with("\u{1b}[G"))
		}),
		"nothing in the fixture erases rows it never printed"
	);

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
