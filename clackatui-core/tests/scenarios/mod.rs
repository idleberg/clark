//! The Scenarios, read back out of the Fixtures.
//!
//! Shared by `scenario_replay.rs` and `scenario_parity.rs` rather than duplicated: the two ask
//! different questions of the same recording, and a Scenario that one of them reads differently
//! from the other is a bug waiting to be blamed on the port.
//!
//! Nothing here interprets output. A [`Scenario`] is upstream's input — a configuration, a sequence
//! of keys, a terminal size — plus the bytes clack wrote back, verbatim, exactly as CONTEXT.md
//! defines a Fixture. What those bytes *mean* is decided on the Rust side, by whichever test is
//! asking.
//!
//! # Nine Fixtures, one shape
//!
//! [`harvested`], [`password`], [`confirm`], [`select`], [`multi_select`], [`select_key`],
//! [`group_multi_select`] and [`autocomplete`] are
//! clack's own test suite, one file per suite — and one suite covers two Prompts,
//! recorded while the suite passed its own snapshots. [`authored`] is `scripts/authored/cases.mjs`,
//! written because upstream's tests never vary the terminal and so can say nothing about wrapping.
//! They are recorded by different scripts and carry different evidence behind them — ADR-0016 — but
//! they are the same shape and are read the same way, and [`all`] is what the tests run over.
//!
//! # Nine kinds of Prompt
//!
//! A Scenario names the Prompt it configures, and [`Run`] is all of them behind one door.
//! Nothing above this module branches on the kind: a Scenario opens, takes keys, resizes and
//! settles, and which widget is doing the drawing is the loader's business.

#![allow(dead_code)]

use clackatui_core::autocomplete::required as autocomplete_required;
use clackatui_core::autocomplete::{
	AutocompleteMultiSelectWidget, AutocompleteState, AutocompleteWidget,
};
use clackatui_core::confirm::{ConfirmState, ConfirmWidget};
use clackatui_core::date::{Date, DateFormat, DateState, DateWidget, validator as date_validator};
use clackatui_core::frame::Frame;
use clackatui_core::group_multi_select::{GroupMultiSelectState, GroupMultiSelectWidget};
use clackatui_core::line_editor::{Key, KeyName};
use clackatui_core::multi_line::{MultiLineState, MultiLineWidget};
use clackatui_core::multi_select::{MultiSelectState, MultiSelectWidget, required};
use clackatui_core::password::{PasswordState, PasswordWidget};
use clackatui_core::prompt::{Prompt, Status};
use clackatui_core::select::{SelectOption, SelectState, SelectWidget};
use clackatui_core::select_key::{SelectKeyState, SelectKeyWidget};
use clackatui_core::session::Session;
use clackatui_core::text::{TextState, TextWidget};

const HARVESTED: &str = include_str!("../fixtures/scenarios/text.json");
const AUTHORED: &str = include_str!("../fixtures/scenarios/authored.json");
const PASSWORD: &str = include_str!("../fixtures/scenarios/password.json");
const CONFIRM: &str = include_str!("../fixtures/scenarios/confirm.json");
const SELECT: &str = include_str!("../fixtures/scenarios/select.json");
const MULTI_SELECT: &str = include_str!("../fixtures/scenarios/multi-select.json");
const SELECT_KEY: &str = include_str!("../fixtures/scenarios/select-key.json");
const GROUP_MULTI_SELECT: &str = include_str!("../fixtures/scenarios/group-multi-select.json");
const AUTOCOMPLETE: &str = include_str!("../fixtures/scenarios/autocomplete.json");
const DATE: &str = include_str!("../fixtures/scenarios/date.json");
const MULTI_LINE: &str = include_str!("../fixtures/scenarios/multi-line.json");

/// The tag `README.md` names. A fixture from anywhere else is not the thing we claim parity with.
pub const TAG: &str = "@clack/prompts@1.7.0";

pub struct Scenario {
	pub name: String,
	pub kind: String,
	pub message: String,
	pub placeholder: Option<String>,
	pub default_value: Option<String>,
	pub initial_value: Option<String>,
	/// `initialValue` again, for the Prompt whose values are booleans.
	pub initial_flag: Option<bool>,
	/// `password`'s `mask` and `clearOnError`.
	pub mask: Option<String>,
	pub clear_on_error: bool,
	/// `confirm`'s two labels and its layout.
	pub active: Option<String>,
	pub inactive: Option<String>,
	pub vertical: bool,
	/// `select`'s list, and the two options that decide how much of it is drawn.
	pub options: Vec<Choice>,
	/// `groupMultiselect`'s list, which is a map rather than an array — the group's name, and what
	/// was listed under it, in the order they were written.
	pub groups: Vec<(String, Vec<Choice>)>,
	/// Its `selectableGroups` and `groupSpacing`.
	pub selectable_groups: bool,
	pub group_spacing: isize,
	pub max_items: Option<usize>,
	pub show_instructions: bool,
	/// `selectKey`'s `caseSensitive`.
	pub case_sensitive: bool,
	/// `opts.required` as the two `autocomplete` Prompts read it: off unless asked for, where
	/// `multiselect`'s is on unless refused. The same word, the opposite default, so it cannot be the
	/// same field — [`required`](Self::required) cannot tell an absent option from a `true` one.
	pub required_explicit: bool,
	/// Upstream passed a `filter` callback. One shape of them, in three Scenarios — see
	/// [`Scenario::filter`].
	pub filters: bool,
	/// `date`'s `format`, `locale` and `separator` — the three that decide which segments are drawn
	/// and in what order. See [`Scenario::date_format`], which is where a locale is turned into the
	/// only thing a Rust port can act on.
	pub format: Option<String>,
	pub locale: Option<String>,
	pub separator: Option<String>,
	/// Its four date-valued options, each recorded as the instant it held. `initialValue` and
	/// `defaultValue` are separate fields here because the Prompt reads them separately, even though
	/// the constructor coalesces them.
	pub initial_date: Option<Date>,
	pub default_date: Option<Date>,
	pub min_date: Option<Date>,
	pub max_date: Option<Date>,
	/// `multiselect`'s `initialValues`, `cursorAt` and `required`.
	pub initial_values: Vec<String>,
	pub cursor_at: Option<String>,
	pub required: bool,
	/// `multiline`'s `showSubmit`: a `[ submit ]` button, and a `return` that is only ever a newline.
	pub show_submit: bool,
	/// `opts.withGuide`, which falls back to `settings.withGuide` when absent.
	pub with_guide: bool,
	/// The width clack wrapped its Frames to — `process.stdout.columns`, recorded as
	/// `terminal.stdout`, not the `columns` the Prompt's own stream reports. `Prompt.render` reads
	/// the global and ignores the stream, so the stream's number is the wrong one to wrap by; a
	/// Fixture recorded before that was noticed falls back to it rather than guessing.
	pub columns: usize,
	/// The width the Prompt's *own* stream reports — `getColumns(opts.output)`, which is what a
	/// widget measures its message and its option list against.
	///
	/// The same terminal as [`columns`](Self::columns) outside a harness, and the same number in
	/// every Fixture up to `confirm`. `select`'s suite is the first to set them apart, by handing the
	/// Prompt a 30- or 40-column stream while `process.stdout` stays at 80 — so a Scenario that does
	/// that is a Scenario whose Frames are wrapped at one width and written at another.
	pub stream_columns: usize,
	/// Its height, which is a separate number for the reason `Emitter::frame` gives — that one
	/// *does* come from the Prompt's own stream.
	pub rows: usize,
	/// Upstream passed a `validate` callback, which a recording cannot carry across.
	pub validates: bool,
	/// The keypresses on their own, for everything that drives a bare [`Prompt`] and has no notion
	/// of a terminal. A Scenario that resizes says so in [`Scenario::events`] instead.
	pub keys: Vec<Recorded>,
	/// Everything that happened to the Prompt, in order.
	pub events: Vec<Event>,
	pub output: Vec<String>,
}

/// One thing that happened to an open Prompt.
pub enum Event {
	Key(Recorded),
	/// The terminal changed size, `at` chunks into what clack wrote.
	///
	/// The position is what a keypress does not need. Anything replaying the recording has to
	/// change the terminal it is replaying *into* at the same point clack's did, or the two streams
	/// are being read at different widths and the comparison means nothing.
	Resize {
		columns: usize,
		rows: usize,
		at: usize,
	},
}

/// A run of bytes and the terminal they were written into.
pub struct Segment {
	pub bytes: String,
	pub columns: usize,
	pub rows: usize,
}

/// One entry of a `select`'s `options` array, as a recording carries it.
pub struct Choice {
	pub value: String,
	pub label: Option<String>,
	pub hint: Option<String>,
	pub disabled: bool,
}

pub struct Recorded {
	pub s: Option<String>,
	pub name: Option<String>,
	pub ctrl: bool,
	pub meta: bool,
	pub shift: bool,
	pub sequence: Option<String>,
}

/// Clack's own `text` suite, recorded while it passed its own snapshots.
pub fn harvested() -> (serde_json::Value, Vec<Scenario>) {
	parse(HARVESTED, "fixtures/scenarios/text.json")
}

/// The hand-authored cases: the widths upstream never varies. See ADR-0016.
pub fn authored() -> (serde_json::Value, Vec<Scenario>) {
	parse(AUTHORED, "fixtures/scenarios/authored.json")
}

/// Clack's own `password` suite.
pub fn password() -> (serde_json::Value, Vec<Scenario>) {
	parse(PASSWORD, "fixtures/scenarios/password.json")
}

/// Clack's own `confirm` suite.
pub fn confirm() -> (serde_json::Value, Vec<Scenario>) {
	parse(CONFIRM, "fixtures/scenarios/confirm.json")
}

/// Clack's own `select` suite.
pub fn select() -> (serde_json::Value, Vec<Scenario>) {
	parse(SELECT, "fixtures/scenarios/select.json")
}

/// Clack's own `multiselect` suite.
pub fn multi_select() -> (serde_json::Value, Vec<Scenario>) {
	parse(MULTI_SELECT, "fixtures/scenarios/multi-select.json")
}

/// Clack's own `selectKey` suite.
pub fn select_key() -> (serde_json::Value, Vec<Scenario>) {
	parse(SELECT_KEY, "fixtures/scenarios/select-key.json")
}

/// Clack's own `groupMultiselect` suite.
pub fn group_multi_select() -> (serde_json::Value, Vec<Scenario>) {
	parse(
		GROUP_MULTI_SELECT,
		"fixtures/scenarios/group-multi-select.json",
	)
}

/// Clack's own `autocomplete` suite, which covers both `autocomplete` and `autocompleteMultiselect`.
pub fn autocomplete() -> (serde_json::Value, Vec<Scenario>) {
	parse(AUTOCOMPLETE, "fixtures/scenarios/autocomplete.json")
}

/// Clack's own `date` suite.
pub fn date() -> (serde_json::Value, Vec<Scenario>) {
	parse(DATE, "fixtures/scenarios/date.json")
}

/// Clack's own `multiline` suite.
pub fn multi_line() -> (serde_json::Value, Vec<Scenario>) {
	parse(MULTI_LINE, "fixtures/scenarios/multi-line.json")
}

/// Every Scenario there is. Which Fixture one came from is a question about the evidence behind it,
/// not about what the port owes it, so the tests do not ask.
pub fn all() -> Vec<Scenario> {
	let (_, mut scenarios) = harvested();
	scenarios.extend(authored().1);
	scenarios.extend(password().1);
	scenarios.extend(confirm().1);
	scenarios.extend(select().1);
	scenarios.extend(multi_select().1);
	scenarios.extend(select_key().1);
	scenarios.extend(group_multi_select().1);
	scenarios.extend(autocomplete().1);
	scenarios.extend(date().1);
	scenarios.extend(multi_line().1);
	scenarios
}

fn parse(source: &str, path: &str) -> (serde_json::Value, Vec<Scenario>) {
	let json: serde_json::Value =
		serde_json::from_str(source).unwrap_or_else(|_| panic!("{path} parses"));

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
				initial_flag: opts["initialValue"].as_bool(),
				mask: opts["mask"].as_str().map(str::to_owned),
				clear_on_error: opts["clearOnError"].as_bool() == Some(true),
				active: opts["active"].as_str().map(str::to_owned),
				inactive: opts["inactive"].as_str().map(str::to_owned),
				vertical: opts["vertical"].as_bool() == Some(true),
				options: opts["options"]
					.as_array()
					.map(|options| options.iter().map(choice).collect())
					.unwrap_or_default(),
				groups: opts["options"]
					.as_object()
					.map(|groups| {
						groups
							.iter()
							.map(|(name, options)| {
								let options = options
									.as_array()
									.expect("a group holds an array of options")
									.iter()
									.map(choice)
									.collect();
								(name.clone(), options)
							})
							.collect()
					})
					.unwrap_or_default(),
				// `opts.selectableGroups !== false`: the same default-on shape as `required`, and carried the
				// same way.
				selectable_groups: opts["selectableGroups"].as_bool().unwrap_or(true),
				group_spacing: opts["groupSpacing"].as_i64().unwrap_or(0) as isize,
				max_items: opts["maxItems"].as_u64().map(|n| n as usize),
				case_sensitive: opts["caseSensitive"].as_bool() == Some(true),
				show_instructions: opts["showInstructions"].as_bool().unwrap_or(true),
				initial_values: opts["initialValues"]
					.as_array()
					.map(|values| {
						values
							.iter()
							.map(|value| {
								value
									.as_str()
									.expect("an initial value is a string")
									.to_owned()
							})
							.collect()
					})
					.unwrap_or_default(),
				cursor_at: opts["cursorAt"].as_str().map(str::to_owned),
				show_submit: opts["showSubmit"].as_bool() == Some(true),
				format: opts["format"].as_str().map(str::to_owned),
				locale: opts["locale"].as_str().map(str::to_owned),
				separator: opts["separator"].as_str().map(str::to_owned),
				initial_date: recorded_date(&opts["initialValue"]),
				default_date: recorded_date(&opts["defaultValue"]),
				min_date: recorded_date(&opts["minDate"]),
				max_date: recorded_date(&opts["maxDate"]),
				required_explicit: opts["required"].as_bool() == Some(true),
				filters: opts["filter"]["callback"].as_bool() == Some(true),
				// `opts.required ?? true`. The one option in any Fixture whose default is on, and so the
				// one a Scenario carries as an absence rather than as a value.
				required: opts["required"].as_bool().unwrap_or(true),
				with_guide: opts["withGuide"]
					.as_bool()
					.or_else(|| run["settings"]["withGuide"].as_bool())
					.unwrap_or(true),
				columns: run["terminal"]["stdout"]
					.as_u64()
					.or_else(|| run["terminal"]["columns"].as_u64())
					.unwrap_or(80) as usize,
				stream_columns: run["terminal"]["columns"]
					.as_u64()
					.or_else(|| run["terminal"]["stdout"].as_u64())
					.unwrap_or(80) as usize,
				rows: run["terminal"]["rows"].as_u64().unwrap_or(20) as usize,
				validates: opts["validate"]["callback"].as_bool() == Some(true),
				keys: keys(run),
				// A Fixture whose Recorder predates resizes says nothing about them, and a Scenario
				// that never resized is its keys in order — so the one reads as the other rather
				// than as a missing field.
				events: match run["events"].as_array() {
					Some(events) => events.iter().map(event).collect(),
					None => keys(run).into_iter().map(Event::Key).collect(),
				},
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

fn keys(run: &serde_json::Value) -> Vec<Recorded> {
	run["keys"]
		.as_array()
		.expect("keys is an array")
		.iter()
		.map(keypress)
		.collect()
}

fn choice(option: &serde_json::Value) -> Choice {
	Choice {
		// Every recorded `select` option holds a string. One holding anything else would be a
		// Scenario this loader cannot build, and saying so here beats drawing the wrong list.
		// A `groupMultiselect` option whose value was a `Symbol` is recorded without one — see
		// `Scenario::group_choices`, which is where the empty string is given its meaning.
		value: option["value"].as_str().unwrap_or("").to_owned(),
		label: option["label"].as_str().map(str::to_owned),
		hint: option["hint"].as_str().map(str::to_owned),
		disabled: option["disabled"].as_bool() == Some(true),
	}
}

/// A date-valued option, which the Recorder writes as `{ "date": "<ISO instant>" }`.
///
/// Only the calendar part is read, because only the calendar part is what the Prompt does with one:
/// every date it touches goes through `getUTCFullYear` and its two neighbours.
fn recorded_date(value: &serde_json::Value) -> Option<Date> {
	let iso = value["date"].as_str()?;
	let mut parts = iso[..10].split('-').map(|n| n.parse().expect("a number"));
	let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
	Some(Date::new(year, month, day).unwrap_or_else(|| panic!("{iso} is a date clack could hold")))
}

fn keypress(key: &serde_json::Value) -> Recorded {
	Recorded {
		s: key["s"].as_str().map(str::to_owned),
		name: key["key"]["name"].as_str().map(str::to_owned),
		ctrl: key["key"]["ctrl"].as_bool() == Some(true),
		meta: key["key"]["meta"].as_bool() == Some(true),
		shift: key["key"]["shift"].as_bool() == Some(true),
		sequence: key["key"]["sequence"].as_str().map(str::to_owned),
	}
}

fn event(event: &serde_json::Value) -> Event {
	match event["kind"].as_str() {
		Some("resize") => Event::Resize {
			columns: event["columns"].as_u64().expect("a resize has a width") as usize,
			rows: event["rows"].as_u64().expect("a resize has a height") as usize,
			at: event["at"].as_u64().expect("a resize has a position") as usize,
		},
		Some("key") => Event::Key(keypress(event)),
		other => panic!("unknown event kind {other:?}"),
	}
}

impl Scenario {
	/// Whether this Scenario can be driven from its recording at all.
	///
	/// A `validate` callback does not survive a harvest — a recording carries the frames the
	/// predicate produced but not the predicate — so replaying without it would compare against
	/// frames the port was never asked to draw. And one Scenario is driven by an `AbortSignal`
	/// rather than by keys, which is a cancellation path with no keypress in it.
	pub fn is_replayable(&self) -> bool {
		!self.validates && !self.keys.is_empty()
	}

	/// The whole Scenario as a Session: the Prompt, the widget, and clack's terminal size.
	///
	/// Everything a Scenario configures is resolved here, once. A widget option that a Fixture does
	/// not carry — a `text`'s placeholder in a `confirm` recording — is simply absent, so the three
	/// arms read like the three builders in `@clack/prompts` do.
	/// The `options` array as the two list Prompts want it. Both read the same four fields, because
	/// `multi-select.ts` imports `Option<Value>` from `select.ts` rather than declaring one.
	fn choices(&self) -> Vec<SelectOption<String>> {
		self.options
			.iter()
			.map(|choice| {
				let mut option = match &choice.label {
					Some(label) => SelectOption::labelled(choice.value.clone(), label),
					None => SelectOption::new(choice.value.clone()),
				};
				if let Some(hint) = &choice.hint {
					option = option.with_hint(hint);
				}
				option.with_disabled(choice.disabled)
			})
			.collect()
	}

	/// The `options` map as a `groupMultiselect` wants it.
	///
	/// An option a recording carries no value for is given one nothing else can equal. Upstream's
	/// suite has one such case — two options whose values are `Symbol()`, which JSON cannot hold —
	/// and two distinct values impossible to type is exactly what a pair of Symbols is.
	fn group_choices(&self) -> Vec<(String, Vec<SelectOption<String>>)> {
		let mut at = 0;
		self.groups
			.iter()
			.map(|(name, options)| {
				let options = options
					.iter()
					.map(|choice| {
						at += 1;
						let value = if choice.value.is_empty() {
							format!("\u{0}{at}")
						} else {
							choice.value.clone()
						};
						let mut option = match &choice.label {
							Some(label) => SelectOption::labelled(value, label),
							None => SelectOption::new(value),
						};
						if let Some(hint) = &choice.hint {
							option = option.with_hint(hint);
						}
						option.with_disabled(choice.disabled)
					})
					.collect();
				(name.clone(), options)
			})
			.collect()
	}

	/// The one `filter` upstream's suite installs, for the three Scenarios that install it.
	///
	/// A callback cannot be recorded, so this is the `required` bargain again: a Scenario carries the
	/// fact that there was one and the loader supplies the shape all three of them have — a label
	/// that *starts with* the search rather than containing it. A newer tag that writes a different
	/// one does not fail here; it fails in parity, loudly, which is the right place for it.
	/// The segment order and the separator a `date` Scenario asks for.
	///
	/// `opts.format` says both directly. `opts.locale` says neither: upstream asks
	/// `Intl.DateTimeFormat` and Rust has no locale data, so the one locale upstream's suite uses is
	/// written down here and any other is refused rather than guessed at — the same bargain as the
	/// `filter` above, and it fails in the loader rather than in parity because a wrong segment order
	/// is not a wrong drawing, it is a different Prompt.
	fn date_format(&self) -> (DateFormat, String) {
		let separator = self.separator.clone();
		if let Some(format) = &self.format {
			let format = match format.as_str() {
				"YMD" => DateFormat::Ymd,
				"MDY" => DateFormat::Mdy,
				"DMY" => DateFormat::Dmy,
				other => panic!("{}: unknown date format {other}", self.name),
			};
			return (format, separator.unwrap_or_else(|| "/".into()));
		}
		match self.locale.as_deref() {
			Some("en-US") | None => (DateFormat::Mdy, separator.unwrap_or_else(|| "/".into())),
			Some(other) => panic!(
				"{}: no segment order is recorded for the locale {other}",
				self.name
			),
		}
	}

	fn filter(&self) -> Option<impl Fn(&str, &SelectOption<String>) -> bool + use<>> {
		self.filters
			.then_some(|search: &str, option: &SelectOption<String>| {
				option
					.label()
					.to_lowercase()
					.starts_with(&search.to_lowercase())
			})
	}

	/// The width a widget measures against, given the terminal a Frame is being drawn for.
	///
	/// `getColumns(opts.output)` is read on every render, so it follows a resize — where the Prompt's
	/// own stream *is* the terminal that resized, which outside a harness it always is. A Scenario
	/// that sets the two widths apart never resizes, and
	/// `the_two_widths_only_come_apart_where_nothing_resizes` is what says so — so one number or the
	/// other is the live one and this is where that is decided, once, for every widget that measures.
	fn stream_width(&self) -> impl Fn(u16) -> usize + use<> {
		let (stream, split) = (self.stream_columns, self.stream_columns != self.columns);
		move |columns| if split { stream } else { columns as usize }
	}

	pub fn run(&self) -> Run {
		let size = (self.columns as u16, self.rows as u16);
		let message = self.message.clone();
		let with_guide = self.with_guide;
		let width = self.stream_width();

		match self.kind.as_str() {
			"password" => {
				let state = PasswordState::new().with_clear_on_error(self.clear_on_error);
				let mask = self.mask.clone();
				Run::Password(
					Session::new(Prompt::new(state), move |prompt, _columns, _rows| {
						let mut widget =
							PasswordWidget::new(prompt, &message).with_guide(with_guide);
						if let Some(mask) = &mask {
							widget = widget.with_mask(mask);
						}
						widget.frame()
					})
					.with_size(size.0, size.1),
				)
			}

			"confirm" => {
				let mut state = ConfirmState::new();
				if let Some(initial) = self.initial_flag {
					state = state.with_initial_value(initial);
				}
				let (active, inactive) = (self.active.clone(), self.inactive.clone());
				let vertical = self.vertical;
				Run::Confirm(
					Session::new(Prompt::new(state), move |prompt, columns, _rows| {
						let mut widget = ConfirmWidget::new(prompt, &message)
							.with_guide(with_guide)
							.with_vertical(vertical)
							.with_columns(width(columns) as u16);
						if let Some(active) = &active {
							widget = widget.with_active(active);
						}
						if let Some(inactive) = &inactive {
							widget = widget.with_inactive(inactive);
						}
						widget.frame()
					})
					.with_size(size.0, size.1),
				)
			}

			"select" => {
				let mut state = SelectState::new(self.choices());
				if let Some(initial) = &self.initial_value {
					state = state.with_initial_value(initial);
				}
				let max_items = self.max_items;
				let show_instructions = self.show_instructions;
				Run::Select(
					// The width handed to the widget is the Prompt's own stream, not the Session's —
					// the two are different numbers in this suite. A resize would have to move both,
					// which is why `the_two_widths_only_come_apart_where_nothing_resizes` insists that
					// no Scenario asks for both at once.
					// The height, on the other hand, is the Session's: it is the stream's number on
					// both sides, and it is the one a resize moves.
					Session::new(Prompt::new(state), move |prompt, columns, rows| {
						let mut widget = SelectWidget::new(prompt, &message)
							.with_guide(with_guide)
							.with_columns(width(columns))
							.with_rows(rows as usize)
							.with_instructions(show_instructions);
						if let Some(max_items) = max_items {
							widget = widget.with_max_items(max_items);
						}
						widget.frame()
					})
					.with_size(size.0, size.1),
				)
			}

			"multiselect" => {
				let mut state = MultiSelectState::new(self.choices())
					.with_initial_values(self.initial_values.clone());
				if let Some(at) = &self.cursor_at {
					state = state.with_cursor_at(at);
				}
				let mut prompt = Prompt::new(state);
				// The one validator any Scenario installs. Upstream's is not a `validate` a recording
				// could carry — `multiselect()` writes it itself — so a Scenario reproduces the option
				// that turns it on rather than the callback it produced.
				if self.required {
					prompt = prompt.with_validator(required);
				}
				let max_items = self.max_items;
				let show_instructions = self.show_instructions;
				Run::MultiSelect(
					Session::new(prompt, move |prompt, columns, rows| {
						let mut widget = MultiSelectWidget::new(prompt, &message)
							.with_guide(with_guide)
							.with_columns(width(columns))
							.with_rows(rows as usize)
							.with_instructions(show_instructions);
						if let Some(max_items) = max_items {
							widget = widget.with_max_items(max_items);
						}
						widget.frame()
					})
					.with_size(size.0, size.1),
				)
			}

			"selectKey" => {
				let mut state =
					SelectKeyState::new(self.choices()).with_case_sensitive(self.case_sensitive);
				if let Some(initial) = &self.initial_value {
					state = state.with_initial_value(initial);
				}
				Run::SelectKey(
					// No height: a `selectKey` has no `limitOptions`, so it draws its whole list
					// however tall the terminal is.
					Session::new(Prompt::new(state), move |prompt, columns, _rows| {
						SelectKeyWidget::new(prompt, &message)
							.with_guide(with_guide)
							.with_columns(width(columns))
							.frame()
					})
					.with_size(size.0, size.1),
				)
			}

			"groupMultiselect" => {
				let mut state = GroupMultiSelectState::new(self.group_choices())
					.with_selectable_groups(self.selectable_groups)
					.with_initial_values(self.initial_values.clone());
				if let Some(at) = &self.cursor_at {
					state = state.with_cursor_at(at);
				}
				let mut prompt = Prompt::new(state);
				if self.required {
					prompt = prompt.with_validator(required);
				}
				let max_items = self.max_items;
				let show_instructions = self.show_instructions;
				let group_spacing = self.group_spacing;
				Run::GroupMultiSelect(
					Session::new(prompt, move |prompt, columns, rows| {
						let mut widget = GroupMultiSelectWidget::new(prompt, &message)
							.with_guide(with_guide)
							.with_columns(width(columns))
							.with_rows(rows as usize)
							.with_instructions(show_instructions)
							.with_group_spacing(group_spacing);
						if let Some(max_items) = max_items {
							widget = widget.with_max_items(max_items);
						}
						widget.frame()
					})
					.with_size(size.0, size.1),
				)
			}

			"date" => {
				let (format, separator) = self.date_format();
				let mut state = DateState::new(format, separator);
				// `opts.initialValue ?? opts.defaultValue` seeds the field, and `defaultValue` is also
				// the fallback — so the two are set in upstream's order and the state coalesces them.
				if let Some(default) = self.default_date {
					state = state.with_default_value(default);
				}
				if let Some(initial) = self.initial_date {
					state = state.with_initial_value(initial);
				}
				if let Some(min) = self.min_date {
					state = state.with_min_date(min);
				}
				if let Some(max) = self.max_date {
					state = state.with_max_date(max);
				}
				// `date()` writes this validator itself out of its own options, so a Scenario carries
				// the options rather than the callback — the `multiselect` bargain again.
				let prompt = Prompt::new(state).with_validator(date_validator(
					self.min_date,
					self.max_date,
					self.default_date,
					Default::default(),
					None,
				));
				Run::Date(
					// No width and no height: `date` measures nothing. Its only wrap is the outer one
					// in `Prompt.render`, which the Session applies.
					Session::new(prompt, move |prompt, _columns, _rows| {
						DateWidget::new(prompt, &message)
							.with_guide(with_guide)
							.frame()
					})
					.with_size(size.0, size.1),
				)
			}

			"multiline" => {
				let mut state = MultiLineState::new().with_show_submit(self.show_submit);
				if let Some(default) = &self.default_value {
					state = state.with_default_value(default);
				}
				let mut prompt = Prompt::new(state);
				// `initialValue` reaches the base class as `initialUserInput`, so it is typed into
				// the field rather than held as a fallback — the same route `text` takes.
				if let Some(initial) = &self.initial_value {
					prompt = prompt.with_initial_user_input(initial);
				}
				let placeholder = self.placeholder.clone();
				Run::MultiLine(
					// No height: `multiline` draws every row its text needs, however tall the
					// terminal is.
					Session::new(prompt, move |prompt, columns, _rows| {
						let mut widget = MultiLineWidget::new(prompt, &message)
							.with_guide(with_guide)
							.with_columns(width(columns));
						if let Some(placeholder) = &placeholder {
							widget = widget.with_placeholder(placeholder);
						}
						widget.frame()
					})
					.with_size(size.0, size.1),
				)
			}

			"autocomplete" | "autocompleteMultiselect" => {
				let multiple = self.kind == "autocompleteMultiselect";
				let mut state = match self.filter() {
					Some(filter) => AutocompleteState::with_filter(self.choices(), filter),
					None => AutocompleteState::new(self.choices()),
				}
				.with_multiple(multiple);
				if let Some(placeholder) = &self.placeholder {
					state = state.with_placeholder(placeholder);
				}
				// `initialValue` on one, `initialValues` on the other, and the same array underneath.
				if multiple {
					if !self.initial_values.is_empty() {
						state = state.with_initial_values(self.initial_values.clone());
					}
				} else if let Some(initial) = &self.initial_value {
					state = state.with_initial_values([initial.clone()]);
				}

				let mut prompt = Prompt::new(state);
				// `autocompleteMultiselect` writes this validator itself, out of an option that is off
				// until it is asked for — the other way round from `multiselect`'s.
				if multiple && self.required_explicit {
					prompt = prompt.with_validator(autocomplete_required);
				}

				let max_items = self.max_items;
				let placeholder = self.placeholder.clone();
				let session = Session::new(prompt, move |prompt, columns, rows| {
					if multiple {
						let mut widget = AutocompleteMultiSelectWidget::new(prompt, &message)
							.with_guide(with_guide)
							.with_columns(width(columns))
							.with_rows(rows as usize);
						if let Some(placeholder) = &placeholder {
							widget = widget.with_placeholder(placeholder);
						}
						if let Some(max_items) = max_items {
							widget = widget.with_max_items(max_items);
						}
						widget.frame()
					} else {
						let mut widget = AutocompleteWidget::new(prompt, &message)
							.with_guide(with_guide)
							.with_columns(width(columns))
							.with_rows(rows as usize);
						if let Some(placeholder) = &placeholder {
							widget = widget.with_placeholder(placeholder);
						}
						if let Some(max_items) = max_items {
							widget = widget.with_max_items(max_items);
						}
						widget.frame()
					}
				})
				.with_size(size.0, size.1);
				if multiple {
					Run::AutocompleteMultiSelect(session)
				} else {
					Run::Autocomplete(session)
				}
			}

			_ => {
				let mut state = TextState::new();
				if let Some(default) = &self.default_value {
					state = state.with_default_value(default);
				}
				let mut prompt = Prompt::new(state);
				if let Some(initial) = &self.initial_value {
					prompt = prompt.with_initial_user_input(initial);
				}
				let placeholder = self.placeholder.clone();
				Run::Text(
					Session::new(prompt, move |prompt, _columns, _rows| {
						let mut widget = TextWidget::new(prompt, &message).with_guide(with_guide);
						if let Some(placeholder) = &placeholder {
							widget = widget.with_placeholder(placeholder);
						}
						widget.frame()
					})
					.with_size(size.0, size.1),
				)
			}
		}
	}

	/// The Frame the port draws before any key arrives — the one clack writes whole rather than as
	/// a diff, and so the only one that can be held against a recording without an emulator.
	pub fn opening_frame(&self) -> Frame {
		self.run().frame()
	}

	/// The state the port settles in, having been fed every key this Scenario recorded.
	pub fn settles(&self) -> Status {
		let mut run = self.run();
		for recorded in &self.keys {
			run.key(recorded.s.as_deref(), &recorded.key());
		}
		run.status()
	}

	/// Every byte the port writes for this Scenario, from the opening Frame to the closing cursor.
	pub fn replay(&self) -> String {
		self.replayed()
			.into_iter()
			.map(|segment| segment.bytes)
			.collect()
	}

	/// Every byte clack wrote for it. The chunks are one `output.write` call each; a terminal sees
	/// them as one stream, so that is how they are handed on.
	pub fn recorded(&self) -> String {
		self.output.concat()
	}

	/// What the port writes, cut where the terminal changed size under it.
	pub fn replayed(&self) -> Vec<Segment> {
		let mut session = self.run();
		let mut segments = Vec::new();
		let (mut columns, mut rows) = (self.columns, self.rows);
		let mut bytes = session.open();

		for event in &self.events {
			match event {
				Event::Key(recorded) => {
					bytes.push_str(&session.key(recorded.s.as_deref(), &recorded.key()));
				}
				Event::Resize {
					columns: to,
					rows: high,
					..
				} => {
					// Everything so far belongs to the terminal it was written into; the resize's
					// own output belongs to the new one, as it does on clack's side.
					segments.push(Segment {
						bytes: std::mem::take(&mut bytes),
						columns,
						rows,
					});
					(columns, rows) = (*to, *high);
					bytes.push_str(&session.resize(*to as u16, *high as u16));
				}
			}
		}

		segments.push(Segment {
			bytes,
			columns,
			rows,
		});
		segments
	}

	/// What clack wrote, cut at the same points.
	///
	/// A resize records how many chunks had been written when it arrived, which is what makes the
	/// two sides splittable in the same places. Without that the recording would be one stream with
	/// no way to know which width any part of it was meant for.
	pub fn recorded_segments(&self) -> Vec<Segment> {
		let mut segments = Vec::new();
		let (mut columns, mut rows) = (self.columns, self.rows);
		let mut start = 0;

		for event in &self.events {
			let Event::Resize {
				columns: to,
				rows: high,
				at,
			} = event
			else {
				continue;
			};
			segments.push(Segment {
				bytes: self.output[start..*at].concat(),
				columns,
				rows,
			});
			(columns, rows, start) = (*to, *high, *at);
		}

		segments.push(Segment {
			bytes: self.output[start..].concat(),
			columns,
			rows,
		});
		segments
	}

	/// Whether the terminal ever changed size under this Prompt.
	pub fn resizes(&self) -> bool {
		self.events
			.iter()
			.any(|event| matches!(event, Event::Resize { .. }))
	}
}

/// A Scenario's Session, whichever Prompt it is for.
///
/// [`Session`] is generic over its state and the three states are different types, so something has
/// to name all three somewhere. Doing it here keeps the tests above written in terms of a Scenario
/// rather than in terms of a Prompt, which is what lets the same Grid comparison cover every kind.
pub enum Run {
	Text(Session<TextState>),
	Password(Session<PasswordState>),
	Confirm(Session<ConfirmState>),
	Select(Session<SelectState<String>>),
	MultiSelect(Session<MultiSelectState<String>>),
	SelectKey(Session<SelectKeyState<String>>),
	GroupMultiSelect(Session<GroupMultiSelectState<String>>),
	Autocomplete(Session<AutocompleteState<String>>),
	AutocompleteMultiSelect(Session<AutocompleteState<String>>),
	Date(Session<DateState>),
	MultiLine(Session<MultiLineState>),
}

/// The same call on whichever Session is inside. A macro because the arms differ only in the
/// variant name, and one copy of each method per Prompt would be one place per Prompt to forget one.
macro_rules! dispatch {
	($self:ident, $session:ident => $call:expr) => {
		match $self {
			Run::Text($session) => $call,
			Run::Password($session) => $call,
			Run::Confirm($session) => $call,
			Run::Select($session) => $call,
			Run::MultiSelect($session) => $call,
			Run::SelectKey($session) => $call,
			Run::GroupMultiSelect($session) => $call,
			Run::Autocomplete($session) => $call,
			Run::AutocompleteMultiSelect($session) => $call,
			Run::Date($session) => $call,
			Run::MultiLine($session) => $call,
		}
	};
}

impl Run {
	pub fn open(&mut self) -> String {
		dispatch!(self, session => session.open())
	}

	pub fn key(&mut self, s: Option<&str>, key: &Key) -> String {
		dispatch!(self, session => session.key(s, key))
	}

	pub fn resize(&mut self, columns: u16, rows: u16) -> String {
		dispatch!(self, session => session.resize(columns, rows))
	}

	pub fn status(&self) -> Status {
		dispatch!(self, session => session.status())
	}

	/// The Frame as it stands, drawn for the terminal the Session was given.
	pub fn frame(&self) -> Frame {
		dispatch!(self, session => session.frame())
	}
}

impl Recorded {
	/// A recorded keypress as the Line editor wants it.
	///
	/// The names come from readline, which is what the Scenarios record, so this is
	/// [`KeyName::readline_name`] read backwards. An unrecognised name is left as `None` rather than
	/// guessed at: the Prompt then sees a keypress with a character and no name, which is what
	/// readline does for a key it cannot name.
	pub fn key(&self) -> Key {
		Key {
			name: self.name.as_deref().and_then(key_name),
			ctrl: self.ctrl,
			meta: self.meta,
			shift: self.shift,
			sequence: self.sequence.clone(),
		}
	}
}

fn key_name(readline: &str) -> Option<KeyName> {
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
