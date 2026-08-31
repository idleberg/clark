//! `confirm()`, ported from `@clack/prompts`' `prompts/confirm.ts`.
//!
//! A yes-or-no question. Unlike every other Prompt here it does not type: the arrows and the vim
//! aliases move between the two choices, `return` takes the one that is lit, and a `y` or an `n`
//! answers outright without a `return` at all.
//!
//! It takes no validator, because upstream's `ConfirmOptions` has none — there are two answers and
//! both are always acceptable.

use clackatui_core::confirm::{ConfirmState, ConfirmWidget};
use clackatui_core::prompt::{Outcome, Prompt};
use clackatui_core::session::Session;
use clackatui_core::settings::Settings;
use clackatui_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// A `confirm` Prompt, waiting to be configured and run.
///
/// ```no_run
/// if clackatui::confirm("Publish?").interact()? {
///     // …
/// }
/// # Ok::<_, clackatui::ClackError>(())
/// ```
#[derive(Default)]
pub struct Confirm {
	message: String,
	active: Option<String>,
	inactive: Option<String>,
	initial_value: Option<bool>,
	vertical: bool,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
}

/// Ask a yes-or-no question.
pub fn confirm(message: impl Into<String>) -> Confirm {
	Confirm {
		message: message.into(),
		..Confirm::default()
	}
}

impl Confirm {
	/// The label for the `true` choice. Defaults to `Yes`.
	pub fn active(mut self, label: impl Into<String>) -> Self {
		self.active = Some(label.into());
		self
	}

	/// The label for the `false` choice. Defaults to `No`.
	pub fn inactive(mut self, label: impl Into<String>) -> Self {
		self.inactive = Some(label.into());
		self
	}

	/// Which choice is lit when the Prompt opens. Defaults to `true`.
	pub fn initial_value(mut self, value: bool) -> Self {
		self.initial_value = Some(value);
		self
	}

	/// Draw the two choices one above the other rather than side by side.
	pub fn vertical(mut self, vertical: bool) -> Self {
		self.vertical = vertical;
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
	pub fn interact(self) -> Result<bool, ClackError> {
		self.interact_opt()?.ok_or(ClackError::Cancelled)
	}

	/// Ask, and report a cancel as [`None`].
	///
	/// A cancelled `confirm` still has a value, and it is not the one that was lit: `escape` is an
	/// alias for `cancel`, and upstream flips the answer on its way past. That value is deliberately
	/// not returned here — a cancel is not an answer — but it is what the closing Frame shows.
	pub fn interact_opt(self) -> Result<Option<bool>, ClackError> {
		let prompt = driver::run(self.session())?;
		Ok(match prompt.outcome() {
			Some(Outcome::Submitted(value)) => value.copied(),
			_ => None,
		})
	}

	/// The Session this Prompt would be run as, without running it.
	pub fn session(self) -> Session<ConfirmState> {
		let mut state = ConfirmState::new();
		if let Some(initial) = self.initial_value {
			state = state.with_initial_value(initial);
		}

		let mut prompt = Prompt::new(state);
		if let Some(settings) = self.settings {
			prompt = prompt.with_settings(settings);
		}

		let message = self.message;
		let (active, inactive) = (self.active, self.inactive);
		let vertical = self.vertical;
		let theme = self.theme.unwrap_or_else(Theme::clack);
		let with_guide = self.with_guide;

		Session::new(prompt, move |prompt, columns| {
			let mut widget = ConfirmWidget::new(prompt, &message)
				.with_theme(&theme)
				.with_vertical(vertical)
				// The width the message is wrapped against. Upstream reads it off the Prompt's own
				// output stream, which for a real program is the terminal the Session already knows.
				.with_columns(columns);
			if let Some(active) = &active {
				widget = widget.with_active(active);
			}
			if let Some(inactive) = &inactive {
				widget = widget.with_inactive(inactive);
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

	fn answer(session: &Session<ConfirmState>) -> Option<bool> {
		match session.outcome() {
			Some(Outcome::Submitted(value)) => value.copied(),
			_ => None,
		}
	}

	#[test]
	fn return_takes_the_choice_that_is_lit() {
		let mut session = confirm("Publish?").session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session), Some(true));
	}

	#[test]
	fn an_initial_value_lights_the_other_one() {
		let mut session = confirm("Publish?").initial_value(false).session();
		let opening = session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session), Some(false));
		assert!(opening.contains("No"));
	}

	#[test]
	fn an_arrow_moves_between_them() {
		let mut session = confirm("Publish?").session();
		session.open();
		session.key(None, &Key::named(KeyName::Right));
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session), Some(false));
	}

	/// The path no other Prompt has: settled from inside a listener, before `onKeypress` is done.
	#[test]
	fn a_y_answers_without_a_return() {
		let mut session = confirm("Publish?").session();
		session.open();
		let written = session.key(Some("n"), &Key::named(KeyName::Char('n')));

		assert_eq!(answer(&session), Some(false));
		assert!(session.is_finished());
		// One row up before anything is redrawn, and the cursor shown before the settled Frame
		// rather than after it. Both are upstream's, and neither is anything a driver would arrange.
		assert!(
			written.starts_with("\u{1b}[1A\n\u{1b}[?25h"),
			"the early close was not written the way clack writes it: {written:?}"
		);
		assert!(written.ends_with('\n'), "the second close is missing");
	}

	#[test]
	fn custom_labels_are_what_the_frame_shows() {
		let mut session = confirm("Publish?")
			.active("Ship it")
			.inactive("Not yet")
			.session();
		let opening = session.open();
		assert!(opening.contains("Ship it") && opening.contains("Not yet"));
	}

	#[test]
	fn vertical_puts_them_on_two_rows() {
		let mut session = confirm("Publish?").vertical(true).session();
		let opening = session.open();
		let rows: Vec<&str> = opening.lines().collect();
		assert!(
			rows.iter().any(|row| row.contains("Yes")) && rows.iter().any(|row| row.contains("No")),
			"{opening:?}"
		);
		assert!(
			!rows
				.iter()
				.any(|row| row.contains("Yes") && row.contains("No")),
			"the two choices are still on one row: {opening:?}"
		);
	}

	#[test]
	fn a_cancel_is_no_answer_at_all() {
		let mut session = confirm("Publish?").session();
		session.open();
		session.key(None, &Key::named(KeyName::Escape));
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
		assert_eq!(answer(&session), None);
	}
}
