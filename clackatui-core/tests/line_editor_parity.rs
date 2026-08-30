//! Conformance: the line editor against Node's `readline` itself.
//!
//! ADR-0004 asks for a suite that drives readline directly and asserts `(line, cursor)` after each
//! key, so that a word-boundary disagreement is reported once as a keymap defect rather than as a
//! dozen unexplained Grid mismatches. This is that suite.
//!
//! As with the width port, the JavaScript side does not run here: `prior-art/` is not committed and
//! CI has no Node to drive, so `scripts/harvest-line-editor.mjs` records readline's answers into
//! `fixtures/line-editor.json` and this test replays them. See ADR-0008 for why the comparison is
//! harvested rather than live.
//!
//! Every string in the fixture is a list of code points, never a literal — the corpus is full of
//! combining marks, a byte order mark and C0 controls, none of which survive a round trip through a
//! source file intact.

use std::collections::BTreeSet;

use clackatui_core::line_editor::{Key, KeyName, LineEditor};

const FIXTURE: &str = include_str!("fixtures/line-editor.json");

struct Scenario {
	name: String,
	steps: Vec<Step>,
}

struct Step {
	s: Option<String>,
	key: Key,
	line: String,
	/// `rl.cursor`, which is a UTF-16 offset.
	cursor: usize,
}

fn text(value: &serde_json::Value, what: &str) -> String {
	value
		.as_array()
		.unwrap_or_else(|| panic!("{what} is an array"))
		.iter()
		.map(|cp| {
			let cp = cp
				.as_u64()
				.unwrap_or_else(|| panic!("{what} holds numbers")) as u32;
			char::from_u32(cp).unwrap_or_else(|| panic!("{what}: {cp:#X} is not a scalar value"))
		})
		.collect()
}

/// Node reports a letter key by its own lowercase character and everything else by a fixed name.
/// An unrecognised name is a fixture the port has no branch for, which is worth failing loudly on
/// rather than silently treating as inert.
fn key_name(name: &str) -> KeyName {
	let mut chars = name.chars();
	if let (Some(c), None) = (chars.next(), chars.next()) {
		return KeyName::Char(c);
	}
	match name {
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
		other => panic!("fixture uses key name `{other}`, which the port does not model"),
	}
}

fn fixture() -> Vec<Scenario> {
	let json: serde_json::Value =
		serde_json::from_str(FIXTURE).expect("fixtures/line-editor.json parses");

	json["scenarios"]
		.as_array()
		.expect("scenarios is an array")
		.iter()
		.map(|scenario| Scenario {
			name: scenario["name"]
				.as_str()
				.expect("name is a string")
				.to_owned(),
			steps: scenario["steps"]
				.as_array()
				.expect("steps is an array")
				.iter()
				.map(|step| {
					let key = &step["key"];
					Step {
						s: step["s"].as_array().map(|_| text(&step["s"], "s")),
						key: Key {
							name: key["name"].as_str().map(key_name),
							ctrl: key["ctrl"].as_bool().unwrap_or(false),
							meta: key["meta"].as_bool().unwrap_or(false),
							shift: key["shift"].as_bool().unwrap_or(false),
							sequence: key["sequence"]
								.as_array()
								.map(|_| text(&key["sequence"], "sequence")),
						},
						line: text(&step["line"], "line"),
						cursor: step["cursor"].as_u64().expect("cursor is a number") as usize,
					}
				})
				.collect(),
		})
		.collect()
}

/// Renders a key the way a person would say it, for the failure message.
fn describe(key: &Key) -> String {
	let mut parts = Vec::new();
	if key.ctrl {
		parts.push("ctrl".to_owned());
	}
	if key.meta {
		parts.push("alt".to_owned());
	}
	if key.shift {
		parts.push("shift".to_owned());
	}
	parts.push(match key.name {
		Some(KeyName::Char(c)) => format!("{c:?}"),
		Some(other) => format!("{other:?}").to_lowercase(),
		None => match &key.sequence {
			Some(seq) => seq
				.chars()
				.map(|c| format!("{}", c.escape_unicode()))
				.collect(),
			None => "(no key)".to_owned(),
		},
	});
	parts.join("+")
}

/// The whole point of the file. Every scenario is replayed to the end even after it diverges, and
/// every scenario is run, so one failure reports the full extent of the disagreement.
#[test]
fn every_keypress_leaves_the_editor_where_readline_leaves_it() {
	let scenarios = fixture();
	let mut failures = Vec::new();

	for scenario in &scenarios {
		let mut editor = LineEditor::new();
		let mut already_diverged = false;

		for (index, step) in scenario.steps.iter().enumerate() {
			editor.write(step.s.as_deref(), &step.key);

			let ours = (editor.line(), editor.cursor_utf16());
			let theirs = (step.line.as_str(), step.cursor);
			if ours == theirs {
				continue;
			}

			// Only the first divergence in a scenario is a finding; everything after it is
			// downstream of a state that is already wrong.
			if !already_diverged {
				already_diverged = true;
				failures.push(format!(
					"  {}\n    step {} ({})\n    readline: {:?} cursor {}\n    ours:     {:?} cursor {}",
					scenario.name,
					index,
					describe(&step.key),
					theirs.0,
					theirs.1,
					ours.0,
					ours.1,
				));
			}
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {} scenarios diverge from node:readline.\n\n{}\n\n\
		 Re-harvest with `node scripts/harvest-line-editor.mjs` to check whether Node moved; the\n\
		 keymap lives in `internal/readline/interface.js`, in `kTtyWrite` and the primitives it\n\
		 dispatches to.",
		failures.len(),
		scenarios.len(),
		failures.join("\n\n"),
	);
}

/// A fixture is only worth trusting if it is complete. These guard the recording itself, not the
/// port: a truncated or duplicated harvest would otherwise pass silently.
#[test]
fn the_fixture_is_a_plausible_recording() {
	let scenarios = fixture();

	assert!(
		scenarios.len() >= 55,
		"fixture has shrunk to {} scenarios; a partial harvest passes for free",
		scenarios.len()
	);

	let keypresses: usize = scenarios.iter().map(|s| s.steps.len()).sum();
	assert!(
		keypresses >= 450,
		"fixture has shrunk to {keypresses} keypresses"
	);

	let names: BTreeSet<&str> = scenarios.iter().map(|s| s.name.as_str()).collect();
	assert_eq!(
		names.len(),
		scenarios.len(),
		"duplicate scenario names in the fixture"
	);

	// One per branch of the keymap that has its own opinion about where a boundary is. Each of
	// these is a scenario the port would get wrong if that branch were deleted.
	for required in [
		"word right stops inside a punctuation run",
		"delete word right swallows the whole punctuation run",
		"a byte order mark counts as whitespace",
		"accented letters are not word characters",
		"astral code points move as one",
		"combining marks are separate stops",
		"yank pop walks the ring",
		"a duplicate kill is not pushed twice",
		"undo steps back through typing",
		"return discards the undo history",
		"a pasted string with newlines submits each line",
		"ctrl shift backspace and delete",
	] {
		assert!(
			names.contains(required),
			"fixture lost the `{required}` scenario"
		);
	}
}

/// The port indexes in bytes and readline indexes in UTF-16 units. Byte offsets are never compared
/// against the fixture, so this checks the other half of the claim: that the byte cursor is always
/// a real character boundary in the line it points into.
#[test]
fn the_byte_cursor_stays_on_a_character_boundary() {
	for scenario in &fixture() {
		let mut editor = LineEditor::new();
		for step in &scenario.steps {
			editor.write(step.s.as_deref(), &step.key);
			assert!(
				editor.line().is_char_boundary(editor.cursor()),
				"{}: cursor {} is inside a character of {:?}",
				scenario.name,
				editor.cursor(),
				editor.line()
			);
		}
	}
}
