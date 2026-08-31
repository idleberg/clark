//! Ported from `@clack/core`'s `prompts/prompt.ts` — the half of it that is a state machine.
//!
//! Upstream's `Prompt` is two things wearing one coat. One half owns a `readline.Interface`, an
//! `EventEmitter`, a `Promise`, and a frame-diffing writer; the other decides, for each keypress,
//! what the Prompt now *is* — its status, its user input, its cursor, its error, its value. Only
//! the second half is here. The first is the Emitter's (ADR-0002) and the driver's.
//!
//! ## What is ported
//!
//! `onKeypress` in full, in its original order, since the order is load-bearing: the cursor is read
//! back from the Line editor *before* validation, and the error status is cleared *before* the key
//! that may set it again. Also `_setUserInput`, `_clearUserInput`, `_setValue`, and the
//! `_isActionKey` / `_shouldSubmit` hooks, which are the seams subclasses reach through.
//!
//! ## What replaces the emitter
//!
//! clack subscribes to its own events (`this.on('userInput', …)`) as its subclassing mechanism: a
//! `TextPrompt` *is* a `Prompt` with three listeners attached. A `Map<string, Function[]>` port
//! would be neither cheaper nor clearer than what it is standing in for, so the seven events a
//! Prompt raises on itself are methods on [`PromptState`] instead, and the Prompt owns one such
//! state rather than being subclassed by it. The events upstream raises for *callers* —
//! `submit`, `cancel`, `value` — carry no information beyond the accessors on this struct, so they
//! are dropped in favour of polling: a driver that renders after every key has already observed
//! everything they would have told it.
//!
//! ## What is left out
//!
//! `accessible` mode, `AbortSignal`, and the `prompt()` promise are I/O. `render` and
//! `restoreCursor` are the Emitter's. `settings` is a value the Prompt owns rather than a global,
//! for the reasons [`crate::settings`] gives.
//!
//! One upstream behaviour is deliberately not reproduced: `ctrl+d` on an empty line closes the
//! readline interface without ever reaching clack's cancel check, so the Prompt is left running
//! above a dead editor and every later key is swallowed. The Line editor reports it as
//! [`LineEvent::Abort`](crate::line_editor::LineEvent::Abort) and this driver ignores it, which
//! means the Prompt stays usable. `ctrl+c` is unaffected: it cancels, through the alias, on both
//! sides.

use crate::line_editor::{Key, KeyName, LineEditor};
use crate::settings::{Action, Settings};

/// Upstream's `ClackState`. Named for the status it reports, to leave "state" for [`PromptState`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
	/// Nothing has been typed yet. The first keypress leaves it; the Emitter also leaves it after
	/// the first frame, which is why upstream hides the cursor here and nowhere else.
	#[default]
	Initial,
	Active,
	/// Validation rejected the value. Cleared by the next keypress.
	Error,
	Submit,
	Cancel,
}

impl Status {
	/// Whether the Prompt has produced its outcome and should stop being fed keys.
	pub fn is_finished(self) -> bool {
		matches!(self, Self::Submit | Self::Cancel)
	}
}

/// How a Prompt ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome<'a, T> {
	Submitted(Option<&'a T>),
	/// The user abandoned the Prompt. Distinct from a failure — the program may continue.
	Cancelled,
}

/// Validation of a Prompt's value: `Some(message)` rejects it.
///
/// clack also accepts a [Standard Schema](https://standardschema.dev), which has no Rust analogue.
/// This trait is the extension point for adapting a validation crate; the blanket impl below means
/// a closure works without one.
pub trait Validator<T> {
	/// `value` is `None` when the Prompt never set one — a bare `return` on an untouched `text`.
	fn validate(&mut self, value: Option<&T>) -> Option<String>;
}

impl<T, F> Validator<T> for F
where
	F: FnMut(Option<&T>) -> Option<String>,
{
	fn validate(&mut self, value: Option<&T>) -> Option<String> {
		self(value)
	}
}

/// The behaviour that upstream expresses by subclassing `Prompt` and subscribing to its events.
///
/// Every method has a default, so a state that only needs to turn user input into a value overrides
/// one of them. The names are upstream's, minus the `_` and the `on('…')`.
pub trait PromptState {
	/// What the Prompt resolves to.
	type Value;

	/// `trackValue`: whether the Line editor's text is this Prompt's user input.
	///
	/// True for `text` and friends. False for `select` and friends, which navigate rather than
	/// type — and which, being untracked, are the only Prompts the vim aliases apply to.
	const TRACKS_INPUT: bool = true;

	/// Whether [`confirm`](Self::confirm) ends the Prompt where it stands.
	///
	/// `ConfirmPrompt` is the only Prompt in clack that settles from inside one of its own
	/// listeners: its `confirm` handler sets `state = 'submit'` and calls `close()` there and then,
	/// several statements before `onKeypress` reaches its own submit check — and, because `close()`
	/// writes, several *writes* before it too. [`Prompt::closed_early`] is how a Session finds out.
	/// See ADR-0018.
	const CONFIRMS_ON_KEY: bool = false;

	/// `_isActionKey`: whether this key means something to the Prompt rather than to the text.
	///
	/// When it does and the Prompt tracks input, the Line editor is sent a `ctrl+h` to undo the
	/// insertion readline has already made. The default is upstream's — tab and nothing else, which
	/// is what makes tab inert in a `text` Prompt despite readline happily typing one.
	fn is_action_key(&self, s: Option<&str>, key: &Key) -> bool {
		let _ = key;
		s == Some("\t")
	}

	/// `_shouldSubmit`: whether `return` ends the Prompt. `multi-line` is the reason this exists.
	fn should_submit(&self, s: Option<&str>, key: &Key) -> bool {
		let _ = (s, key);
		true
	}

	/// `on('userInput')`: the Line editor's text changed.
	fn user_input(&mut self, input: &str) {
		let _ = input;
	}

	/// `on('cursor')`: a navigation key, resolved through the aliases.
	fn cursor(&mut self, action: Action) {
		let _ = action;
	}

	/// `on('confirm')`: a `y` or an `n` was typed, in either case.
	fn confirm(&mut self, yes: bool) {
		let _ = yes;
	}

	/// `on('key')`: the raw keypress, after the derived events and before submission is decided.
	fn key(&mut self, s: Option<&str>, key: &Key) {
		let _ = (s, key);
	}

	/// `on('finalize')`: the Prompt has settled on submit or cancel. The last chance to set a value.
	fn finalize(&mut self) {}

	/// Whether the `render` callback clears the user input on its way past.
	///
	/// Not an event: upstream's `password()` calls `this.clear()` from *inside* the callback that
	/// composes the error Frame, after it has captured the text that Frame shows. A widget here is
	/// handed a `&Prompt` and cannot do that, so the decision is asked for instead and
	/// [`Prompt::after_render`] carries it out — at the same point in the sequence, which is what
	/// leaves the old value on the error Frame and an empty one behind it.
	fn clears_after_render(&self, status: Status) -> bool {
		let _ = status;
		false
	}

	/// The value as it stands. Read by validation, and again for the outcome.
	fn value(&self) -> Option<&Self::Value>;
}

/// The user input split around the cursor, for a Prompt that draws its own.
///
/// clack builds a string with an ANSI inverse escape in the middle of it. A Frame is drawn into a
/// `Buffer` here, so the split is handed over and the styling is the widget's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputWithCursor<'a> {
	/// The Prompt has been submitted; no cursor is drawn.
	Plain(&'a str),
	/// The cursor is past the last character. Upstream appends a block, `U+2588`.
	AtEnd(&'a str),
	/// The cursor is over `at`, which upstream renders inverse.
	Over {
		before: &'a str,
		at: &'a str,
		after: &'a str,
	},
}

/// A Prompt: the shared machinery, driving one [`PromptState`].
///
/// Feed it keys with [`key`](Self::key) and read the accessors. It performs no I/O and holds no
/// terminal state, so a driver is free to render after every key, after none, or in another thread.
pub struct Prompt<S: PromptState> {
	state: S,
	status: Status,
	error: String,
	user_input: String,
	/// `_cursor`, a UTF-16 offset — see [`LineEditor::cursor_utf16`].
	cursor_utf16: usize,
	/// The same position as a byte offset into [`user_input`](Self::user_input).
	cursor: usize,
	editor: LineEditor,
	settings: Settings,
	validator: Option<Box<dyn Validator<S::Value>>>,
	/// Whether the last keypress settled the Prompt from inside a listener rather than at the end.
	closed_early: bool,
}

impl<S: PromptState> Prompt<S> {
	pub fn new(state: S) -> Self {
		Self {
			state,
			status: Status::default(),
			error: String::new(),
			user_input: String::new(),
			cursor_utf16: 0,
			cursor: 0,
			editor: LineEditor::new(),
			settings: Settings::default(),
			validator: None,
			closed_early: false,
		}
	}

	/// `validate`. Runs on `return`, against the value as it stands.
	pub fn with_validator(mut self, validator: impl Validator<S::Value> + 'static) -> Self {
		self.validator = Some(Box::new(validator));
		self
	}

	pub fn with_settings(mut self, settings: Settings) -> Self {
		self.settings = settings;
		self
	}

	/// `initialUserInput`: text the Prompt starts with, as though it had been typed.
	///
	/// The cursor lands at the end, because upstream writes it into readline from empty.
	pub fn with_initial_user_input(mut self, text: impl Into<String>) -> Self {
		let text = text.into();
		self.set_user_input(text.clone());
		if S::TRACKS_INPUT {
			self.editor.set_line(text);
			self.sync_cursor();
		}
		self
	}

	pub fn status(&self) -> Status {
		self.status
	}

	/// The validation message, empty unless [`status`](Self::status) is [`Status::Error`].
	pub fn error(&self) -> &str {
		&self.error
	}

	/// What the user has typed. Empty for a Prompt that does not track input.
	pub fn user_input(&self) -> &str {
		&self.user_input
	}

	/// The cursor as a byte offset into [`user_input`](Self::user_input).
	pub fn cursor(&self) -> usize {
		self.cursor
	}

	/// The cursor as a UTF-16 offset — the number clack's `_cursor` holds.
	pub fn cursor_utf16(&self) -> usize {
		self.cursor_utf16
	}

	pub fn state(&self) -> &S {
		&self.state
	}

	pub fn state_mut(&mut self) -> &mut S {
		&mut self.state
	}

	pub fn settings(&self) -> &Settings {
		&self.settings
	}

	/// The Line editor, for a Prompt that wants more than the text and the cursor.
	pub fn editor(&self) -> &LineEditor {
		&self.editor
	}

	/// The outcome, once [`Status::is_finished`] holds.
	pub fn outcome(&self) -> Option<Outcome<'_, S::Value>> {
		match self.status {
			Status::Submit => Some(Outcome::Submitted(self.state.value())),
			Status::Cancel => Some(Outcome::Cancelled),
			_ => None,
		}
	}

	/// `userInputWithCursor`: the input split where the cursor sits.
	///
	/// Upstream slices at `cursor + 1` *UTF-16 code units*, which cuts an astral character in half
	/// and prints two replacement characters. This takes the whole character instead. The
	/// divergence is visible only when the cursor rests on an emoji or other non-BMP character, and
	/// is left for Grid parity to adjudicate rather than reproduced on faith.
	pub fn input_with_cursor(&self) -> InputWithCursor<'_> {
		if self.status == Status::Submit {
			return InputWithCursor::Plain(&self.user_input);
		}
		let Some(at) = self.user_input[self.cursor..].chars().next() else {
			return InputWithCursor::AtEnd(&self.user_input);
		};
		let end = self.cursor + at.len_utf8();
		InputWithCursor::Over {
			before: &self.user_input[..self.cursor],
			at: &self.user_input[self.cursor..end],
			after: &self.user_input[end..],
		}
	}

	/// `_clearUserInput`.
	///
	/// Upstream sends the Line editor a `ctrl+u` and then blanks `userInput` directly. Those are not
	/// the same operation — `ctrl+u` kills only to the left of the cursor — so any text after the
	/// cursor survives in the editor while the Prompt believes the input is empty, and reappears the
	/// moment the next keypress reads the line back. Reproduced, because Prompts that call this
	/// (`autocomplete`, `date`) do so with the cursor at the end, where the two agree.
	pub fn clear_user_input(&mut self) {
		self.editor.write(None, &Key::ctrl('u'));
		self.set_user_input(String::new());
	}

	/// Feed one keypress: `char` as readline reports it, plus the decoded key.
	///
	/// This is `onKeypress`, and it runs after readline has already processed the key — upstream
	/// registers its listener on an input the interface is already reading, so `rl.line` is up to
	/// date by the time clack looks at it. The Line editor is therefore driven first, here too, and
	/// for every Prompt: upstream's interface is live whether or not the Prompt tracks its text.
	pub fn key(&mut self, s: Option<&str>, key: &Key) {
		self.closed_early = false;
		self.editor.write(s, key);

		if S::TRACKS_INPUT && key.name != Some(KeyName::Return) {
			// The insertion has already happened; an action key takes it back out again.
			if key.name.is_some() && self.state.is_action_key(s, key) {
				self.editor.write(None, &Key::ctrl('h'));
			}
			self.sync_cursor();
			let line = self.editor.line().to_string();
			self.set_user_input(line);
		}

		if self.status == Status::Error {
			self.status = Status::Active;
		}

		if let Some(key_name) = key.name {
			// The aliases are for Prompts that navigate. In one that types, `j` is a `j`.
			if !S::TRACKS_INPUT {
				if let Some(action) = self.settings.alias(&key_name.readline_name()) {
					self.state.cursor(action);
				}
			}
			if let Some(action) = Action::from_key_name(&key_name) {
				self.state.cursor(action);
			}
		}

		if let Some(c) = s {
			let lower = c.to_lowercase();
			if lower == "y" || lower == "n" {
				self.state.confirm(lower == "y");
				if S::CONFIRMS_ON_KEY {
					// Upstream's listener does the last two of these itself, in this order, and
					// then keeps going through `onKeypress` — so the checks below still run and can
					// still turn a submit into a cancel.
					self.closed_early = true;
					self.status = Status::Submit;
				}
			}
		}

		self.state.key(s, key);

		if key.name == Some(KeyName::Return) && self.state.should_submit(s, key) {
			if let Some(validator) = &mut self.validator {
				if let Some(problem) = validator.validate(self.state.value()) {
					self.error = problem;
					self.status = Status::Error;
					// `return` cleared the editor. Put the text back so it can be corrected.
					self.editor.set_line(self.user_input.clone());
				}
			}
			if self.status != Status::Error {
				self.status = Status::Submit;
			}
		}

		let name = key.name.map(|n| n.readline_name());
		let cancel = self.settings.is_action_key(
			&[s, name.as_deref(), key.sequence.as_deref()],
			Action::Cancel,
		);
		if cancel {
			self.status = Status::Cancel;
		}

		if self.status.is_finished() {
			self.state.finalize();
		}
	}

	/// Whether the last keypress settled the Prompt part-way through rather than at the end.
	///
	/// Only ever true for a state with [`PromptState::CONFIRMS_ON_KEY`] set, which is `confirm` and
	/// nothing else. A driver that ignores this still gets the right answer; what it loses is the
	/// two sequences upstream writes before the settled Frame. See [`crate::session::Session::key`].
	pub fn closed_early(&self) -> bool {
		self.closed_early
	}

	/// What the `render` callback does to the Prompt on its way past.
	///
	/// Called once per Frame, *after* the Frame has been composed — see
	/// [`PromptState::clears_after_render`] for why the order is the whole point. A driver that
	/// composes its own Frames has to call this itself; [`crate::session::Session`] does.
	pub fn after_render(&mut self) {
		if self.state.clears_after_render(self.status) {
			self.clear_user_input();
		}
	}

	/// `_setUserInput`, without the write-back: the caller has already updated the editor.
	fn set_user_input(&mut self, value: String) {
		self.user_input = value;
		self.state.user_input(&self.user_input);
	}

	fn sync_cursor(&mut self) {
		self.cursor = self.editor.cursor();
		self.cursor_utf16 = self.editor.cursor_utf16();
	}
}

impl<S: PromptState + Default> Default for Prompt<S> {
	fn default() -> Self {
		Self::new(S::default())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::line_editor::KeyName;

	/// A state that records everything it is told, so the dispatch order can be asserted.
	#[derive(Debug, Default)]
	struct Recording {
		value: Option<String>,
		log: Vec<String>,
	}

	impl PromptState for Recording {
		type Value = String;

		fn user_input(&mut self, input: &str) {
			self.log.push(format!("input({input})"));
			self.value = Some(input.to_string());
		}

		fn cursor(&mut self, action: Action) {
			self.log.push(format!("cursor({action:?})"));
		}

		fn confirm(&mut self, yes: bool) {
			self.log.push(format!("confirm({yes})"));
		}

		fn key(&mut self, _s: Option<&str>, key: &Key) {
			self.log.push(format!("key({:?})", key.name));
		}

		fn finalize(&mut self) {
			self.log.push("finalize".into());
		}

		fn value(&self) -> Option<&String> {
			self.value.as_ref()
		}
	}

	/// The same, but navigating rather than typing — a stand-in for `select`.
	#[derive(Debug, Default)]
	struct Navigating {
		log: Vec<Action>,
	}

	impl PromptState for Navigating {
		type Value = ();
		const TRACKS_INPUT: bool = false;

		fn cursor(&mut self, action: Action) {
			self.log.push(action);
		}

		fn value(&self) -> Option<&()> {
			Some(&())
		}
	}

	fn typed(prompt: &mut Prompt<Recording>, text: &str) {
		for c in text.chars() {
			let s = c.to_string();
			prompt.key(Some(&s), &Key::named(KeyName::Char(c)));
		}
	}

	fn press(prompt: &mut Prompt<Recording>, name: KeyName) {
		prompt.key(None, &Key::named(name));
	}

	#[test]
	fn typing_flows_through_the_line_editor_into_the_user_input() {
		let mut prompt = Prompt::new(Recording::default());
		typed(&mut prompt, "hi");
		assert_eq!(prompt.user_input(), "hi");
		assert_eq!(prompt.cursor(), 2);
		assert_eq!(prompt.state().value.as_deref(), Some("hi"));
	}

	#[test]
	fn the_cursor_is_reported_in_both_units() {
		let mut prompt = Prompt::new(Recording::default());
		typed(&mut prompt, "\u{1F600}");
		assert_eq!(prompt.cursor(), 4);
		assert_eq!(prompt.cursor_utf16(), 2);
	}

	#[test]
	fn return_submits_without_reading_the_cleared_line_back() {
		// readline empties itself on `return`. The Prompt skips the read-back for exactly that key,
		// which is the only reason the answer survives.
		let mut prompt = Prompt::new(Recording::default());
		typed(&mut prompt, "hi");
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Submit);
		assert_eq!(prompt.user_input(), "hi");
		assert!(matches!(prompt.outcome(), Some(Outcome::Submitted(Some(v))) if v == "hi"));
	}

	#[test]
	fn escape_cancels() {
		let mut prompt = Prompt::new(Recording::default());
		typed(&mut prompt, "hi");
		press(&mut prompt, KeyName::Escape);
		assert_eq!(prompt.status(), Status::Cancel);
		assert_eq!(prompt.outcome(), Some(Outcome::Cancelled));
	}

	#[test]
	fn ctrl_c_cancels_by_its_sequence() {
		let mut prompt = Prompt::new(Recording::default());
		prompt.key(Some("\u{3}"), &Key::ctrl('c'));
		assert_eq!(prompt.status(), Status::Cancel);
	}

	#[test]
	fn ctrl_d_on_an_empty_line_does_not_cancel() {
		// Upstream closes readline here and never reaches its own cancel check. The Prompt is left
		// running, and so is this one — see the module docs.
		let mut prompt = Prompt::new(Recording::default());
		prompt.key(None, &Key::ctrl('d'));
		assert_eq!(prompt.status(), Status::Initial);
		typed(&mut prompt, "x");
		assert_eq!(prompt.user_input(), "x");
	}

	#[test]
	fn tab_is_swallowed_because_it_is_an_action_key() {
		let mut prompt = Prompt::new(Recording::default());
		typed(&mut prompt, "hi");
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		assert_eq!(prompt.user_input(), "hi");
		assert_eq!(prompt.cursor(), 2);
	}

	#[test]
	fn a_failed_validation_sets_the_error_and_restores_the_line() {
		let mut prompt =
			Prompt::new(Recording::default()).with_validator(|v: Option<&String>| match v {
				Some(v) if v.len() >= 3 => None,
				_ => Some("too short".into()),
			});
		typed(&mut prompt, "hi");
		press(&mut prompt, KeyName::Return);

		assert_eq!(prompt.status(), Status::Error);
		assert_eq!(prompt.error(), "too short");
		// The editor was cleared by `return` and written back, so typing resumes where it left off.
		assert_eq!(prompt.editor().line(), "hi");
		assert_eq!(prompt.editor().cursor(), 2);

		typed(&mut prompt, "!");
		assert_eq!(prompt.status(), Status::Active);
		assert_eq!(prompt.user_input(), "hi!");

		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Submit);
	}

	#[test]
	fn validation_sees_no_value_when_nothing_was_typed() {
		let mut prompt = Prompt::new(Recording::default())
			.with_validator(|v: Option<&String>| v.is_none().then(|| "required".to_string()));
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Error);
		assert_eq!(prompt.error(), "required");
	}

	#[test]
	fn an_error_is_cleared_by_the_next_key_before_it_can_be_set_again() {
		let mut prompt = Prompt::new(Recording::default())
			.with_validator(|_: Option<&String>| Some("never happy".to_string()));
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Error);
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Error);
	}

	#[test]
	fn a_y_or_an_n_confirms_in_either_case() {
		let mut prompt = Prompt::new(Recording::default());
		typed(&mut prompt, "yN");
		let confirms: Vec<_> = prompt
			.state()
			.log
			.iter()
			.filter(|e| e.starts_with("confirm"))
			.collect();
		assert_eq!(confirms, ["confirm(true)", "confirm(false)"]);
	}

	#[test]
	fn the_events_fire_in_upstreams_order() {
		let mut prompt = Prompt::new(Recording::default());
		typed(&mut prompt, "y");
		press(&mut prompt, KeyName::Return);
		assert_eq!(
			prompt.state().log,
			[
				"input(y)",
				"confirm(true)",
				"key(Some(Char('y')))",
				"key(Some(Return))",
				"finalize",
			]
		);
	}

	#[test]
	fn the_vim_aliases_only_reach_a_prompt_that_does_not_type() {
		let mut navigating = Prompt::new(Navigating::default());
		navigating.key(Some("j"), &Key::named(KeyName::Char('j')));
		assert_eq!(navigating.state().log, [Action::Down]);

		let mut typing = Prompt::new(Recording::default());
		typed(&mut typing, "j");
		assert_eq!(typing.user_input(), "j");
		assert!(!typing.state().log.iter().any(|e| e.starts_with("cursor")));
	}

	#[test]
	fn an_arrow_key_is_an_action_for_every_prompt() {
		let mut prompt = Prompt::new(Recording::default());
		press(&mut prompt, KeyName::Up);
		assert!(prompt.state().log.contains(&"cursor(Up)".to_string()));
	}

	#[test]
	fn initial_user_input_lands_with_the_cursor_at_the_end() {
		let prompt = Prompt::new(Recording::default()).with_initial_user_input("seed");
		assert_eq!(prompt.user_input(), "seed");
		assert_eq!(prompt.cursor(), 4);
		assert_eq!(prompt.state().value.as_deref(), Some("seed"));
	}

	#[test]
	fn the_cursor_split_covers_a_whole_character() {
		let mut prompt = Prompt::new(Recording::default());
		typed(&mut prompt, "a\u{1F600}b");
		press(&mut prompt, KeyName::Left);
		press(&mut prompt, KeyName::Left);
		assert_eq!(
			prompt.input_with_cursor(),
			InputWithCursor::Over {
				before: "a",
				at: "\u{1F600}",
				after: "b"
			}
		);
	}

	#[test]
	fn the_cursor_split_is_dropped_once_the_prompt_is_submitted() {
		let mut prompt = Prompt::new(Recording::default());
		typed(&mut prompt, "hi");
		assert_eq!(prompt.input_with_cursor(), InputWithCursor::AtEnd("hi"));
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.input_with_cursor(), InputWithCursor::Plain("hi"));
	}

	#[test]
	fn clearing_the_input_leaves_whatever_was_right_of_the_cursor_in_the_editor() {
		let mut prompt = Prompt::new(Recording::default());
		typed(&mut prompt, "abcd");
		press(&mut prompt, KeyName::Left);
		press(&mut prompt, KeyName::Left);
		prompt.clear_user_input();

		assert_eq!(prompt.user_input(), "");
		assert_eq!(prompt.editor().line(), "cd");
		// And it comes back on the next key, because that is what upstream does.
		typed(&mut prompt, "x");
		assert_eq!(prompt.user_input(), "xcd");
	}
}
