//! The Scenarios, read back out of their recordings.
//!
//! ADR-0003 makes upstream's tests the specification: each of them is a Prompt configuration, a
//! sequence of keypresses, and the output clack wrote back. ADR-0010 describes how they are caught,
//! by `scripts/harvest-scenarios.mjs`, which is run once per Prompt — `text`, `password`, `confirm`.
//! Twenty-two more are hand-authored, because upstream's tests never vary the terminal and so can
//! say nothing about a wrap or a resize, and because three of the behaviours M2 found are reached by
//! no test upstream has; ADR-0016 describes what that costs and what stands in for the oracle a
//! harvest has. The recordings are what this file reads; no JavaScript runs here, for the reasons
//! ADR-0008 gives.
//!
//! No emulator runs here either. That is `scenario_parity.rs`, which is where the Grid comparison
//! ADR-0001 asks for lives. What is left in this file is the set of checks that read the recording
//! *directly* — each cheaper than a Grid comparison, each failing with a smaller and more specific
//! message when it goes wrong, which is the whole reason for keeping them once the Grid is green:
//!
//!   - the fixture is a plausible recording, so that a truncated harvest cannot pass for free;
//!   - every Scenario replayed through its Prompt settles in the state clack settled in;
//!   - every Scenario's *opening* Frame is drawn the way clack drew it, styles included — the one
//!     Frame upstream writes whole rather than as a diff, because it has nothing to diff against;
//!   - every Scenario's whole byte stream, styling stripped from both sides, is the stream clack
//!     wrote — which covers the diffs, the erasures and the order a `Session` asks for them in.
//!
//! The second was the first outside check the Prompt state machine had; ADR-0009 notes it was
//! ported by close reading alone, against no oracle. The third is the first check on appearance,
//! and the first thing in the project to compare bytes clack actually wrote with something the port
//! actually drew. The fourth covers a Prompt end to end, and is the one a Grid comparison most
//! nearly subsumes — it survives because a Grid says two terminals look alike, while this says the
//! port asked for the same work, which is what an author reads when the two stop agreeing.

use std::collections::BTreeSet;

use clackatui_core::frame::Frame;
use clackatui_core::prompt::Status;
use ratatui_core::style::{Color, Modifier, Style};

mod scenarios;
use scenarios::{TAG, all, authored, confirm, harvested, password};

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
	let scenarios = all();
	let mut failures = Vec::new();
	let mut replayed = 0;

	for scenario in &scenarios {
		if !scenario.is_replayable() {
			continue;
		}

		let Some(expected) = settled(&scenario.output) else {
			failures.push(format!(
				"  {:<52} no step symbol in the output",
				scenario.name
			));
			continue;
		};

		let settled = scenario.settles();

		replayed += 1;
		if settled != expected {
			failures.push(format!(
				"  {:<52} clack settled {:?}, the port settled {:?}",
				scenario.name, expected, settled
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
		replayed >= 49,
		"only {replayed} Scenarios were replayable; the filters above have eaten the suite"
	);
}

// --- The first Frame ---------------------------------------------------------------------------

/// The first thing clack writes for a Prompt is its whole opening Frame, uncut: `render` has no
/// previous frame to diff against, so it prints the lot. Every later write is a diff, and making
/// sense of one needs a terminal emulator — but this one is a Frame on its own, and the widget can
/// be held against it directly, colours and all.
///
/// It keeps earning its place next to the Grid comparison for two reasons. It runs against every
/// Scenario, including the ones a recording cannot replay; and it compares styles the emulator has
/// no model for, conceal in particular, which is exactly the attribute clack's empty placeholder is
/// drawn with.
///
/// What it cannot do is a Scenario whose opening Frame wraps. What clack recorded is post-wrap —
/// `render` wraps the Frame to the terminal before it writes it (ADR-0012) — and the port's side
/// here is a [`Frame`], which is pre-wrap by definition; laying it out is `Frame::rows`, which is
/// the Emitter's and not public. Those Scenarios are left to the Grid, which sees the same rows
/// clack wrote, and counted here so that "left to the Grid" cannot quietly become "left out".
#[test]
fn every_scenario_draws_clacks_opening_frame() {
	let scenarios = all();
	let mut failures = Vec::new();
	let mut compared = 0;
	let mut wrapped = 0;

	for scenario in &scenarios {
		let Some(recorded) = opening_frame(&scenario.output) else {
			failures.push(format!("  {}: nothing was drawn", scenario.name));
			continue;
		};

		let frame = scenario.opening_frame();

		if frame
			.lines
			.iter()
			.any(|line| line.width() > scenario.columns)
		{
			wrapped += 1;
			continue;
		}

		compared += 1;
		let ours = flatten(&frame);
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
		compared >= 50,
		"only {compared} Frames were compared; the fixtures have stopped carrying them"
	);

	// The hand-authored Fixture exists to reach a wrap, so at least one of its Scenarios has to be
	// past this test's reach. If none is, it is not testing what it was written for.
	assert!(
		wrapped >= 3,
		"{wrapped} opening Frames were too wide to compare unwrapped; the authored Scenarios have \
		 stopped wrapping"
	);
}

// --- The whole stream --------------------------------------------------------------------------

/// Every byte clack wrote for a Scenario, against every byte a `Session` writes for it — with the
/// styling taken off both sides first.
///
/// This is not the Grid comparison, and does not stand in for one: nothing here knows where the
/// cursor *ends up*, only which instructions were issued to move it. It survives alongside the Grid
/// because the two fail differently. A Grid says the two terminals do not look alike, and leaves an
/// author to work out which write caused it; this says the port asked for different work, in a
/// stream short enough to read.
///
/// SGR is stripped from both sides because the two encode the same appearance differently and
/// deliberately: clack's Frames arrive as picocolors output, which states one attribute per escape
/// and turns each off again by name, while the Emitter states a whole [`Style`] per run and resets
/// (ADR-0011, ADR-0013). Colour is what the opening-Frame test above and the Grid comparison
/// compare, each in the terms it can. What is left after stripping is the part where byte equality
/// is the right question: cursor movement, erasure, and text.
#[test]
fn every_scenario_is_written_the_way_clack_wrote_it() {
	let scenarios = all();
	let mut failures = Vec::new();
	let mut compared = 0;

	for scenario in &scenarios {
		if !scenario.is_replayable() {
			continue;
		}

		compared += 1;
		let ours = strip_sgr(&scenario.replay());
		let theirs = strip_sgr(&scenario.recorded());
		if ours != theirs {
			failures.push(format!(
				"  {}\n     clack: {}\n      port: {}",
				scenario.name,
				readable(&theirs),
				readable(&ours),
			));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {compared} Scenarios are written differently from clack.\n\n{}\n",
		failures.len(),
		failures.join("\n"),
	);

	assert!(
		compared >= 49,
		"only {compared} Scenarios were compared; the filters above have eaten the suite"
	);
}

/// A byte stream with every SGR sequence removed. Anything else that starts with `ESC[` is kept.
fn strip_sgr(stream: &str) -> String {
	let mut out = String::new();
	let mut chars = stream.chars().peekable();

	while let Some(c) = chars.next() {
		if c != '\u{1B}' {
			out.push(c);
			continue;
		}
		let mut sequence = String::from(c);
		if chars.peek() != Some(&'[') {
			out.push_str(&sequence);
			continue;
		}
		sequence.push(chars.next().expect("peeked"));
		// Parameter and intermediate bytes, then one final byte in `@`..`~`.
		for c in chars.by_ref() {
			sequence.push(c);
			if ('\u{40}'..='\u{7E}').contains(&c) {
				break;
			}
		}
		if !sequence.ends_with('m') {
			out.push_str(&sequence);
		}
	}

	out
}

/// Escapes made legible, so a failure reads as a sequence rather than as a wall of `\u{1b}`.
fn readable(stream: &str) -> String {
	stream.replace('\u{1B}', "ESC").replace('\n', "\\n")
}

/// The stripper is load-bearing for the comparison above — if it ate too much, two streams that
/// differ would agree — so it is pinned on both sides' actual encodings.
#[test]
fn stripping_styles_leaves_the_movement_and_the_text() {
	// picocolors, as clack writes it.
	assert_eq!(strip_sgr("\u{1B}[7m\u{1B}[8m_\u{1B}[28m\u{1B}[27m"), "_");
	// The Emitter, which states a whole Style at once and resets.
	assert_eq!(strip_sgr("\u{1B}[0;7;8m_\u{1B}[0m"), "_");
	// Everything that is not SGR survives, including the private-mode cursor sequences.
	assert_eq!(
		strip_sgr("\u{1B}[?25l\u{1B}[999D\u{1B}[4A\u{1B}[1B\u{1B}[J\u{1B}[2K\u{1B}[G\u{1B}[?25h"),
		"\u{1B}[?25l\u{1B}[999D\u{1B}[4A\u{1B}[1B\u{1B}[J\u{1B}[2K\u{1B}[G\u{1B}[?25h"
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
fn the_harvested_fixture_is_a_plausible_recording() {
	let (json, scenarios) = harvested();

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

/// The same guard for M2's two Fixtures, which are harvested by the same script from the same
/// suite and so are worth exactly as much as the `text` one.
///
/// The required names are one per branch of each renderer, again — a harvest that lost the error
/// Frame or the cancelled one would otherwise be merely smaller, and the count below would let it
/// through.
#[test]
fn the_password_and_confirm_fixtures_are_plausible_recordings() {
	for (kind, (json, scenarios), least, required) in [
		(
			"password",
			password(),
			9,
			&[
				"password › renders message",
				"password › renders masked value",
				"password › renders custom mask",
				"password › renders cancelled value",
				"password › renders and clears validation errors",
				"password › clears input on error when clearOnError is true",
				"password › withGuide: false removes guide",
			][..],
		),
		(
			"confirm",
			confirm(),
			12,
			&[
				"confirm › renders message with choices",
				"confirm › can cancel",
				"confirm › can set initialValue",
				"confirm › renders custom active choice",
				"confirm › renders options in vertical alignment",
				"confirm › renders multi-line messages correctly",
				"confirm › right arrow moves to next choice",
				"confirm › withGuide: false removes guide",
			][..],
		),
	] {
		assert_eq!(
			json["tag"].as_str(),
			Some(TAG),
			"the {kind} Fixture was harvested from a clack other than the one README.md pins"
		);

		assert!(
			scenarios.len() >= least,
			"the {kind} Fixture has shrunk to {} Scenarios",
			scenarios.len()
		);

		let names: BTreeSet<&str> = scenarios.iter().map(|s| s.name.as_str()).collect();
		assert_eq!(names.len(), scenarios.len(), "duplicate Scenario names");

		for name in required {
			assert!(
				names.contains(name),
				"the {kind} Fixture lost the `{name}` Scenario"
			);
		}

		for scenario in &scenarios {
			assert_eq!(
				scenario.kind, kind,
				"{}: not a {kind} prompt",
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

		// One Scenario in each suite is driven by an `AbortSignal` rather than by keys.
		let keyless = scenarios.iter().filter(|s| s.keys.is_empty()).count();
		assert_eq!(keyless, 1, "{kind} Scenarios that send no keypresses");
	}

	// `confirm` takes no validator at all, so a `validate` callback in its Fixture would mean
	// upstream had grown one and the widget has a branch it does not draw.
	assert!(
		confirm().1.iter().all(|s| !s.validates),
		"a confirm Scenario carries a validate callback; upstream's `confirm` has no such option"
	);
	assert_eq!(
		password().1.iter().filter(|s| s.validates).count(),
		2,
		"password Scenarios carrying a validate callback"
	);
}

/// The hand-authored Fixture, guarded rather harder than the harvested one — for the reason
/// ADR-0016 gives, it has no upstream snapshot behind it.
///
/// The recording is made by pinning `process.stdout.columns`, which is a thing done to the
/// environment rather than observed in it, and which nothing downstream would notice if it stopped
/// working: the Scenarios would quietly become seven more 80-column cases and go on passing. So the
/// check is that clack's own bytes show the width it was told about — no row of an opening Frame
/// wider than the Scenario says the terminal is.
#[test]
fn the_authored_fixture_records_the_widths_it_claims() {
	let (json, scenarios) = authored();

	assert_eq!(
		json["tag"].as_str(),
		Some(TAG),
		"the authored Fixture was recorded from a clack other than the harvested one, so the two \
		 are being compared side by side against different upstreams"
	);

	assert!(
		scenarios.len() >= 22,
		"the authored Fixture has shrunk to {} Scenarios",
		scenarios.len()
	);

	// Each of the three Prompts, because each has something a harvest of its own suite cannot
	// reach: a wrap for `text`, a `y` and a mismeasured prefix for `confirm`, an astral character
	// for `password`.
	for kind in ["text", "password", "confirm"] {
		assert!(
			scenarios.iter().any(|s| s.kind == kind),
			"the authored Fixture has no {kind} Scenario left in it"
		);
	}

	// The point of the whole Fixture: it has to reach widths the harvest cannot.
	let narrow = scenarios.iter().filter(|s| s.columns < 80).count();
	assert!(
		narrow >= 8,
		"only {narrow} authored Scenarios are narrower than the harvest's 80 columns, which is \
		 what this Fixture exists for"
	);

	// And the other thing upstream's tests never do. A Scenario that resizes is the only evidence
	// there is for the `restoreCursor` behaviour ADR-0017 records, so losing them would leave that
	// behaviour asserted by nothing but the unit test that was written from it.
	let resizing = scenarios.iter().filter(|s| s.resizes()).count();
	assert!(
		resizing >= 4,
		"only {resizing} authored Scenarios resize the terminal"
	);

	for scenario in &scenarios {
		assert!(
			scenario.is_replayable(),
			"{}: not replayable",
			scenario.name
		);

		let opening = opening_frame(&scenario.output)
			.unwrap_or_else(|| panic!("{}: no opening Frame", scenario.name));

		for line in strip_sgr(opening).lines() {
			assert!(
				clackatui_core::width::width(line) <= scenario.columns,
				"{}: clack wrote a {}-column row into a {}-column terminal, so the recording was \
				 not made at the width it claims: {}",
				scenario.name,
				clackatui_core::width::width(line),
				scenario.columns,
				readable(line),
			);
		}
	}

	// And at least one of them has to have actually wrapped, or the widths above are satisfied by
	// Scenarios that were simply too short to reach the edge.
	let wrapped = scenarios.iter().any(|scenario| {
		opening_frame(&scenario.output).is_some_and(|opening| {
			strip_sgr(opening)
				.lines()
				.any(|line| clackatui_core::width::width(line) > scenario.columns - 4)
		})
	});
	assert!(
		wrapped,
		"no authored Scenario has a row anywhere near its terminal's width, so none of them \
		 exercises the wrap this Fixture exists to test"
	);
}
