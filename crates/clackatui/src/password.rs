//! `password()`, ported from `@clack/prompts`' `prompts/password.ts`.
//!
//! A `text` whose field is drawn as a row of mask characters. The builder is the same shape as
//! [`crate::Text`]'s minus the two options a masked field has no use for — a placeholder would be
//! drawn instead of the mask, and a default value would be an answer nobody typed — and plus the
//! two upstream adds: `mask` and `clear_on_error`.

use clackatui_core::password::{PasswordState, PasswordWidget};
use clackatui_core::prompt::{Outcome, Prompt, Validator};
use clackatui_core::session::Session;
use clackatui_core::settings::Settings;
use clackatui_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// A `password` Prompt, waiting to be configured and run.
///
/// ```no_run
/// let secret = clackatui::password("Passphrase").interact()?;
/// # Ok::<_, clackatui::ClackError>(())
/// ```
#[derive(Default)]
pub struct Password {
	message: String,
	mask: Option<String>,
	clear_on_error: bool,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
	validator: Option<Box<dyn Validator<String>>>,
}

/// Ask for a line of text, drawn masked.
pub fn password(message: impl Into<String>) -> Password {
	Password {
		message: message.into(),
		..Password::default()
	}
}

impl Password {
	/// What each unit of the answer is drawn as. Defaults to the Theme's `▪`.
	///
	/// One character. Upstream slices the mask at an offset counted in units of the *answer*, so a
	/// longer one puts the cursor in the wrong place — a defect this port reproduces rather than
	/// corrects, which is a reason not to use one rather than a reason to allow it.
	pub fn mask(mut self, mask: impl Into<String>) -> Self {
		self.mask = Some(mask.into());
		self
	}

	/// Throw the typed value away when validation rejects it, so the user starts again.
	pub fn clear_on_error(mut self, clear: bool) -> Self {
		self.clear_on_error = clear;
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
	pub fn session(self) -> Session<PasswordState> {
		let state = PasswordState::new().with_clear_on_error(self.clear_on_error);

		let mut prompt = Prompt::new(state);
		if let Some(settings) = self.settings {
			prompt = prompt.with_settings(settings);
		}
		if let Some(mut validator) = self.validator {
			prompt = prompt.with_validator(move |value: Option<&String>| validator.validate(value));
		}

		let message = self.message;
		let mask = self.mask;
		let theme = self.theme.unwrap_or_else(Theme::clack);
		let with_guide = self.with_guide;

		Session::new(prompt, move |prompt, _columns, _rows| {
			// The Theme first: it carries the default mask, so setting one after a mask would undo
			// it.
			let mut widget = PasswordWidget::new(prompt, &message).with_theme(&theme);
			if let Some(mask) = &mask {
				widget = widget.with_mask(mask);
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

	fn typed(session: &mut Session<PasswordState>, s: &str) {
		for c in s.chars() {
			session.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
		}
	}

	fn answer(session: &Session<PasswordState>) -> Option<String> {
		match session.outcome() {
			Some(Outcome::Submitted(value)) => Some(value.cloned().unwrap_or_default()),
			_ => None,
		}
	}

	#[test]
	fn the_answer_comes_back_unmasked_and_was_never_written_out() {
		let mut session = password("Passphrase").session();
		let mut stream = session.open();
		stream.push_str(&{
			typed(&mut session, "hunter2");
			String::new()
		});
		session.key(None, &Key::named(KeyName::Return));

		assert_eq!(answer(&session).as_deref(), Some("hunter2"));
		assert!(
			!stream.contains("hunter2"),
			"the opening Frame leaked the answer"
		);
	}

	#[test]
	fn a_custom_mask_is_what_reaches_the_terminal() {
		let mut session = password("Passphrase").mask("*").session();
		session.open();
		let written = session.key(Some("a"), &Key::named(KeyName::Char('a')));
		assert!(written.contains('*'), "the mask was not drawn: {written:?}");
		assert!(!written.contains('a'), "the character was drawn");
	}

	/// The one option in clack that changes the Prompt rather than the Frame. The Session applies it
	/// after the error Frame has been written, so the message is on screen and the field is empty.
	#[test]
	fn clearing_on_error_empties_the_field_behind_the_frame_that_reported_it() {
		let mut session = password("Passphrase")
			.clear_on_error(true)
			.validate(|value: Option<&String>| {
				value
					.filter(|v| v.len() >= 4)
					.is_none()
					.then(|| "Too short".to_owned())
			})
			.session();
		session.open();
		typed(&mut session, "ab");

		let written = session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.status(), Status::Error);
		assert!(written.contains("Too short"));
		assert!(
			written.contains("▪▪"),
			"the error Frame lost the value it was drawn for: {written:?}"
		);
		assert_eq!(
			session.prompt().user_input(),
			"",
			"the field was not cleared behind it"
		);

		typed(&mut session, "abcd");
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session).as_deref(), Some("abcd"));
	}

	#[test]
	fn without_clearing_the_field_keeps_what_failed() {
		let mut session = password("Passphrase")
			.validate(|_: Option<&String>| Some("Nope".to_owned()))
			.session();
		session.open();
		typed(&mut session, "ab");
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.prompt().user_input(), "ab");
	}

	#[test]
	fn a_cancel_is_no_answer_at_all() {
		let mut session = password("Passphrase").session();
		session.open();
		typed(&mut session, "hunter2");
		session.key(None, &Key::named(KeyName::Escape));
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
		assert_eq!(answer(&session), None);
	}
}
