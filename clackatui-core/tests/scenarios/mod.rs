//! The harvested `text` Scenarios, read back out of the Fixture.
//!
//! Shared by `scenario_replay.rs` and `scenario_parity.rs` rather than duplicated: the two ask
//! different questions of the same recording, and a Scenario that one of them reads differently
//! from the other is a bug waiting to be blamed on the port.
//!
//! Nothing here interprets output. A [`Scenario`] is upstream's input — a configuration, a sequence
//! of keys, a terminal size — plus the bytes clack wrote back, verbatim, exactly as CONTEXT.md
//! defines a Fixture. What those bytes *mean* is decided on the Rust side, by whichever test is
//! asking.

#![allow(dead_code)]

use clackatui_core::line_editor::{Key, KeyName};
use clackatui_core::prompt::Prompt;
use clackatui_core::session::Session;
use clackatui_core::text::{TextState, TextWidget};

const FIXTURE: &str = include_str!("../fixtures/scenarios/text.json");

/// The tag `README.md` names. A fixture from anywhere else is not the thing we claim parity with.
pub const TAG: &str = "@clack/prompts@1.7.0";

pub struct Scenario {
	pub name: String,
	pub kind: String,
	pub message: String,
	pub placeholder: Option<String>,
	pub default_value: Option<String>,
	pub initial_value: Option<String>,
	/// `opts.withGuide`, which falls back to `settings.withGuide` when absent.
	pub with_guide: bool,
	/// The terminal clack wrapped its Frames to.
	pub columns: usize,
	/// Its height, which is a separate number for the reason `Emitter::frame` gives.
	pub rows: usize,
	/// Upstream passed a `validate` callback, which a recording cannot carry across.
	pub validates: bool,
	pub keys: Vec<Recorded>,
	pub output: Vec<String>,
}

pub struct Recorded {
	pub s: Option<String>,
	pub name: Option<String>,
	pub ctrl: bool,
	pub meta: bool,
	pub shift: bool,
	pub sequence: Option<String>,
}

pub fn fixture() -> (serde_json::Value, Vec<Scenario>) {
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
				columns: run["terminal"]["columns"].as_u64().unwrap_or(80) as usize,
				rows: run["terminal"]["rows"].as_u64().unwrap_or(20) as usize,
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

	/// The Prompt this Scenario configures, before anything is asked of it.
	pub fn prompt(&self) -> Prompt<TextState> {
		let mut state = TextState::new();
		if let Some(default) = &self.default_value {
			state = state.with_default_value(default);
		}
		let mut prompt = Prompt::new(state);
		if let Some(initial) = &self.initial_value {
			prompt = prompt.with_initial_user_input(initial);
		}
		prompt
	}

	/// The widget upstream's `text()` would have built, drawing the Prompt it is handed.
	pub fn widget<'a>(&'a self, prompt: &'a Prompt<TextState>) -> TextWidget<'a> {
		let mut widget = TextWidget::new(prompt, &self.message).with_guide(self.with_guide);
		if let Some(placeholder) = &self.placeholder {
			widget = widget.with_placeholder(placeholder);
		}
		widget
	}

	/// The whole Scenario as a Session: the Prompt, the widget, and clack's terminal size.
	pub fn session(&self) -> Session<TextState> {
		let message = self.message.clone();
		let placeholder = self.placeholder.clone();
		let with_guide = self.with_guide;

		Session::new(self.prompt(), move |prompt| {
			let mut widget = TextWidget::new(prompt, &message).with_guide(with_guide);
			if let Some(placeholder) = &placeholder {
				widget = widget.with_placeholder(placeholder);
			}
			widget.frame()
		})
		.with_size(self.columns as u16, self.rows as u16)
	}

	/// Every byte the port writes for this Scenario, from the opening Frame to the closing cursor.
	pub fn replay(&self) -> String {
		let mut session = self.session();
		let mut out = session.open();
		for recorded in &self.keys {
			out.push_str(&session.key(recorded.s.as_deref(), &recorded.key()));
		}
		out
	}

	/// Every byte clack wrote for it. The chunks are one `output.write` call each; a terminal sees
	/// them as one stream, so that is how they are handed on.
	pub fn recorded(&self) -> String {
		self.output.concat()
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
