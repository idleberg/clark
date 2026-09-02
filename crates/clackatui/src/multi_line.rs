//! `multiline()`, ported from `@clack/prompts`' `prompts/multi-line.ts`.
//!
//! The same shape as [`text`](crate::text) with one option more, and one rule that is nothing like
//! it: `return` inserts a newline instead of settling the Prompt. Two of them in a row at the end of
//! the text settle it, and the newline the first one made is taken back out on the way — or, with
//! [`show_submit`](MultiLine::show_submit) on, `return` is only ever a newline and tab moves the
//! focus to a `[ submit ]` button that `return` then presses.
//!
//! # `initialValue` and `defaultValue` are not the same option
//!
//! The same distinction `text` makes, wired the same way. `initialValue` reaches the base class as
//! `initialUserInput` and is typed into the field; `defaultValue` is never drawn and stands in only
//! for an answer that is empty when the Prompt settles.

use clackatui_core::multi_line::{MultiLineState, MultiLineWidget};
use clackatui_core::prompt::{Outcome, Prompt, Validator};
use clackatui_core::session::Session;
use clackatui_core::settings::Settings;
use clackatui_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// A `multiline` Prompt, waiting to be configured and run.
///
/// ```no_run
/// let bio = clackatui::multiline("Tell us about yourself")
///     .placeholder("I once...")
///     .show_submit(true)
///     .interact()?;
/// # Ok::<_, clackatui::ClackError>(())
/// ```
#[derive(Default)]
pub struct MultiLine {
	message: String,
	placeholder: Option<String>,
	default_value: Option<String>,
	initial_value: Option<String>,
	show_submit: bool,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
	validator: Option<Box<dyn Validator<String>>>,
}

/// Ask for several lines of text.
pub fn multiline(message: impl Into<String>) -> MultiLine {
	MultiLine {
		message: message.into(),
		..MultiLine::default()
	}
}

impl MultiLine {
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

	/// Text the field starts with, as though the user had typed it. The cursor lands at its end.
	pub fn initial_value(mut self, value: impl Into<String>) -> Self {
		self.initial_value = Some(value.into());
		self
	}

	/// Draw a `[ submit ]` button, focused with tab and pressed with `return`.
	///
	/// With it on, `return` in the editor is always a newline — so the double-`return` that would
	/// otherwise settle the Prompt simply inserts two.
	pub fn show_submit(mut self, show_submit: bool) -> Self {
		self.show_submit = show_submit;
		self
	}

	/// Reject an answer with a message. `None` accepts it.
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
	pub fn interact(self) -> Result<String, ClackError> {
		self.interact_opt()?.ok_or(ClackError::Cancelled)
	}

	/// Ask, and report a cancel as [`None`].
	pub fn interact_opt(self) -> Result<Option<String>, ClackError> {
		let prompt = driver::run(self.session())?;
		Ok(match prompt.outcome() {
			Some(Outcome::Submitted(value)) => Some(value.cloned().unwrap_or_default()),
			_ => None,
		})
	}

	/// The Session this Prompt would be run as, without running it.
	pub fn session(self) -> Session<MultiLineState> {
		let mut state = MultiLineState::new().with_show_submit(self.show_submit);
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
			prompt = prompt.with_validator(move |value: Option<&String>| validator.validate(value));
		}

		let message = self.message;
		let placeholder = self.placeholder;
		let theme = self.theme.unwrap_or_else(Theme::clack);
		let with_guide = self.with_guide;

		// The width matters here, unlike in `text`: the field is wrapped by the widget rather than
		// by the outer wrap, so it has to follow the terminal.
		Session::new(prompt, move |prompt, columns, _rows| {
			let mut widget = MultiLineWidget::new(prompt, &message)
				.with_theme(&theme)
				.with_columns(columns as usize);
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

	fn typed(session: &mut Session<MultiLineState>, s: &str) {
		for c in s.chars() {
			session.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
		}
	}

	/// A bare `return`, as readline reports it.
	fn enter(session: &mut Session<MultiLineState>) -> String {
		session.key(Some(""), &Key::named(KeyName::Return))
	}

	fn answer(session: &Session<MultiLineState>) -> Option<String> {
		match session.outcome() {
			Some(Outcome::Submitted(value)) => Some(value.cloned().unwrap_or_default()),
			_ => None,
		}
	}

	#[test]
	fn two_returns_settle_the_prompt_and_the_first_newline_is_taken_back() {
		let mut session = multiline("Bio").session();
		session.open();
		typed(&mut session, "one");
		enter(&mut session);
		typed(&mut session, "two");
		enter(&mut session);
		assert!(!session.is_finished(), "one return is a newline");
		enter(&mut session);
		assert_eq!(answer(&session).as_deref(), Some("one\ntwo"));
	}

	#[test]
	fn a_button_takes_over_from_the_double_return() {
		let mut session = multiline("Bio").show_submit(true).session();
		session.open();
		typed(&mut session, "one");
		enter(&mut session);
		enter(&mut session);
		assert!(
			!session.is_finished(),
			"with a button, a return is only ever a newline"
		);

		session.key(Some("\t"), &Key::named(KeyName::Tab));
		enter(&mut session);
		assert_eq!(answer(&session).as_deref(), Some("one\n\n"));
	}

	#[test]
	fn an_initial_value_is_editable_and_a_default_value_is_not_drawn() {
		let mut session = multiline("Bio").initial_value("one\ntwo").session();
		assert!(
			session.open().contains("two"),
			"the initial value was not typed in"
		);

		let mut session = multiline("Bio").default_value("anonymous").session();
		assert!(!session.open().contains("anonymous"));
		enter(&mut session);
		enter(&mut session);
		assert_eq!(answer(&session).as_deref(), Some("anonymous"));
	}

	#[test]
	fn a_validator_holds_the_prompt_open_until_the_answer_passes() {
		let mut session = multiline("Bio")
			.validate(|value: Option<&String>| {
				value
					.filter(|v| !v.is_empty())
					.is_none()
					.then(|| "Required".to_owned())
			})
			.session();
		session.open();

		enter(&mut session);
		enter(&mut session);
		assert_eq!(session.status(), Status::Error);
		assert!(!session.is_finished());

		typed(&mut session, "Jan");
		enter(&mut session);
		enter(&mut session);
		assert_eq!(answer(&session).as_deref(), Some("Jan"));
	}

	#[test]
	fn a_cancel_is_no_answer_at_all() {
		let mut session = multiline("Bio").session();
		session.open();
		typed(&mut session, "Jan");
		session.key(None, &Key::named(KeyName::Escape));
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
		assert_eq!(answer(&session), None);
	}

	#[test]
	fn turning_the_guide_off_takes_the_bar_out_of_the_frame() {
		let mut guided = multiline("Bio").session();
		let mut bare = multiline("Bio").with_guide(false).session();
		assert!(guided.open().contains('│'));
		assert!(!bare.open().contains('│'));
	}

	/// The one thing this builder has to get right that `text`'s does not: the widget's width has to
	/// be the terminal's, and it has to move when the terminal does.
	#[test]
	fn the_field_is_wrapped_against_the_terminal_and_follows_a_resize() {
		let mut session = multiline("Bio").session();
		session.open();
		typed(&mut session, "aaaa bbbb cccc dddd");

		let wide = session.frame().lines.len();
		session.resize(23, 20);
		let narrow = session.frame().lines.len();
		assert!(
			narrow > wide,
			"the field did not re-wrap: {wide} rows at 80, {narrow} at 23"
		);
	}
}
