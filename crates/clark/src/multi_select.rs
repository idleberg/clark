//! `multiselect()`, ported from `@clack/prompts`' `multi-select.ts`.
//!
//! [`select`](crate::select) with checkboxes. The arrows walk the list, `space` ticks the option
//! under the cursor, `a` ticks everything and `i` swaps what is ticked for what is not, and `return`
//! hands back every value that is ticked.
//!
//! # `required` is the validator
//!
//! A `multiselect` refuses an empty answer by default, and that refusal is the only validation it
//! has: upstream writes its own `validate` and there is no way to pass another. So there is no
//! `validate` builder here either — [`MultiSelect::required`] is the whole of it, and turning it off
//! makes `return` on an untouched Prompt an answer of no values at all.

use clark_core::multi_select::{MultiSelectState, MultiSelectWidget, required};
use clark_core::prompt::{Outcome, Prompt};
use clark_core::select::SelectOption;
use clark_core::session::Session;
use clark_core::settings::Settings;
use clark_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// A `multiselect` Prompt, waiting to be configured and run.
///
/// ```no_run
/// let toppings = clark::multiselect("Pick your toppings")
///     .option("cheese")
///     .option("basil")
///     .interact()?;
/// # Ok::<_, clark::ClackError>(())
/// ```
pub struct MultiSelect<T> {
	message: String,
	options: Vec<SelectOption<T>>,
	initial_values: Vec<T>,
	cursor_at: Option<T>,
	max_items: Option<usize>,
	required: bool,
	show_instructions: bool,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
}

/// Ask for any number of a list of options.
pub fn multiselect<T>(message: impl Into<String>) -> MultiSelect<T> {
	MultiSelect {
		message: message.into(),
		options: Vec::new(),
		initial_values: Vec::new(),
		cursor_at: None,
		max_items: None,
		required: true,
		show_instructions: true,
		theme: None,
		settings: None,
		with_guide: None,
	}
}

impl<T> MultiSelect<T> {
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

	/// `initialValues`: the boxes that are ticked before a key is pressed.
	pub fn initial_values(mut self, values: impl IntoIterator<Item = T>) -> Self {
		self.initial_values = values.into_iter().collect();
		self
	}

	/// `cursorAt`: which option the list opens on, which need not be one of the ticked ones. A value
	/// the list does not hold is ignored, as upstream's is.
	pub fn cursor_at(mut self, value: T) -> Self {
		self.cursor_at = Some(value);
		self
	}

	/// `maxItems`: draw no more than this many options, however tall the terminal is. Below five it
	/// does nothing — see [`select`](crate::Select::max_items).
	pub fn max_items(mut self, max_items: usize) -> Self {
		self.max_items = Some(max_items);
		self
	}

	/// `required`: whether an empty answer is refused. On by default.
	pub fn required(mut self, required: bool) -> Self {
		self.required = required;
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
	pub fn interact(self) -> Result<Vec<T>, ClackError>
	where
		T: Clone + PartialEq + 'static,
	{
		self.interact_opt()?.ok_or(ClackError::Cancelled)
	}

	/// Ask, and report a cancel as [`None`].
	///
	/// An empty `Vec` is an answer, not the absence of one: it is what a `multiselect` with
	/// [`required`](Self::required) turned off submits when nothing was ticked.
	pub fn interact_opt(self) -> Result<Option<Vec<T>>, ClackError>
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
	pub fn session(self) -> Session<MultiSelectState<T>>
	where
		T: Clone + PartialEq + 'static,
	{
		let mut state =
			MultiSelectState::new(self.options).with_initial_values(self.initial_values);
		if let Some(at) = &self.cursor_at {
			state = state.with_cursor_at(at);
		}

		let mut prompt = Prompt::new(state);
		if let Some(settings) = self.settings {
			prompt = prompt.with_settings(settings);
		}
		if self.required {
			prompt = prompt.with_validator(required);
		}

		let message = self.message;
		let theme = self.theme.unwrap_or_else(Theme::clack);
		let (max_items, show_instructions) = (self.max_items, self.show_instructions);
		let with_guide = self.with_guide;

		Session::new(prompt, move |prompt, columns, rows| {
			let mut widget = MultiSelectWidget::new(prompt, &message)
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

impl<T: std::fmt::Display> MultiSelect<T> {
	/// An option labelled by its own value.
	pub fn option(self, value: T) -> Self {
		self.choice(SelectOption::new(value))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use clark_core::line_editor::{Key, KeyName};
	use clark_core::prompt::Status;

	fn answer(session: &Session<MultiSelectState<String>>) -> Option<Vec<String>> {
		match session.outcome() {
			Some(Outcome::Submitted(value)) => value.cloned(),
			_ => None,
		}
	}

	fn toppings() -> MultiSelect<String> {
		multiselect("Pick your toppings")
			.option("cheese".to_string())
			.option("basil".to_string())
			.option("olives".to_string())
	}

	fn space(session: &mut Session<MultiSelectState<String>>) {
		session.key(Some(" "), &Key::named(KeyName::Char(' ')));
	}

	#[test]
	fn space_then_return_answers_with_what_was_ticked() {
		let mut session = toppings().session();
		session.open();
		space(&mut session);
		session.key(None, &Key::named(KeyName::Down));
		session.key(None, &Key::named(KeyName::Down));
		space(&mut session);
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(
			answer(&session).as_deref(),
			Some(["cheese".to_string(), "olives".to_string()].as_slice())
		);
	}

	/// The default. A bare `return` is refused rather than answered, and the Prompt stays open.
	#[test]
	fn an_empty_answer_is_refused_by_default() {
		let mut session = toppings().session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.status(), Status::Error);
		assert_eq!(answer(&session), None);
	}

	#[test]
	fn required_can_be_turned_off_and_then_nothing_is_an_answer() {
		let mut session = toppings().required(false).session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.status(), Status::Submit);
		assert_eq!(answer(&session).as_deref(), Some([].as_slice()));
	}

	#[test]
	fn initial_values_start_the_boxes_ticked() {
		let mut session = toppings().initial_values(["basil".to_string()]).session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(
			answer(&session).as_deref(),
			Some(["basil".to_string()].as_slice())
		);
	}

	#[test]
	fn cursor_at_opens_the_list_somewhere_other_than_the_top() {
		let mut session = toppings().cursor_at("olives".to_string()).session();
		session.open();
		space(&mut session);
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(
			answer(&session).as_deref(),
			Some(["olives".to_string()].as_slice())
		);
	}

	#[test]
	fn a_ticks_the_whole_list() {
		let mut session = toppings().session();
		session.open();
		session.key(Some("a"), &Key::named(KeyName::Char('a')));
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(answer(&session).map(|v| v.len()), Some(3));
	}

	#[test]
	fn the_opening_frame_carries_the_labels_and_three_instructions() {
		let mut session = toppings().session();
		let opening = session.open();
		for text in [
			"cheese",
			"basil",
			"olives",
			"to navigate",
			"select",
			"confirm",
		] {
			assert!(opening.contains(text), "{text} is missing: {opening:?}");
		}
	}

	#[test]
	fn a_long_list_is_cut_to_the_terminal() {
		let options = (0..40).map(|i| SelectOption::new(format!("Option {i}")));
		let mut session = multiselect("Pick some")
			.options(options)
			.session()
			.with_size(80, 12);
		let opening = session.open();
		assert!(opening.contains("..."), "{opening:?}");
		assert!(!opening.contains("Option 39"), "{opening:?}");
	}

	#[test]
	fn a_cancel_is_no_answer_at_all() {
		let mut session = toppings().session();
		session.open();
		space(&mut session);
		session.key(None, &Key::named(KeyName::Escape));
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
		assert_eq!(answer(&session), None);
	}

	/// A label of its own, for a value with no `Display` — the shape a caller with an enum uses.
	#[test]
	fn a_value_can_be_labelled_rather_than_printed() {
		#[derive(Clone, PartialEq)]
		struct Topping(u32);

		let mut session = multiselect::<Topping>("Pick your toppings")
			.labelled(Topping(1), "cheese")
			.labelled(Topping(2), "basil")
			.session();
		let opening = session.open();
		assert!(opening.contains("basil"), "{opening:?}");
		session.key(Some(" "), &Key::named(KeyName::Char(' ')));
		session.key(None, &Key::named(KeyName::Return));
		match session.outcome() {
			Some(Outcome::Submitted(Some(toppings))) => {
				assert_eq!(toppings.iter().map(|t| t.0).collect::<Vec<_>>(), [1]);
			}
			other => panic!("{:?}", other.is_some()),
		}
	}
}
