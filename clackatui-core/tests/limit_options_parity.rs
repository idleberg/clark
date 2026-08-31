//! Conformance: the `limitOptions` port against clack's own.
//!
//! The fourth harvested oracle, and the same arrangement as the width, wrap and Emitter ones for the
//! same reason: `prior-art/` is not committed, so `scripts/harvest-limit-options.mjs` runs the real
//! function over the corpus and records its answers in `fixtures/limit-options.json`, and this
//! asserts against the recording (ADR-0008).
//!
//! What is being checked is where a list is cut. Every list Prompt in M3 and M4 draws through this,
//! and a window off by one puts a different option under the cursor — not a difference in
//! decoration but in which answer the user is about to give.
//!
//! # What the corpus can and cannot carry
//!
//! Every style in `scripts/limit-options/cases.mjs` is plain, so the only styled thing in a
//! recording is the `...` overflow row, carried as a flag rather than as an escape (ADR-0011). That
//! leaves one thing unasserted here: that the port styles the overflow row the way clack does. The
//! unit tests in `limit_options.rs` hold that against the Theme, and the Grid comparison will hold
//! it against clack once `select` lands.
//!
//! Every string is a list of code points, never a literal, for the reason `width_parity.rs` gives.

use std::collections::BTreeSet;

use clackatui_core::frame::{Line, Span};
use clackatui_core::limit_options::{LimitOptions, OVERFLOW};

const FIXTURE: &str = include_str!("fixtures/limit-options.json");

struct Fixture {
	tag: String,
	cases: Vec<Case>,
}

struct Case {
	name: String,
	options: Vec<String>,
	cursor: usize,
	columns: usize,
	rows: usize,
	max_items: Option<usize>,
	column_padding: Option<usize>,
	row_padding: Option<usize>,
	style: String,
	lines: Vec<Row>,
}

/// One row of a recording: its text, and whether it is the overflow row rather than an option.
struct Row {
	text: String,
	overflow: bool,
}

impl Case {
	/// The case, built the way `select` builds one.
	fn limit(&self) -> LimitOptions<'_, String> {
		let mut limit = LimitOptions::new(&self.options, self.cursor)
			.with_columns(self.columns)
			.with_rows(self.rows);
		if let Some(max_items) = self.max_items {
			limit = limit.with_max_items(max_items);
		}
		if let Some(padding) = self.column_padding {
			limit = limit.with_column_padding(padding);
		}
		if let Some(padding) = self.row_padding {
			limit = limit.with_row_padding(padding);
		}
		limit
	}

	/// The style the case names. None of them styles — see the module docs.
	fn style(&self, option: &String, active: bool) -> Vec<Line> {
		let text = match self.style.as_str() {
			"plain" => option.clone(),
			"marker" => {
				if active {
					format!("> {option}")
				} else {
					format!("  {option}")
				}
			}
			"wide" => format!("-- {option} --"),
			other => panic!("{}: unknown style {other}", self.name),
		};
		// The callback returns the rows an option occupies before wrapping, which is upstream's
		// `computeLabel` splitting a label on its line breaks.
		text.split('\n')
			.map(|line| Span::raw(line).into())
			.collect()
	}

	fn ours(&self) -> Vec<Row> {
		self.limit()
			.lines(|option, active| self.style(option, active))
			.iter()
			.map(|line| {
				let text: String = line.spans.iter().map(|span| span.text.as_str()).collect();
				// The port draws the overflow row as one styled span and nothing else does, which is
				// what makes this readable off the Line rather than off a flag passed alongside it.
				let overflow = text == OVERFLOW
					&& line.spans.len() == 1
					&& line.spans[0].style != Default::default();
				Row { text, overflow }
			})
			.collect()
	}
}

fn text(rows: &[Row]) -> Vec<String> {
	rows.iter()
		.map(|row| {
			if row.overflow {
				format!("<{}>", row.text)
			} else {
				row.text.clone()
			}
		})
		.collect()
}

fn codepoints(value: &serde_json::Value, what: &str) -> String {
	value
		.as_array()
		.unwrap_or_else(|| panic!("{what} is an array"))
		.iter()
		.map(|cp| {
			let cp = cp.as_u64().expect("code point is a number") as u32;
			char::from_u32(cp).unwrap_or_else(|| panic!("{cp:#X} is not a scalar value"))
		})
		.collect()
}

fn optional(value: &serde_json::Value) -> Option<usize> {
	value.as_u64().map(|n| n as usize)
}

fn fixture() -> Fixture {
	let json: serde_json::Value =
		serde_json::from_str(FIXTURE).expect("fixtures/limit-options.json parses");

	let cases = json["cases"]
		.as_array()
		.expect("cases is an array")
		.iter()
		.map(|case| Case {
			name: case["name"].as_str().expect("name is a string").to_owned(),
			options: case["options"]
				.as_array()
				.expect("options is an array")
				.iter()
				.map(|option| codepoints(option, "an option"))
				.collect(),
			cursor: case["cursor"].as_u64().expect("cursor is a number") as usize,
			columns: case["columns"].as_u64().expect("columns is a number") as usize,
			rows: case["rows"].as_u64().expect("rows is a number") as usize,
			max_items: optional(&case["maxItems"]),
			column_padding: optional(&case["columnPadding"]),
			row_padding: optional(&case["rowPadding"]),
			style: case["style"]
				.as_str()
				.expect("style is a string")
				.to_owned(),
			lines: case["lines"]
				.as_array()
				.expect("lines is an array")
				.iter()
				.map(|line| Row {
					text: codepoints(&line["codepoints"], "a line"),
					overflow: line["overflow"].as_bool().expect("overflow is a bool"),
				})
				.collect(),
		})
		.collect();

	Fixture {
		tag: json["tag"].as_str().unwrap_or("unknown").to_owned(),
		cases,
	}
}

/// Every case at once, so one run reports every disagreement rather than the first.
#[test]
fn every_case_is_cut_where_clack_cuts_it() {
	let fixture = fixture();
	let mut failures = Vec::new();

	for case in &fixture.cases {
		let ours = case.ours();
		if text(&ours) != text(&case.lines) {
			failures.push(format!(
				"  {}\n      expected {:?}\n      got      {:?}",
				case.name,
				text(&case.lines),
				text(&ours),
			));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {} cases are cut differently from clack's limitOptions at {}.\n\n{}\n\n\
		 Re-harvest with `node scripts/harvest-limit-options.mjs` to see whether upstream moved.",
		failures.len(),
		fixture.cases.len(),
		fixture.tag,
		failures.join("\n"),
	);
}

/// A fixture is only worth trusting if it is complete. This guards the recording, not the port.
#[test]
fn the_fixture_is_a_plausible_recording() {
	let fixture = fixture();

	assert!(
		fixture.cases.len() >= 50,
		"fixture has shrunk to {} cases; a partial harvest passes for free",
		fixture.cases.len()
	);

	let names: BTreeSet<&str> = fixture.cases.iter().map(|c| c.name.as_str()).collect();
	assert_eq!(
		names.len(),
		fixture.cases.len(),
		"duplicate case names in the fixture"
	);

	// Each of these is a branch the port would get wrong if it were deleted: the floor of five, the
	// window's three-option lead, the second trim over groups of uneven height, and the width that
	// goes to nothing.
	for required in [
		"maxItems below the floor",
		"a terminal shorter than the padding",
		"cursor at 3 of ten with a window of five",
		"a tall option in the middle",
		"the cursor is on a tall option taller than the terminal",
		"a padding as wide as the terminal",
		"the marker follows the cursor",
	] {
		assert!(
			names.contains(required),
			"fixture lost the `{required}` case"
		);
	}

	// A corpus in which nothing is ever cut would pass the parity test for free.
	let cut = fixture
		.cases
		.iter()
		.filter(|case| case.lines.iter().any(|line| line.overflow))
		.count();
	assert!(
		cut >= 15,
		"only {cut} cases overflow; the corpus has stopped exercising the window"
	);

	// And one in which nothing wraps would leave the second trim untested.
	let wrapped = fixture
		.cases
		.iter()
		.filter(|case| case.lines.len() > case.options.len())
		.count();
	assert!(
		wrapped >= 8,
		"only {wrapped} cases produce more rows than options; the wrap inside is untested"
	);
}
