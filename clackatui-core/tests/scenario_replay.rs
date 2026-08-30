//! The `text` Scenarios, harvested from clack's own test suite by `scripts/harvest-scenarios.mjs`.
//!
//! ADR-0003 makes upstream's tests the specification: each of them is a Prompt configuration, a
//! sequence of keypresses, and the output clack wrote back. ADR-0010 describes how they are caught.
//! The recording is what this file reads; no JavaScript runs here, for the reasons ADR-0008 gives.
//!
//! What is asserted is not yet the Grid. The Emitter and the `text` widget do not exist, so the
//! recorded output chunks are carried but not compared — that is what M1 finishes with. Two things
//! are checked in the meantime, and both are worth having on their own:
//!
//!   - the fixture is a plausible recording, so that a truncated harvest cannot pass for free;
//!   - every Scenario replayed through [`Prompt<TextState>`] settles in the state clack settled in.
//!
//! The second is the first outside check the Prompt state machine has had. ADR-0009 notes it was
//! ported by close reading alone, against no oracle; the state clack's last frame reports is a
//! small one, but it is upstream's answer rather than ours.

use std::collections::BTreeSet;

use clackatui_core::line_editor::{Key, KeyName};
use clackatui_core::prompt::{Prompt, Status};
use clackatui_core::text::TextState;

const FIXTURE: &str = include_str!("fixtures/scenarios/text.json");

/// The tag `README.md` names. A fixture from anywhere else is not the thing we claim parity with.
const TAG: &str = "@clack/prompts@1.7.0";

struct Scenario {
	name: String,
	kind: String,
	message: String,
	default_value: Option<String>,
	initial_value: Option<String>,
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
				default_value: opts["defaultValue"].as_str().map(str::to_owned),
				initial_value: opts["initialValue"].as_str().map(str::to_owned),
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
