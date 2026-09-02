//! `select()`, ported from `@clack/prompts`' `prompts/select.ts`.
//!
//! A list, one answer. The arrows and the vim aliases walk it, disabled options are stepped over
//! rather than landed on, and `return` takes whichever one the cursor is on.
//!
//! It takes no validator, for the reason `confirm` does not: upstream's `SelectOptions` has none, and
//! there is nothing to reject — the options are the acceptable answers.
//!
//! # The list is cut to the terminal
//!
//! A `select` draws as much of its list as the terminal has room for once its title and footer are
//! accounted for, with a `...` at whichever end it had to cut. That is
//! [`limit_options`](clackatui_core::limit_options), and it means the height of the terminal is part
//! of what the widget draws — which is why [`Session`]'s draw callback carries one.

use clackatui_core::prompt::{Outcome, Prompt};
use clackatui_core::select::{SelectOption, SelectState, SelectWidget};
use clackatui_core::session::Session;
use clackatui_core::settings::Settings;
use clackatui_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// A `select` Prompt, waiting to be configured and run.
///
/// ```no_run
/// let colour = clackatui::select("Pick a colour")
///     .option("red")
///     .option("green")
///     .interact()?;
/// # Ok::<_, clackatui::ClackError>(())
/// ```
pub struct Select<T> {
	message: String,
	options: Vec<SelectOption<T>>,
	initial_value: Option<T>,
	max_items: Option<usize>,
	show_instructions: bool,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
}

/// Ask for one of a list of options.
pub fn select<T>(message: impl Into<String>) -> Select<T> {
	Select {
		message: message.into(),
		options: Vec::new(),
		initial_value: None,
		max_items: None,
		show_instructions: true,
		theme: None,
		settings: None,
		with_guide: None,
	}
}

impl<T> Select<T> {
	/// An option built elsewhere — the way to give one a hint or to disable it.
	pub fn choice(mut self, option: SelectOption<T>) -> Self {
		self.options.push(option);
		self
	}

	/// Every option at once, replacing anything added so far.
	pub fn options(mut self, options: impl IntoIterator<Item = SelectOption<T>>) -> Self {
		self.options = options.into_iter().collect();
		self
	}

	/// An option with a label of its own, for a value that cannot print itself.
	pub fn labelled(self, value: T, label: impl Into<String>) -> Self {
		self.choice(SelectOption::labelled(value, label))
	}

	/// Which option the list opens on. A value the list does not hold is ignored, as upstream's is.
	pub fn initial_value(mut self, value: T) -> Self {
		self.initial_value = Some(value);
		self
	}

	/// `maxItems`: draw no more than this many options, however tall the terminal is.
	///
	/// Below five it does nothing: upstream clamps the window to a floor of five, on the grounds
	/// that anything shorter is not worth drawing.
	pub fn max_items(mut self, max_items: usize) -> Self {
		self.max_items = Some(max_items);
		self
	}

	/// Whether the `↑/↓ to navigate` footer is drawn under the list. On by default.
	pub fn show_instructions(mut self, show: bool) -> Self {
		self.show_instructions = show;
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
		T: Clone + PartialEq + 'static,
	{
		self.interact_opt()?.ok_or(ClackError::Cancelled)
	}

	/// Ask, and report a cancel as [`None`].
	///
	/// Also `None` for a `select` with no options at all, which upstream allows and answers with
	/// `undefined`: there is nothing under the cursor to hand back.
	pub fn interact_opt(self) -> Result<Option<T>, ClackError>
	where
		T: Clone + PartialEq + 'static,
	{
		let prompt = driver::run(self.session())?;
		Ok(match prompt.outcome() {
			Some(Outcome::Submitted(value)) => value.cloned(),
			_ => None,
		})
	}

	/// The Session this Prompt would be run as, without running it.
	pub fn session(self) -> Session<SelectState<T>>
	where
		T: PartialEq + 'static,
	{
		let mut state = SelectState::new(self.options);
		if let Some(initial) = &self.initial_value {
			state = state.with_initial_value(initial);
		}

		let mut prompt = Prompt::new(state);
		if let Some(settings) = self.settings {
			prompt = prompt.with_settings(settings);
		}

		let message = self.message;
		let theme = self.theme.unwrap_or_else(Theme::clack);
		let (max_items, show_instructions) = (self.max_items, self.show_instructions);
		let with_guide = self.with_guide;

		Session::new(prompt, move |prompt, columns, rows| {
			// Upstream measures both against the Prompt's own output stream, which for a real
			// program is the terminal the Session already knows.
			let mut widget = SelectWidget::new(prompt, &message)
				.with_theme(&theme)
				.with_columns(columns as usize)
				.with_rows(rows as usize)
				.with_instructions(show_instructions);
			if let Some(max_items) = max_items {
				widget = widget.with_max_items(max_items);
			}
			if let Some(with_guide) = with_guide {
				widget = widget.with_guide(with_guide);
			}
			widget.frame()
		})
	}
}

impl<T: std::fmt::Display> Select<T> {
	/// An option labelled by its own value.
	pub fn option(self, value: T) -> Self {
		self.choice(SelectOption::new(value))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use clackatui_core::line_editor::{Key, KeyName};

	fn answer(session: &Session<SelectState<String>>) -> Option<String> {
		match session.outcome() {
			Some(Outcome::Submitted(value)) => value.cloned(),
			_ => None,
		}
	}

	fn colours() -> Select<String> {
		select("Pick a colour")
			.option("red".to_string())
			.option("green".to_string())
			.option("blue".to_string())
	}

	#[test]
	fn return_takes_the_option_the_cursor_is_on() {
		let mut session = colours().session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session).as_deref(), Some("red"));
	}

	#[test]
	fn the_arrows_walk_the_list() {
		let mut session = colours().session();
		session.open();
		session.key(None, &Key::named(KeyName::Down));
		session.key(None, &Key::named(KeyName::Down));
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session).as_deref(), Some("blue"));
	}

	#[test]
	fn an_initial_value_opens_the_list_on_it() {
		let mut session = colours().initial_value("green".to_string()).session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session).as_deref(), Some("green"));
	}

	#[test]
	fn a_disabled_option_is_never_landed_on() {
		let mut session = select("Pick a colour")
			.option("red".to_string())
			.choice(SelectOption::new("green".to_string()).with_disabled(true))
			.option("blue".to_string())
			.session();
		session.open();
		session.key(None, &Key::named(KeyName::Down));
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session).as_deref(), Some("blue"));
	}

	#[test]
	fn the_opening_frame_carries_the_labels_and_the_footer() {
		let mut session = colours().session();
		let opening = session.open();
		for label in ["red", "green", "blue", "to navigate"] {
			assert!(opening.contains(label), "{label} is missing: {opening:?}");
		}
	}

	#[test]
	fn the_footer_can_be_turned_off() {
		let mut session = colours().show_instructions(false).session();
		let opening = session.open();
		assert!(!opening.contains("to navigate"), "{opening:?}");
	}

	/// A list longer than the terminal is cut, and says so with a `...`.
	#[test]
	fn a_long_list_is_cut_to_the_terminal() {
		let options = (0..40).map(|i| SelectOption::new(format!("Option {i}")));
		let mut session = select("Pick one")
			.options(options)
			.session()
			.with_size(80, 12);
		let opening = session.open();
		assert!(opening.contains("..."), "{opening:?}");
		assert!(!opening.contains("Option 39"), "{opening:?}");
	}

	#[test]
	fn a_cancel_is_no_answer_at_all() {
		let mut session = colours().session();
		session.open();
		session.key(None, &Key::named(KeyName::Escape));
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
		assert_eq!(answer(&session), None);
	}

	/// A label of its own, for a value that has no `Display` — the shape a caller with an enum uses.
	#[test]
	fn a_value_can_be_labelled_rather_than_printed() {
		#[derive(Clone, PartialEq)]
		struct Colour(u32);

		let mut session = select::<Colour>("Pick a colour")
			.labelled(Colour(0xff0000), "red")
			.labelled(Colour(0x00ff00), "green")
			.session();
		let opening = session.open();
		assert!(opening.contains("green"), "{opening:?}");
		session.key(None, &Key::named(KeyName::Down));
		session.key(None, &Key::named(KeyName::Return));
		match session.outcome() {
			Some(Outcome::Submitted(Some(colour))) => assert_eq!(colour.0, 0x00ff00),
			other => panic!("{:?}", other.is_some()),
		}
	}
}
