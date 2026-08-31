//! `selectKey()`, ported from `@clack/prompts`' `select-key.ts`.
//!
//! A list answered by pressing a letter. Each option is drawn with its value in a chip beside the
//! label, and pressing the value's first character submits that option there and then — no arrows,
//! no `return`.
//!
//! # There is no cursor to move
//!
//! [`SelectKey::initial_value`] decides which option opens highlighted and nothing moves it
//! afterwards, so the highlight says where the list *started*, not where the answer is coming from.
//! And it is matched against each option's first character rather than against its value, which is
//! upstream's and is spelled out in [`clackatui_core::select_key`].

use std::fmt::Display;

use clackatui_core::prompt::{Outcome, Prompt};
use clackatui_core::select::SelectOption;
use clackatui_core::select_key::{SelectKeyState, SelectKeyWidget};
use clackatui_core::session::Session;
use clackatui_core::settings::Settings;
use clackatui_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// A `selectKey` Prompt, waiting to be configured and run.
///
/// ```no_run
/// let answer = clackatui::select_key("Overwrite?")
///     .labelled("yes", "Overwrite the file")
///     .labelled("no", "Leave it alone")
///     .interact()?;
/// # Ok::<_, clackatui::ClackError>(())
/// ```
pub struct SelectKey<T> {
	message: String,
	options: Vec<SelectOption<T>>,
	initial_value: Option<String>,
	case_sensitive: bool,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
}

/// Ask for one of a list of options, chosen by a keypress.
pub fn select_key<T>(message: impl Into<String>) -> SelectKey<T> {
	SelectKey {
		message: message.into(),
		options: Vec::new(),
		initial_value: None,
		case_sensitive: false,
		theme: None,
		settings: None,
		with_guide: None,
	}
}

impl<T> SelectKey<T> {
	/// An option built elsewhere — the way to give one a hint.
	pub fn choice(mut self, option: SelectOption<T>) -> Self {
		self.options.push(option);
		self
	}

	/// Every option at once, replacing anything added so far.
	pub fn options(mut self, options: impl IntoIterator<Item = SelectOption<T>>) -> Self {
		self.options = options.into_iter().collect();
		self
	}

	/// An option with a label of its own. The value is still what the chip prints and what the
	/// keypress is matched against, so it wants to be short.
	pub fn labelled(self, value: T, label: impl Into<String>) -> Self {
		self.choice(SelectOption::labelled(value, label))
	}

	/// `initialValue`: which option opens highlighted.
	///
	/// One character, because that is what upstream compares it with — see the module docs. Anything
	/// longer, and anything the list does not begin with, leaves the highlight on the first option.
	pub fn initial_value(mut self, value: impl Into<String>) -> Self {
		self.initial_value = Some(value.into());
		self
	}

	/// `caseSensitive`: whether `A` and `a` are two keys. Off by default.
	pub fn case_sensitive(mut self, case_sensitive: bool) -> Self {
		self.case_sensitive = case_sensitive;
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
	pub fn interact(self) -> Result<T, ClackError>
	where
		T: Clone + Display + 'static,
	{
		self.interact_opt()?.ok_or(ClackError::Cancelled)
	}

	/// Ask, and report a cancel as [`None`].
	///
	/// A `return` is [`None`] too, and for a reason worth knowing: it submits, but with no value,
	/// because only a matching keypress ever sets one.
	pub fn interact_opt(self) -> Result<Option<T>, ClackError>
	where
		T: Clone + Display + 'static,
	{
		let prompt = driver::run(self.session())?;
		Ok(match prompt.outcome() {
			Some(Outcome::Submitted(value)) => value.cloned(),
			_ => None,
		})
	}

	/// The Session this Prompt would be run as, without running it.
	pub fn session(self) -> Session<SelectKeyState<T>>
	where
		T: Display + 'static,
	{
		let mut state = SelectKeyState::new(self.options).with_case_sensitive(self.case_sensitive);
		if let Some(initial) = &self.initial_value {
			state = state.with_initial_value(initial);
		}

		let mut prompt = Prompt::new(state);
		if let Some(settings) = self.settings {
			prompt = prompt.with_settings(settings);
		}

		let message = self.message;
		let theme = self.theme.unwrap_or_else(Theme::clack);
		let with_guide = self.with_guide;

		Session::new(prompt, move |prompt, columns, _rows| {
			let mut widget = SelectKeyWidget::new(prompt, &message)
				.with_theme(&theme)
				.with_columns(columns as usize);
			if let Some(with_guide) = with_guide {
				widget = widget.with_guide(with_guide);
			}
			widget.frame()
		})
	}
}

impl<T: Display> SelectKey<T> {
	/// An option labelled by its own value.
	pub fn option(self, value: T) -> Self {
		self.choice(SelectOption::new(value))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use clackatui_core::line_editor::{Key, KeyName};
	use clackatui_core::prompt::Status;

	fn answer(session: &Session<SelectKeyState<String>>) -> Option<String> {
		match session.outcome() {
			Some(Outcome::Submitted(value)) => value.cloned(),
			_ => None,
		}
	}

	fn overwrite() -> SelectKey<String> {
		select_key("Overwrite?")
			.labelled("yes".to_string(), "Overwrite the file")
			.labelled("no".to_string(), "Leave it alone")
			.labelled("diff".to_string(), "Show me the difference")
	}

	fn typed(session: &mut Session<SelectKeyState<String>>, c: char) -> String {
		session.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)))
	}

	#[test]
	fn a_keypress_is_the_whole_answer() {
		let mut session = overwrite().session();
		session.open();
		typed(&mut session, 'n');
		assert_eq!(answer(&session).as_deref(), Some("no"));
		assert!(session.is_finished());
	}

	#[test]
	fn a_key_no_option_begins_with_does_nothing() {
		let mut session = overwrite().session();
		session.open();
		assert_eq!(typed(&mut session, 'q'), "");
		assert!(!session.is_finished());
	}

	/// The cursor is shown again before the settled Frame rather than after the newline that
	/// follows it, because upstream resolves from inside the `key` listener.
	#[test]
	fn the_cursor_comes_back_before_the_settled_frame() {
		let mut session = overwrite().session();
		session.open();
		let last = typed(&mut session, 'y');
		assert!(last.starts_with("\u{1b}[?25h"), "{last:?}");
		assert!(last.ends_with('\n'), "{last:?}");
	}

	#[test]
	fn the_opening_frame_carries_a_chip_and_a_label_for_each_option() {
		let mut session = overwrite().session();
		let opening = session.open();
		for text in [
			" yes ",
			"Overwrite the file",
			" diff ",
			"Show me the difference",
		] {
			assert!(opening.contains(text), "{text} is missing: {opening:?}");
		}
	}

	#[test]
	fn an_initial_value_moves_the_highlight_and_nothing_else() {
		let mut session = overwrite().initial_value("d").session();
		session.open();
		assert_eq!(session.prompt().state().cursor(), 2);
		// It is a highlight, not a selection: `return` still submits nothing.
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.status(), Status::Submit);
		assert_eq!(answer(&session), None);
	}

	#[test]
	fn case_sensitive_tells_two_options_apart() {
		let mut session = select_key::<String>("Pick")
			.labelled("a".to_string(), "lower")
			.labelled("A".to_string(), "upper")
			.case_sensitive(true)
			.session();
		session.open();
		typed(&mut session, 'A');
		assert_eq!(answer(&session).as_deref(), Some("A"));
	}

	#[test]
	fn a_cancel_is_no_answer_at_all() {
		let mut session = overwrite().session();
		session.open();
		session.key(Some(""), &Key::named(KeyName::Escape));
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
	}
}
