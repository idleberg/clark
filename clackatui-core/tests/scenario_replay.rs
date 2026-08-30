//! The `text` Scenarios, harvested from clack's own test suite by `scripts/harvest-scenarios.mjs`.
//!
//! ADR-0003 makes upstream's tests the specification: each of them is a Prompt configuration, a
//! sequence of keypresses, and the output clack wrote back. ADR-0010 describes how they are caught.
//! The recording is what this file reads; no JavaScript runs here, for the reasons ADR-0008 gives.
//!
//! What is asserted is not yet the Grid: no emulator runs here, so every recorded chunk but the
//! first is a diff this file cannot make sense of, and cursor position is not checked at all. That
//! is what M1 finishes with. Three things are checked in the meantime, and each is worth having on
//! its own:
//!
//!   - the fixture is a plausible recording, so that a truncated harvest cannot pass for free;
//!   - every Scenario replayed through [`Prompt<TextState>`] settles in the state clack settled in;
//!   - every Scenario's *opening* Frame is drawn the way clack drew it, styles included — the one
//!     Frame upstream writes whole rather than as a diff, because it has nothing to diff against.
//!
//! The second was the first outside check the Prompt state machine had; ADR-0009 notes it was
//! ported by close reading alone, against no oracle. The third is the first check on appearance,
//! and the first thing in the project to compare bytes clack actually wrote with something the port
//! actually drew.

use std::collections::BTreeSet;

use clackatui_core::frame::Frame;
use clackatui_core::line_editor::{Key, KeyName};
use clackatui_core::prompt::{Prompt, Status};
use clackatui_core::text::{TextState, TextWidget};
use ratatui_core::style::{Color, Modifier, Style};

const FIXTURE: &str = include_str!("fixtures/scenarios/text.json");

/// The tag `README.md` names. A fixture from anywhere else is not the thing we claim parity with.
const TAG: &str = "@clack/prompts@1.7.0";

struct Scenario {
	name: String,
	kind: String,
	message: String,
	placeholder: Option<String>,
	default_value: Option<String>,
	initial_value: Option<String>,
	/// `opts.withGuide`, which falls back to `settings.withGuide` when absent.
	with_guide: bool,
	/// Upstream passed a `validate` callback, which a recording cannot carry across.
	validates: bool,
	keys: Vec<Recorded>,
	output: Vec<String>,
}

struct Recorded {
	s: Option<String>,
	name: Option<String>,
	ctrl: bool,
	meta: bool,
	shift: bool,
	sequence: Option<String>,
}

fn fixture() -> (serde_json::Value, Vec<Scenario>) {
	let json: serde_json::Value =
		serde_json::from_str(FIXTURE).expect("fixtures/scenarios/text.json parses");

	let scenarios = json["scenarios"]
		.as_array()
		.expect("scenarios is an array")
		.iter()
		.filter_map(|scenario| {
			let name = scenario["name"]
				.as_str()
				.expect("name is a string")
				.to_owned();
			// One prompt per Scenario for `text`. A flow that opens several — `group()` — is a
			// different shape and is not harvested yet.
			let runs = scenario["prompts"].as_array().expect("prompts is an array");
			if runs.len() != 1 {
				return None;
			}
			let run = &runs[0];
			let opts = &run["opts"];

			Some(Scenario {
				name,
				kind: run["kind"].as_str().unwrap_or("").to_owned(),
				message: opts["message"].as_str().unwrap_or("").to_owned(),
				placeholder: opts["placeholder"].as_str().map(str::to_owned),
				default_value: opts["defaultValue"].as_str().map(str::to_owned),
				initial_value: opts["initialValue"].as_str().map(str::to_owned),
				with_guide: opts["withGuide"]
					.as_bool()
					.or_else(|| run["settings"]["withGuide"].as_bool())
					.unwrap_or(true),
				validates: opts["validate"]["callback"].as_bool() == Some(true),
				keys: run["keys"]
					.as_array()
					.expect("keys is an array")
					.iter()
					.map(|key| Recorded {
						s: key["s"].as_str().map(str::to_owned),
						name: key["key"]["name"].as_str().map(str::to_owned),
						ctrl: key["key"]["ctrl"].as_bool() == Some(true),
						meta: key["key"]["meta"].as_bool() == Some(true),
						shift: key["key"]["shift"].as_bool() == Some(true),
						sequence: key["key"]["sequence"].as_str().map(str::to_owned),
					})
					.collect(),
				output: run["output"]
					.as_array()
					.expect("output is an array")
					.iter()
					.map(|chunk| chunk.as_str().expect("chunk is a string").to_owned())
					.collect(),
			})
		})
		.collect();

	(json, scenarios)
}

impl Recorded {
	/// A recorded keypress as the Line editor wants it.
	///
	/// The names come from readline, which is what the Scenarios record, so this is
	/// [`KeyName::readline_name`] read backwards. An unrecognised name is left as `None` rather than
	/// guessed at: the Prompt then sees a keypress with a character and no name, which is what
	/// readline does for a key it cannot name.
	fn key(&self) -> Key {
		Key {
			name: self.name.as_deref().and_then(name),
			ctrl: self.ctrl,
			meta: self.meta,
			shift: self.shift,
			sequence: self.sequence.clone(),
		}
	}
}

fn name(readline: &str) -> Option<KeyName> {
	Some(match readline {
		"space" => KeyName::Char(' '),
		"backspace" => KeyName::Backspace,
		"delete" => KeyName::Delete,
		"left" => KeyName::Left,
		"right" => KeyName::Right,
		"home" => KeyName::Home,
		"end" => KeyName::End,
		"up" => KeyName::Up,
		"down" => KeyName::Down,
		"tab" => KeyName::Tab,
		"return" => KeyName::Return,
		"enter" => KeyName::Enter,
		"escape" => KeyName::Escape,
		other => {
			let mut chars = other.chars();
			let c = chars.next()?;
			if chars.next().is_some() {
				return None;
			}
			KeyName::Char(c)
		}
	})
}

/// The state clack was in when it drew its last frame, read off the step symbol it prints.
///
/// `symbol(state)` in clack's `common.ts` is one of four characters and is the first thing on the
/// title line of every frame, so the last one written is the state the Prompt settled in. This is a
/// long way from comparing Grids, and is not meant to stand in for it — it is the one fact about
/// upstream's answer that can be read out of a recording without an emulator.
fn settled(output: &[String]) -> Option<Status> {
	output.iter().rev().find_map(|chunk| {
		chunk.chars().rev().find_map(|c| match c {
			'◆' => Some(Status::Active),
			'◇' => Some(Status::Submit),
			'■' => Some(Status::Cancel),
			'▲' => Some(Status::Error),
			_ => None,
		})
	})
}

/// The port replayed against upstream's own cases. Every Scenario at once, so one run reports every
/// disagreement rather than the first.
#[test]
fn every_scenario_settles_the_way_clack_settled() {
	let (_, scenarios) = fixture();
	let mut failures = Vec::new();
	let mut replayed = 0;

	for scenario in &scenarios {
		// A `validate` callback cannot cross into a recording, so a Scenario that has one would be
		// replayed without the validation that shaped its frames. Those wait for the parity harness,
		// which will supply the predicate by hand.
		if scenario.validates || scenario.keys.is_empty() {
			continue;
		}

		let Some(expected) = settled(&scenario.output) else {
			failures.push(format!(
				"  {:<52} no step symbol in the output",
				scenario.name
			));
			continue;
		};

		let mut state = TextState::new();
		if let Some(default) = &scenario.default_value {
			state = state.with_default_value(default);
		}
		let mut prompt = Prompt::new(state);
		if let Some(initial) = &scenario.initial_value {
			prompt = prompt.with_initial_user_input(initial);
		}

		for recorded in &scenario.keys {
			prompt.key(recorded.s.as_deref(), &recorded.key());
		}

		replayed += 1;
		if prompt.status() != expected {
			failures.push(format!(
				"  {:<52} clack settled {:?}, the port settled {:?}",
				scenario.name,
				expected,
				prompt.status()
			));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {replayed} replayed Scenarios disagree with clack.\n\n{}\n\n\
		 Re-harvest with `node scripts/harvest-scenarios.mjs text` to see whether upstream moved.",
		failures.len(),
		failures.join("\n"),
	);

	assert!(
		replayed >= 9,
		"only {replayed} Scenarios were replayable; the filters above have eaten the suite"
	);
}

// --- The first Frame ---------------------------------------------------------------------------

/// The first thing clack writes for a Prompt is its whole opening Frame, uncut: `render` has no
/// previous frame to diff against, so it prints the lot. Every later write is a diff, and making
/// sense of one needs a terminal emulator — but this one is a Frame on its own, and the widget can
/// be held against it directly, colours and all.
///
/// This is the first parity claim in the project that is about *appearance* rather than about a
/// primitive. It is not yet the Grid comparison ADR-0001 asks for: no emulator runs, so cursor
/// position is not checked and neither is any Frame after the first.
#[test]
fn every_scenario_draws_clacks_opening_frame() {
	let (_, scenarios) = fixture();
	let mut failures = Vec::new();
	let mut compared = 0;

	for scenario in &scenarios {
		let Some(recorded) = opening_frame(&scenario.output) else {
			failures.push(format!("  {}: nothing was drawn", scenario.name));
			continue;
		};

		let mut state = TextState::new();
		if let Some(default) = &scenario.default_value {
			state = state.with_default_value(default);
		}
		let mut prompt = Prompt::new(state);
		if let Some(initial) = &scenario.initial_value {
			prompt = prompt.with_initial_user_input(initial);
		}

		let mut widget =
			TextWidget::new(&prompt, &scenario.message).with_guide(scenario.with_guide);
		if let Some(placeholder) = &scenario.placeholder {
			widget = widget.with_placeholder(placeholder);
		}

		compared += 1;
		let ours = flatten(&widget.frame());
		let theirs = parse(recorded);
		if ours != theirs {
			failures.push(format!(
				"  {}\n     clack: {theirs:?}\n      port: {ours:?}",
				scenario.name
			));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {compared} opening Frames differ from clack's.\n\n{}\n",
		failures.len(),
		failures.join("\n"),
	);

	assert!(
		compared >= 13,
		"only {compared} Frames were compared; the fixture has stopped carrying them"
	);
}

/// The first chunk that is a Frame rather than a cursor instruction, found by the step symbol every
/// Frame opens its title line with.
fn opening_frame(output: &[String]) -> Option<&str> {
	output
		.iter()
		.find(|chunk| chunk.contains(['◆', '◇', '■', '▲']))
		.map(String::as_str)
}

/// A Frame as `(text, style)` runs, one list per line, with adjacent runs of one style joined.
///
/// Both sides are put through this before they are compared, so that a difference in how the text
/// happens to be cut into spans is not reported as a difference in what is drawn.
fn flatten(frame: &Frame) -> Vec<Vec<(String, Style)>> {
	frame
		.lines
		.iter()
		.map(|line| {
			let mut runs: Vec<(String, Style)> = Vec::new();
			for span in &line.spans {
				if span.text.is_empty() {
					continue;
				}
				match runs.last_mut() {
					Some((text, style)) if *style == span.style => text.push_str(&span.text),
					_ => runs.push((span.text.clone(), span.style)),
				}
			}
			runs
		})
		.collect()
}

/// clack's Frame string, read back into the same shape.
///
/// Only SGR is interpreted, because only SGR appears in a Frame — the cursor movement lives in the
/// chunks around it. An unrecognised parameter is a failure rather than something to skip: it would
/// mean clack styles a Frame in a way the Theme has no name for.
fn parse(frame: &str) -> Vec<Vec<(String, Style)>> {
	let mut lines = vec![Vec::<(String, Style)>::new()];
	let mut style = Style::new();
	let mut chars = frame.chars().peekable();

	while let Some(c) = chars.next() {
		match c {
			'\n' => lines.push(Vec::new()),
			'\u{1B}' => {
				assert_eq!(chars.next(), Some('['), "not a CSI sequence");
				let mut params = String::new();
				let final_byte = loop {
					let c = chars.next().expect("unterminated CSI sequence");
					if c.is_ascii_digit() || c == ';' {
						params.push(c);
					} else {
						break c;
					}
				};
				assert_eq!(
					final_byte, 'm',
					"a Frame carries nothing but SGR: {params}{final_byte}"
				);
				for param in params.split(';') {
					style = sgr(style, param);
				}
			}
			_ => {
				let line = lines.last_mut().expect("there is always a line");
				match line.last_mut() {
					Some((text, run)) if *run == style => text.push(c),
					_ => line.push((String::from(c), style)),
				}
			}
		}
	}

	lines
}

/// One SGR parameter, applied. The inverse of the table in `theme.rs`.
fn sgr(style: Style, param: &str) -> Style {
	match param {
		"0" | "" => Style::new(),
		"2" => style.add_modifier(Modifier::DIM),
		"7" => style.add_modifier(Modifier::REVERSED),
		"8" => style.add_modifier(Modifier::HIDDEN),
		"9" => style.add_modifier(Modifier::CROSSED_OUT),
		"22" => off(style, Modifier::DIM),
		"27" => off(style, Modifier::REVERSED),
		"28" => off(style, Modifier::HIDDEN),
		"29" => off(style, Modifier::CROSSED_OUT),
		"31" => style.fg(Color::Red),
		"32" => style.fg(Color::Green),
		"33" => style.fg(Color::Yellow),
		"36" => style.fg(Color::Cyan),
		"90" => style.fg(Color::DarkGray),
		// `\x1b[39m` is "default foreground", which is the absence of a colour rather than a colour
		// named Reset — the same thing `Style::new()` means by `fg: None`.
		"39" => Style { fg: None, ..style },
		other => panic!("clack styled a Frame with SGR {other}, which the Theme has no name for"),
	}
}

/// An SGR "off" code, which is not [`Style::remove_modifier`].
///
/// `Style` describes a patch as well as an appearance: `sub_modifier` says "take this away from
/// whatever you are laid over". A byte stream has nothing underneath it — `\x1b[27m` means the text
/// after it is not reversed, full stop — so the bit is cleared rather than marked for subtraction.
/// A Theme sets appearances, never patches, which is why nothing in `theme.rs` carries a
/// `sub_modifier` either.
fn off(style: Style, modifier: Modifier) -> Style {
	Style {
		add_modifier: style.add_modifier.difference(modifier),
		..style
	}
}

/// A style asserted twice is a style asserted once. These pin the reader, not the port: if the
/// table above drifts from `theme.rs`, the comparison above starts agreeing for the wrong reason.
#[test]
fn the_sgr_table_reads_the_theme_back() {
	let styles = clackatui_core::theme::Styles::CLACK;
	assert_eq!(sgr(Style::new(), "90"), styles.guide);
	assert_eq!(sgr(Style::new(), "36"), styles.guide_active);
	assert_eq!(sgr(Style::new(), "2"), styles.submitted);
	assert_eq!(sgr(sgr(Style::new(), "9"), "2"), styles.cancelled);
	assert_eq!(sgr(sgr(Style::new(), "7"), "8"), styles.placeholder_empty);
	assert_eq!(sgr(Style::new(), "33"), styles.error);
}

/// A fixture is only worth trusting if it is complete. These guard the recording, not the port.
#[test]
fn the_fixture_is_a_plausible_recording() {
	let (json, scenarios) = fixture();

	assert_eq!(
		json["tag"].as_str(),
		Some(TAG),
		"the fixture was harvested from a clack other than the one README.md pins"
	);

	assert!(
		scenarios.len() >= 13,
		"fixture has shrunk to {} scenarios; a partial harvest passes for free",
		scenarios.len()
	);

	let names: BTreeSet<&str> = scenarios.iter().map(|s| s.name.as_str()).collect();
	assert_eq!(names.len(), scenarios.len(), "duplicate Scenario names");

	// One per branch of clack's `text` renderer, so that a harvest which lost a whole state — the
	// error frame, say — is loud rather than merely smaller.
	for required in [
		"text › renders message",
		"text › renders placeholder if set",
		"text › renders submitted value",
		"text › renders cancelled value if one set",
		"text › validation errors render and clear",
		"text › withGuide: false removes guide",
		"text › defaultValue sets the value but does not render",
	] {
		assert!(
			names.contains(required),
			"fixture lost the `{required}` Scenario"
		);
	}

	for scenario in &scenarios {
		assert_eq!(
			scenario.kind, "text",
			"{}: not a text prompt",
			scenario.name
		);
		assert!(
			!scenario.message.is_empty(),
			"{}: no message",
			scenario.name
		);
		assert!(
			!scenario.output.is_empty(),
			"{}: nothing was written",
			scenario.name
		);
	}

	// Two Scenarios carry a callback and one is driven by an `AbortSignal` rather than by keys.
	// Both are counted rather than described, so that a harvest which quietly turns more of the
	// suite into something unreplayable shows up here.
	let validating = scenarios.iter().filter(|s| s.validates).count();
	assert_eq!(validating, 2, "Scenarios carrying a validate callback");

	let keyless = scenarios.iter().filter(|s| s.keys.is_empty()).count();
	assert_eq!(keyless, 1, "Scenarios that send no keypresses");
}
