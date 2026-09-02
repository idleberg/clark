//! Conformance: the wrap port against `fast-wrap-ansi` itself.
//!
//! Companion to `width_parity.rs`, and the same arrangement for the same reason: `upstream/` is not
//! committed, so `scripts/harvest-wrap.mjs` runs the real library over the corpus and records its
//! answers in `fixtures/wrap.json`, and this asserts against the recording (ADR-0008).
//!
//! What is being checked is not decoration. clack does not let the terminal wrap — `@clack/core`'s
//! render wraps the Frame itself and writes the result, and `restoreCursor` counts rows off the same
//! string — so a break in the wrong place moves every row after it and the cursor with them
//! (ADR-0012).
//!
//! Every case is a list of code points, never a string literal, for the reason `width_parity.rs`
//! gives. The corpus carries no ANSI escapes: upstream reopens the style it closed at a break, which
//! a Frame has no need of because it carries styling as a `Style` per span.

use std::collections::BTreeSet;

use clark_core::wrap::{rows, wrap};

const FIXTURE: &str = include_str!("fixtures/wrap.json");

struct Fixture {
	version: String,
	cases: Vec<Case>,
}

struct Case {
	name: String,
	codepoints: Vec<u32>,
	columns: usize,
	rows: Vec<String>,
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
	let json: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixtures/wrap.json parses");

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
			// Signed on the way in and clamped on the way out. Upstream's width is
			// `columns - columnPadding` and can go negative; the port's cannot, and does not
			// need to — a negative width and a zero one wrap identically, which is asserted by
			// the corpus carrying both rather than by this line saying so.
			columns: case["columns"]
				.as_i64()
				.expect("columns is a number")
				.max(0) as usize,
			rows: case["rows"]
				.as_array()
				.expect("rows is an array")
				.iter()
				.map(|row| row.as_str().expect("row is a string").to_owned())
				.collect(),
		})
		.collect();

	Fixture {
		version: json["version"].as_str().unwrap_or("unknown").to_owned(),
		cases,
	}
}

/// Every case at once, so one run reports every disagreement rather than the first.
#[test]
fn every_case_wraps_where_clack_wraps_it() {
	let fixture = fixture();
	let mut failures = Vec::new();

	for case in &fixture.cases {
		let ours: Vec<String> = wrap(&case.text(), case.columns)
			.split('\n')
			.map(str::to_owned)
			.collect();

		if ours != case.rows {
			failures.push(format!(
				"  {:<38} at {} columns\n      expected {:?}\n      got      {:?}",
				case.name, case.columns, case.rows, ours
			));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {} cases wrap differently from fast-wrap-ansi@{}.\n\n{}\n\n\
		 Re-harvest with `node scripts/harvest-wrap.mjs` to see whether upstream moved.",
		failures.len(),
		fixture.cases.len(),
		fixture.version,
		failures.join("\n"),
	);
}

/// A Frame wraps a line by splitting the spans that make it up, which is only sound if wrapping is
/// nothing but a set of break positions. Under `trim: false` it is — no space is eaten and no
/// character is added — and this is what says so for the whole corpus.
#[test]
fn wrapping_a_line_neither_adds_nor_removes_a_character() {
	let fixture = fixture();
	let mut failures = Vec::new();

	for case in &fixture.cases {
		let text = case.text();
		for line in text.split('\n') {
			let line = line.strip_suffix('\r').unwrap_or(line);
			let rejoined: String = rows(line, case.columns).concat();
			if rejoined != line {
				failures.push(format!(
					"  {:<38} {line:?} came back as {rejoined:?}",
					case.name
				));
			}
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {} cases do not rejoin from their rows.\n\n{}",
		failures.len(),
		fixture.cases.len(),
		failures.join("\n"),
	);
}

/// A fixture is only worth trusting if it is complete. This guards the recording, not the port.
#[test]
fn the_fixture_is_a_plausible_recording() {
	let fixture = fixture();

	assert!(
		fixture.cases.len() >= 40,
		"fixture has shrunk to {} cases; a partial harvest passes for free",
		fixture.cases.len()
	);

	let names: BTreeSet<&str> = fixture.cases.iter().map(|c| c.name.as_str()).collect();
	assert_eq!(
		names.len(),
		fixture.cases.len(),
		"duplicate case names in the fixture"
	);

	// Each of these is a case the port would get wrong if a whole branch were deleted: the mid-word
	// break, the row that a break lands on the space of, the look-ahead that decides which row a
	// long word starts on, and the widths that are not one.
	for required in [
		"long word alone",
		"long word after a short one",
		"break falls on the space",
		"double space",
		"emoji sequence broken mid sequence",
		"combining mark at the margin",
		"tab",
		"crlf",
		"whole opening frame",
	] {
		assert!(
			names.contains(required),
			"fixture lost the `{required}` case"
		);
	}

	// A corpus that never wraps anything would pass the parity test for free.
	let wrapped = fixture
		.cases
		.iter()
		.filter(|c| c.rows.len() > c.text().split('\n').count())
		.count();
	assert!(
		wrapped >= 20,
		"only {wrapped} cases actually wrap; the corpus has stopped exercising the wrap"
	);
}
