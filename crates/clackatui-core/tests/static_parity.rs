//! Parity for the renderers that are not Prompts: `log`, `intro`, `outro`, `cancel`, `note` and
//! `box`.
//!
//! The same claim `scenario_parity` makes, against a corpus that needs no Scenario. A static
//! renderer has no state, reads no key and draws no second Frame — it is a function from a message
//! to a string of bytes — so a case is one call, and both sides are compared after one write.
//!
//! Two comparisons per case, and they check different things:
//!
//! - **The Grid.** Characters, styles and cursor position, through one emulator, exactly as
//!   ADR-0001 defines it. This is the one that matters, and the only one that can see colour.
//! - **The characters, with SGR stripped.** The Grid of an 80-column terminal cannot tell a row
//!   that ends in two spaces from one that ends where the padding begins — and one of these
//!   renderers writes those two spaces whether or not there is a message behind them. Trailing
//!   space is invisible on a Grid and visible to anyone who selects the row, so it gets a
//!   comparison of its own.
//!
//! # Only the two that draw a right-hand border wrap
//!
//! clack wraps a Prompt's Frame itself, because it has to count the rows it walks the cursor back
//! over (ADR-0012). Nothing walks back over these, so `log`, `intro`, `outro` and `cancel` send a
//! long line out whole and let the terminal break it — which is why three cases in the corpus are
//! longer than the terminal they are written to. If [`write_once`] ever started wrapping, those
//! three are what would say so. `note` and `box` are the exceptions and for the same reason: a
//! border only lands in the right column if what is inside it was measured first, so both of them
//! wrap the message themselves before they draw it (ADR-0030, ADR-0031).

mod grid;
use grid::{Grid, difference};

use clackatui_core::r#box;
use clackatui_core::emitter::write_once;
use clackatui_core::frame::{Frame, Line, Span};
use clackatui_core::message;
use clackatui_core::note;
use clackatui_core::theme::Theme;
use ratatui_core::style::{Color, Style};
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/static.json");

struct Case {
	name: String,
	kind: String,
	message: String,
	title: String,
	format: Option<String>,
	symbol: Option<String>,
	secondary_symbol: Option<String>,
	spacing: usize,
	with_guide: bool,
	columns: usize,
	rows: usize,
	bytes: String,
	/// Everything else the case passed, read where it is needed. `box` alone has seven options, and
	/// a field apiece on this struct for renderers that never look at them is worse than the lookup.
	options: Value,
}

fn cases() -> (Value, Vec<Case>) {
	let json: Value = serde_json::from_str(FIXTURE).expect("fixtures/static.json parses");
	let cases = json["cases"]
		.as_array()
		.expect("the fixture carries cases")
		.iter()
		.map(|case| {
			let options = &case["options"];
			Case {
				name: case["name"].as_str().expect("a name").to_owned(),
				kind: case["kind"].as_str().expect("a kind").to_owned(),
				message: case["message"].as_str().expect("a message").to_owned(),
				title: case["title"].as_str().unwrap_or_default().to_owned(),
				format: options["format"].as_str().map(str::to_owned),
				symbol: options["symbol"].as_str().map(str::to_owned),
				secondary_symbol: options["secondarySymbol"].as_str().map(str::to_owned),
				// Upstream's defaults, which a case only writes down when it differs from them.
				spacing: options["spacing"].as_u64().unwrap_or(1) as usize,
				with_guide: options["withGuide"].as_bool().unwrap_or(true),
				columns: case["columns"].as_u64().expect("a width") as usize,
				rows: case["rows"].as_u64().expect("a height") as usize,
				bytes: case["bytes"]
					.as_str()
					.expect("the bytes clack wrote")
					.to_owned(),
				options: options.clone(),
			}
		})
		.collect();
	(json, cases)
}

/// The Frame the port draws for a case.
fn drawn(case: &Case) -> Frame {
	let theme = Theme::clack();
	let bar = Span::styled(theme.symbols.bar, theme.styles.guide);

	// The five named `log` helpers are `log.message` with one option set, which is what upstream's
	// are too — each is a one-line call through to it.
	let symbol = match case.kind.as_str() {
		"log.info" => Span::styled(theme.symbols.info, theme.styles.log_info),
		"log.success" => Span::styled(theme.symbols.success, theme.styles.log_success),
		"log.step" => Span::styled(theme.symbols.step_submit, theme.styles.log_step),
		"log.warn" => Span::styled(theme.symbols.warn, theme.styles.log_warn),
		"log.error" => Span::styled(theme.symbols.error, theme.styles.log_error),
		_ => case
			.symbol
			.as_deref()
			.map(Span::raw)
			.unwrap_or_else(|| bar.clone()),
	};
	let secondary = case
		.secondary_symbol
		.as_deref()
		.map(Span::raw)
		.unwrap_or_else(|| bar.clone());

	match case.kind.as_str() {
		"note" => match case.format.as_deref() {
			None => note::note(
				&case.message,
				&case.title,
				case.columns,
				&theme,
				case.with_guide,
			),
			Some(named) => note::note_with(
				&case.message,
				&case.title,
				case.columns,
				&theme,
				case.with_guide,
				formatter(named),
			),
		},
		"box" => r#box::box_with(
			&case.message,
			&case.title,
			case.columns,
			&theme,
			&box_options(case),
		),
		"intro" => message::intro(&case.message, &theme, case.with_guide),
		"outro" => message::outro(&case.message, &theme, case.with_guide),
		"cancel" => message::cancel(&case.message, &theme, case.with_guide),
		_ => message::log(
			&case.message,
			symbol,
			secondary,
			case.spacing,
			case.with_guide,
		),
	}
}

/// A `box` case's options, with upstream's defaults for whatever it did not set.
fn box_options(case: &Case) -> r#box::Options<'static> {
	let options = &case.options;
	let align = |key: &str| match options[key].as_str() {
		Some("center") => r#box::Align::Center,
		Some("right") => r#box::Align::Right,
		// Upstream's `getPaddingForLine` treats every value that is not `center` or `right` as left,
		// including `undefined`, so this is the default and the fallback at once.
		_ => r#box::Align::Left,
	};
	let defaults = r#box::Options::default();
	r#box::Options {
		content_align: align("contentAlign"),
		title_align: align("titleAlign"),
		// Three states, not two: a number is a fraction, the string `'auto'` shrinks to the content,
		// and an absent `width` is neither — it fills the terminal. See `clackatui_core::box`.
		width: match &options["width"] {
			Value::Number(fraction) => {
				r#box::Width::Fraction(fraction.as_f64().expect("a JSON number is an f64"))
			}
			Value::String(auto) if auto == "auto" => r#box::Width::Auto,
			_ => r#box::Width::Full,
		},
		title_padding: options["titlePadding"]
			.as_u64()
			.map_or(defaults.title_padding, |padding| padding as usize),
		content_padding: options["contentPadding"]
			.as_u64()
			.map_or(defaults.content_padding, |padding| padding as usize),
		rounded: options["rounded"].as_bool().unwrap_or(defaults.rounded),
		with_guide: case.with_guide,
		format_border: options["formatBorder"]
			.as_str()
			.map_or(defaults.format_border, formatter),
	}
}

/// The formatter a `note` case names, as a Line-returning one rather than upstream's string one.
///
/// Same three the Recorder holds, and the reason the Fixture names them instead of carrying them: a
/// function does not survive JSON. `red` adds no columns and `stars` adds four, which is the whole
/// point of `wrapWithFormat` — and `red-stars` adds both at once.
fn formatter(named: &str) -> &'static dyn Fn(&str) -> Line {
	match named {
		"stars" => &|line| Line::from(Span::raw(format!("* {line} *"))),
		"red" => &|line| Line::from(Span::styled(line, Style::new().fg(Color::Red))),
		"red-stars" => &|line| {
			Line::from_iter([
				Span::styled("* ", Style::new().fg(Color::Red)),
				Span::styled(line, Style::new().fg(Color::Cyan)),
				Span::styled(" *", Style::new().fg(Color::Red)),
			])
		},
		other => panic!("the fixture names a formatter the test does not have: {other}"),
	}
}

/// The one that matters.
#[test]
fn every_static_renderer_leaves_the_terminal_the_way_clack_left_it() {
	let (_, cases) = cases();
	let mut failures = Vec::new();

	for case in &cases {
		let theirs = Grid::of(&case.bytes, case.columns, case.rows);
		let ours = Grid::of(&write_once(&drawn(case)), case.columns, case.rows);

		// Two blank terminals are equal, so a case whose bytes never reached the emulator would
		// agree for free.
		assert!(
			!theirs.text().trim().is_empty() || case.message.is_empty(),
			"{}: clack's stream left nothing on the terminal",
			case.name
		);

		if ours != theirs {
			failures.push(format!("  {}\n{}", case.name, difference(&theirs, &ours)));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {} static renders leave the terminal in a different state.\n\n{}",
		failures.len(),
		cases.len(),
		failures.join("\n"),
	);

	assert!(
		cases.len() >= 87,
		"only {} cases were compared; the fixture has stopped carrying them",
		cases.len()
	);
}

/// What a Grid cannot see: where a row actually ends.
///
/// `outro('')` writes the corner, two spaces, and nothing else. On a Grid those two spaces are
/// indistinguishable from the blanks a terminal pads every row with, so only the characters can say
/// whether the port wrote them. The styles are stripped because the two sides encode them
/// differently on purpose (ADR-0011) — that difference is the Grid's business, above.
#[test]
fn every_static_render_is_written_the_way_clack_wrote_it() {
	let (_, cases) = cases();
	let mut failures = Vec::new();

	for case in &cases {
		let theirs = strip(&case.bytes);
		let ours = strip(&write_once(&drawn(case)));
		if theirs != ours {
			failures.push(format!(
				"  {}\n       clack: {theirs:?}\n        port: {ours:?}",
				case.name
			));
		}
	}

	assert!(
		failures.is_empty(),
		"{} of {} static renders put different characters on the wire.\n\n{}",
		failures.len(),
		cases.len(),
		failures.join("\n"),
	);
}

/// The fixture is a recording of one clack, and says which.
#[test]
fn the_static_fixture_is_a_plausible_recording() {
	let (json, cases) = cases();

	assert_eq!(json["tag"], "@clack/prompts@1.7.0");
	assert_eq!(json["generatedBy"], "scripts/harvest-static.mjs");

	for kind in [
		"log",
		"log.info",
		"log.success",
		"log.step",
		"log.warn",
		"log.error",
		"intro",
		"outro",
		"cancel",
		"note",
		"box",
	] {
		assert!(
			cases.iter().any(|case| case.kind == kind),
			"nothing in the fixture records {kind}"
		);
	}

	// The three claims the corpus exists to make: a message wider than the terminal, a Guide turned
	// off, and a symbol that is not the bar.
	assert!(
		cases
			.iter()
			.any(|case| case.message.chars().count() > case.columns),
		"nothing in the fixture is longer than the terminal it was written to"
	);
	assert!(
		cases.iter().any(|case| !case.with_guide),
		"nothing in the fixture turns the Guide off"
	);
	assert!(
		cases.iter().any(|case| case.symbol.is_some()),
		"nothing in the fixture sets a symbol of its own"
	);
	// `note` is the only renderer here that reads the terminal's width, so a case at a width other
	// than eighty is the only thing that can catch it reading the wrong one.
	assert!(
		cases
			.iter()
			.any(|case| case.kind == "note" && case.columns != 80),
		"every note in the fixture was written to the same terminal"
	);
	assert!(
		cases.iter().any(|case| case.format.is_some()),
		"nothing in the fixture formats a note's rows"
	);
	// A `box` reads three things a `note` does not, and each has a case that would go unnoticed if the
	// fixture stopped carrying it: the width it was asked for, the corners, and the border formatter.
	assert!(
		cases
			.iter()
			.any(|case| case.kind == "box" && case.options["width"] == "auto"),
		"nothing in the fixture asks a box to fit its content"
	);
	assert!(
		cases
			.iter()
			.any(|case| case.options["rounded"].as_bool() == Some(true)),
		"nothing in the fixture asks a box for rounded corners"
	);
	assert!(
		cases
			.iter()
			.any(|case| case.options["formatBorder"].is_string()),
		"nothing in the fixture formats a box's border"
	);
}

/// Bytes with their SGR sequences taken out. Only SGR — nothing here writes any other escape.
fn strip(bytes: &str) -> String {
	let mut out = String::new();
	let mut chars = bytes.chars();
	while let Some(c) = chars.next() {
		if c == '\u{1b}' {
			for c in chars.by_ref() {
				if c == 'm' {
					break;
				}
			}
		} else {
			out.push(c);
		}
	}
	out
}
