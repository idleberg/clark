//! Conformance: the width port against `fast-string-width` itself.
//!
//! ADR-0005 asks for a suite that feeds one corpus to both the JavaScript library and the Rust port
//! and asserts equal widths. This is that suite for measurement; wrap points follow when
//! `fast-wrap-ansi` is ported.
//!
//! The JavaScript side does not run here. `prior-art/` is not committed, so there is no clack
//! checkout on CI to compare against — instead `scripts/harvest-width.mjs` runs the real library
//! over the corpus and records its answers in `fixtures/width.json`, and this test asserts against
//! that recording. The fixture is refreshed deliberately, when the pinned clack version moves, which
//! is the same Recorder-and-Fixture arrangement the Prompt Scenarios use.
//!
//! Every case is a list of code points, never a string literal, because a decomposed sequence
//! written literally into a source file is silently precomposed on the way to disk.

use std::collections::BTreeSet;

use clackatui_core::width::{segments, width};

const FIXTURE: &str = include_str!("fixtures/width.json");

struct Fixture {
	unicode: String,
	version: String,
	cases: Vec<Case>,
}

struct Case {
	name: String,
	codepoints: Vec<u32>,
	width: usize,
}

impl Case {
	fn text(&self) -> String {
		self.codepoints
			.iter()
			.map(|cp| {
				char::from_u32(*cp)
					.unwrap_or_else(|| panic!("{}: {cp:#X} is not a scalar value", self.name))
			})
			.collect()
	}
}

fn fixture() -> Fixture {
	let json: serde_json::Value =
		serde_json::from_str(FIXTURE).expect("fixtures/width.json parses");

	let cases = json["cases"]
		.as_array()
		.expect("cases is an array")
		.iter()
		.map(|case| Case {
			name: case["name"].as_str().expect("name is a string").to_owned(),
			codepoints: case["codepoints"]
				.as_array()
				.expect("codepoints is an array")
				.iter()
				.map(|cp| cp.as_u64().expect("code point is a number") as u32)
				.collect(),
			width: case["width"].as_u64().expect("width is a number") as usize,
		})
		.collect();

	Fixture {
		unicode: json["unicode"].as_str().unwrap_or("unknown").to_owned(),
		version: json["version"].as_str().unwrap_or("unknown").to_owned(),
		cases,
	}
}

/// The whole point of the file. Every case at once, so one run reports every disagreement rather
/// than the first.
#[test]
fn every_case_measures_the_way_clack_measures_it() {
	let fixture = fixture();
	let mut failures = Vec::new();

	for case in &fixture.cases {
		let ours = width(&case.text());
		if ours != case.width {
			let cps: Vec<String> = case
				.codepoints
				.iter()
				.map(|cp| format!("{cp:04X}"))
				.collect();
			failures.push(format!(
				"  {:<38} {}  expected {}, got {}",
				case.name,
				cps.join(" "),
				case.width,
				ours
			));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {} cases disagree with fast-string-width@{}.\n\n{}\n\n\
		 The fixture was harvested under Unicode {}; unicode-properties here is built against {}.\n\
		 If those differ, suspect the tables before the scanner — the ported regexes lean on \\p{{Emoji}},\n\
		 \\p{{Emoji_Modifier_Base}}, \\p{{Emoji_Presentation}}, \\p{{Script=…}} and \\p{{M}}, so a table bump\n\
		 moves answers without moving code. Re-harvest with `node scripts/harvest-width.mjs` to see\n\
		 whether upstream moved too.",
		failures.len(),
		fixture.cases.len(),
		fixture.version,
		failures.join("\n"),
		fixture.unicode,
		unicode_version(),
	);
}

/// The Frame places text one segment to a cell, so nothing may be lost or duplicated on the way
/// from a string to the cells that hold it.
///
/// Only the *coverage* is asserted here. That the segments carry the right widths is already the
/// subject of the test above, transitively: [`width`] is defined as their sum, so a segment measured
/// wrongly is a case measured wrongly. Asserting the sum again here would be a tautology.
#[test]
fn every_case_is_covered_by_its_segments() {
	let fixture = fixture();
	let mut failures = Vec::new();

	for case in &fixture.cases {
		let text = case.text();
		let rejoined: String = segments(&text).map(|s| s.text).collect();

		if rejoined != text {
			failures.push(format!(
				"  {:<38} {} code points in, {} out",
				case.name,
				text.chars().count(),
				rejoined.chars().count()
			));
		}
		if segments(&text).any(|s| s.text.is_empty()) {
			failures.push(format!("  {:<38} produced an empty segment", case.name));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {} cases do not rejoin from their segments.\n\n{}\n\n\
		 A Frame draws what `segments` yields and nothing else, so text missing here is text \
		 missing from the Grid.",
		failures.len(),
		fixture.cases.len(),
		failures.join("\n"),
	);
}

/// A fixture is only worth trusting if it is complete. These guard the recording itself, not the
/// port: a truncated or duplicated harvest would otherwise pass silently.
#[test]
fn the_fixture_is_a_plausible_recording() {
	let fixture = fixture();

	assert!(
		fixture.cases.len() >= 80,
		"fixture has shrunk to {} cases; a partial harvest passes for free",
		fixture.cases.len()
	);

	let names: BTreeSet<&str> = fixture.cases.iter().map(|c| c.name.as_str()).collect();
	assert_eq!(
		names.len(),
		fixture.cases.len(),
		"duplicate case names in the fixture"
	);

	// The corpus is only interesting if it still exercises every block of the scanner. Each of
	// these is a case the port would get wrong if a whole branch were deleted.
	for required in [
		"conjoining jamo",
		"keycap",
		"scotland tag sequence",
		"zwj family",
		"osc8 bel terminated",
		"csi ending in a digit",
		"three tabs",
		"stacked marks",
		"halfwidth katakana",
		"lone zwj",
	] {
		assert!(
			names.contains(required),
			"fixture lost the `{required}` case"
		);
	}
}

/// Not a failure on its own — the port can be right under two table versions — but a drifting table
/// is the first thing to check when the parity test above starts failing, so it is worth seeing.
#[test]
fn unicode_tables_match_the_harvest() {
	let fixture = fixture();
	let ours = unicode_version();

	assert_eq!(
		major(&fixture.unicode),
		major(&ours),
		"fixture harvested under Unicode {}, unicode-properties built against {}",
		fixture.unicode,
		ours
	);
}

fn unicode_version() -> String {
	let (major, minor, patch) = unicode_properties::UNICODE_VERSION;
	format!("{major}.{minor}.{patch}")
}

fn major(version: &str) -> &str {
	version.split('.').next().unwrap_or(version)
}
