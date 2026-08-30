//! `text()`, ported from `@clack/prompts`' `prompts/text.ts`.
//!
//! Upstream is a function that builds a `TextPrompt`, hands it a `render` closure over the options
//! the state machine never sees, and awaits it. This is the same shape with a builder in front of
//! it, because Rust has no object literal: `text("What is your name?").placeholder("Jan")` is
//! `text({ message: 'What is your name?', placeholder: 'Jan' })`.
//!
//! # `initialValue` and `defaultValue` are not the same option
//!
//! They are easy to confuse and clack keeps them apart deliberately. `initialValue` is typed into
//! the field for the user and is editable from the first keypress; `defaultValue` is never drawn
//! and stands in only for an answer that is empty when the Prompt settles. Upstream forwards the
//! first to the base class's `initialUserInput` and keeps the second on the `text` state, which is
//! exactly how they are wired below.

use clackatui_core::prompt::{Outcome, Prompt, Validator};
use clackatui_core::session::Session;
use clackatui_core::settings::Settings;
use clackatui_core::text::{TextState, TextWidget};
use clackatui_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// A `text` Prompt, waiting to be configured and run.
///
/// ```no_run
/// let name = clackatui::text("What is your name?")
///     .placeholder("Jan")
///     .interact()?;
/// # Ok::<_, clackatui::ClackError>(())
/// ```
#[derive(Default)]
pub struct Text {
	message: String,
	placeholder: Option<String>,
	default_value: Option<String>,
	initial_value: Option<String>,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
	validator: Option<Box<dyn Validator<String>>>,
}

/// Ask for a line of text.
pub fn text(message: impl Into<String>) -> Text {
	Text {
		message: message.into(),
		..Text::default()
	}
}

impl Text {
	/// A hint shown while the field is empty. Never becomes the answer.
	pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
		self.placeholder = Some(placeholder.into());
		self
	}

	/// What an empty answer means. Not drawn — see the module docs.
	pub fn default_value(mut self, value: impl Into<String>) -> Self {
		self.default_value = Some(value.into());
		self
	}

	/// Text the field starts with, as though the user had typed it.
	pub fn initial_value(mut self, value: impl Into<String>) -> Self {
		self.initial_value = Some(value.into());
		self
	}

	/// Reject an answer with a message. `None` accepts it.
	///
	/// The argument is an [`Option`] because upstream runs validation against a value that may never
	/// have been set — a bare `return` on an untouched Prompt reaches the validator with nothing.
	pub fn validate(mut self, validator: impl Validator<String> + 'static) -> Self {
		self.validator = Some(Box::new(validator));
		self
	}

	pub fn theme(mut self, theme: Theme) -> Self {
		self.theme = Some(theme);
		self
	}

	pub fn settings(mut self, settings: Settings) -> Self {
		self.settings = Some(settings);
		self
	}

	/// Whether the Guide — the bar down the left margin — is drawn beside this Prompt.
	pub fn with_guide(mut self, with_guide: bool) -> Self {
		self.with_guide = Some(with_guide);
		self
	}

	/// Ask, and bubble a cancel as [`ClackError::Cancelled`].
	///
	/// This is what most CLIs want: the user pressing `ctrl+c` ends the program rather than
	/// returning a value nobody asked for. Use [`interact_opt`](Self::interact_opt) for clack's own
	/// semantics, where a cancel is a value and the flow carries on.
	pub fn interact(self) -> Result<String, ClackError> {
		self.interact_opt()?.ok_or(ClackError::Cancelled)
	}

	/// Ask, and report a cancel as [`None`].
	///
	/// clack's `text()` resolves with a symbol rather than throwing, and `group()` is built on that
	/// — it writes `'canceled'` into its results, calls `onCancel`, and moves on to the next Prompt,
	/// which an unwinding cancel could not express.
	pub fn interact_opt(self) -> Result<Option<String>, ClackError> {
		let prompt = driver::run(self.session())?;
		Ok(match prompt.outcome() {
			Some(Outcome::Submitted(value)) => Some(value.cloned().unwrap_or_default()),
			_ => None,
		})
	}

	/// The Session this Prompt would be run as, without running it.
	///
	/// Public because it is the seam a caller needs for anything the blocking driver does not do —
	/// driving a Prompt from an existing event loop, or feeding it recorded keys in a test.
	pub fn session(self) -> Session<TextState> {
		let mut state = TextState::new();
		if let Some(default) = self.default_value {
			state = state.with_default_value(default);
		}

		let mut prompt = Prompt::new(state);
		if let Some(settings) = self.settings {
			prompt = prompt.with_settings(settings);
		}
		if let Some(initial) = self.initial_value {
			prompt = prompt.with_initial_user_input(initial);
		}
		if let Some(mut validator) = self.validator {
			// Through a closure rather than directly: `Validator` is blanket-implemented for
			// `FnMut`, and a boxed trait object is not one of those.
			prompt = prompt.with_validator(move |value: Option<&String>| validator.validate(value));
		}

		// The Frame's own options, which upstream closes over in its `render` callback rather than
		// passing to the core Prompt at all.
		let message = self.message;
		let placeholder = self.placeholder;
		let theme = self.theme.unwrap_or_else(Theme::clack);
		let with_guide = self.with_guide;

		Session::new(prompt, move |prompt| {
			let mut widget = TextWidget::new(prompt, &message).with_theme(&theme);
			if let Some(placeholder) = &placeholder {
				widget = widget.with_placeholder(placeholder);
			}
			if let Some(with_guide) = with_guide {
				widget = widget.with_guide(with_guide);
			}
			widget.frame()
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use clackatui_core::line_editor::{Key, KeyName};
	use clackatui_core::prompt::Status;

	fn typed(session: &mut Session<TextState>, s: &str) {
		for c in s.chars() {
			session.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
		}
	}

	fn answer(session: &Session<TextState>) -> Option<String> {
		match session.outcome() {
			Some(Outcome::Submitted(value)) => Some(value.cloned().unwrap_or_default()),
			_ => None,
		}
	}

	/// The builder drives a Session without a terminal, which is the point of `session()` being
	/// public: everything below runs the real thing with no I/O anywhere.
	#[test]
	fn a_typed_answer_comes_back() {
		let mut session = text("What is your name?").session();
		session.open();
		typed(&mut session, "Jan");
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session).as_deref(), Some("Jan"));
	}

	#[test]
	fn an_initial_value_is_editable_and_a_default_value_is_not_drawn() {
		let mut session = text("foo").initial_value("Jan").session();
		let opening = session.open();
		assert!(
			opening.contains("Jan"),
			"the initial value was not typed in"
		);

		let mut session = text("foo").default_value("anonymous").session();
		let opening = session.open();
		assert!(
			!opening.contains("anonymous"),
			"the default value was drawn: {opening:?}"
		);
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session).as_deref(), Some("anonymous"));
	}

	#[test]
	fn a_validator_holds_the_prompt_open_until_the_answer_passes() {
		let mut session = text("foo")
			.validate(|value: Option<&String>| {
				value
					.filter(|v| !v.is_empty())
					.is_none()
					.then(|| "Required".to_owned())
			})
			.session();
		session.open();

		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.status(), Status::Error);
		assert!(!session.is_finished());

		typed(&mut session, "Jan");
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session).as_deref(), Some("Jan"));
	}

	#[test]
	fn a_cancel_is_no_answer_at_all() {
		let mut session = text("foo").session();
		session.open();
		typed(&mut session, "Jan");
		session.key(None, &Key::named(KeyName::Escape));
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
		assert_eq!(answer(&session), None);
	}

	#[test]
	fn turning_the_guide_off_takes_the_bar_out_of_the_frame() {
		let mut guided = text("foo").session();
		let mut bare = text("foo").with_guide(false).session();
		assert!(guided.open().contains('│'));
		assert!(!bare.open().contains('│'));
	}
}
