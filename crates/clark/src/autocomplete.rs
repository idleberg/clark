//! `autocomplete()` and `autocompleteMultiselect()`, ported from `@clack/prompts`'
//! `autocomplete.ts`.
//!
//! A [`select`](crate::select) with a search box in front of it. Type to narrow the list, walk what
//! is left with the arrows, and answer with `return`. The multiple one ticks with `tab` — and with
//! `space` too, but only once the arrows have been used, because until then a space is a space.
//!
//! # Two builders, one state
//!
//! Upstream builds both out of the same `AutocompletePrompt`, and so does this: the difference is
//! `multiple`, and what the two draw for the same selection. [`Autocomplete`] answers with one value
//! and [`AutocompleteMultiSelect`] with any number, which is why they are two types rather than one
//! with a flag — the flag would have to be in the return type.
//!
//! # `filter` and `placeholder` are more than they look
//!
//! The filter decides what the search finds, and the default reads an option's hint and its printed
//! value as well as its label. The placeholder is not only what the empty box says: pressing `tab`
//! on an empty box types it for you, provided it matches something.

use clark_core::autocomplete::{
	AutocompleteMultiSelectWidget, AutocompleteState, AutocompleteWidget, required,
};
use clark_core::prompt::{Outcome, Prompt, Validator};
use clark_core::select::SelectOption;
use clark_core::session::Session;
use clark_core::settings::Settings;
use clark_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// A filter, as both builders take one.
type Filter<T> = Box<dyn Fn(&str, &SelectOption<T>) -> bool>;

/// An `autocomplete` Prompt, waiting to be configured and run.
///
/// ```no_run
/// let framework = clark::autocomplete("Search for a framework")
///     .option("next")
///     .option("astro")
///     .placeholder("Type to search...")
///     .interact()?;
/// # Ok::<_, clark::ClackError>(())
/// ```
pub struct Autocomplete<T> {
	message: String,
	options: Vec<SelectOption<T>>,
	initial_value: Option<T>,
	placeholder: Option<String>,
	max_items: Option<usize>,
	filter: Option<Filter<T>>,
	validator: Option<Box<dyn Validator<T>>>,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
}

/// Search a list of options and answer with one of them.
pub fn autocomplete<T>(message: impl Into<String>) -> Autocomplete<T> {
	Autocomplete {
		message: message.into(),
		options: Vec::new(),
		initial_value: None,
		placeholder: None,
		max_items: None,
		filter: None,
		validator: None,
		theme: None,
		settings: None,
		with_guide: None,
	}
}

impl<T> Autocomplete<T> {
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

	/// `initialValue`: which option the list opens on, chosen. Without one it opens on the first.
	pub fn initial_value(mut self, value: T) -> Self {
		self.initial_value = Some(value);
		self
	}

	/// `placeholder`: what the empty search box says, and what `tab` types into it.
	pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
		self.placeholder = Some(placeholder.into());
		self
	}

	/// `maxItems`: draw no more than this many options, however tall the terminal is.
	pub fn max_items(mut self, max_items: usize) -> Self {
		self.max_items = Some(max_items);
		self
	}

	/// `filter`: what the search matches, in place of
	/// [`default_filter`](clark_core::autocomplete::default_filter).
	pub fn filter(mut self, filter: impl Fn(&str, &SelectOption<T>) -> bool + 'static) -> Self {
		self.filter = Some(Box::new(filter));
		self
	}

	/// `validate`: refuse an answer, with a message. Runs on `return`, against the chosen value —
	/// which is nothing at all when the search matched nothing.
	pub fn validate(mut self, validator: impl Validator<T> + 'static) -> Self {
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
	pub fn interact(self) -> Result<T, ClackError>
	where
		T: Clone + PartialEq + 'static,
	{
		self.interact_opt()?.ok_or(ClackError::Cancelled)
	}

	/// Ask, and report a cancel as [`None`].
	///
	/// Also [`None`] where the Prompt settled on nothing, which a search matching nothing does: the
	/// list it answers from is empty, so there is no option to hand back.
	pub fn interact_opt(self) -> Result<Option<T>, ClackError>
	where
		T: Clone + PartialEq + 'static,
	{
		let prompt = driver::run(self.session())?;
		Ok(match prompt.outcome() {
			Some(Outcome::Submitted(Some(values))) => values.first().cloned(),
			_ => None,
		})
	}

	/// The Session this Prompt would be run as, without running it.
	pub fn session(self) -> Session<AutocompleteState<T>>
	where
		T: Clone + PartialEq + 'static,
	{
		let mut state = match self.filter {
			Some(filter) => AutocompleteState::with_filter(self.options, move |search, option| {
				filter(search, option)
			}),
			None => AutocompleteState::with_filter(self.options, default_filter_for),
		};
		if let Some(placeholder) = &self.placeholder {
			state = state.with_placeholder(placeholder);
		}
		if let Some(initial) = self.initial_value {
			state = state.with_initial_values([initial]);
		}

		let mut prompt = Prompt::new(state);
		if let Some(settings) = self.settings {
			prompt = prompt.with_settings(settings);
		}
		// The state answers with a selection, and a single `autocomplete` answers with the first of
		// it — so a validator written against one value is handed the first of the other.
		if let Some(mut validator) = self.validator {
			prompt = prompt
				.with_validator(move |values: Option<&Vec<T>>| validator.validate(values?.first()));
		}

		let message = self.message;
		let theme = self.theme.unwrap_or_else(Theme::clack);
		let placeholder = self.placeholder;
		let max_items = self.max_items;
		let with_guide = self.with_guide;

		Session::new(prompt, move |prompt, columns, rows| {
			let mut widget = AutocompleteWidget::new(prompt, &message)
				.with_theme(&theme)
				.with_columns(columns as usize)
				.with_rows(rows as usize);
			if let Some(placeholder) = &placeholder {
				widget = widget.with_placeholder(placeholder);
			}
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

impl<T: std::fmt::Display> Autocomplete<T> {
	/// An option labelled by its own value.
	pub fn option(self, value: T) -> Self {
		self.choice(SelectOption::new(value))
	}
}

/// An `autocompleteMultiselect` Prompt, waiting to be configured and run.
///
/// ```no_run
/// let frameworks = clark::autocomplete_multiselect("Select frameworks")
///     .option("next")
///     .option("astro")
///     .required(true)
///     .interact()?;
/// # Ok::<_, clark::ClackError>(())
/// ```
pub struct AutocompleteMultiSelect<T> {
	message: String,
	options: Vec<SelectOption<T>>,
	initial_values: Vec<T>,
	placeholder: Option<String>,
	max_items: Option<usize>,
	required: bool,
	filter: Option<Filter<T>>,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
}

/// Search a list of options and answer with any number of them.
pub fn autocomplete_multiselect<T>(message: impl Into<String>) -> AutocompleteMultiSelect<T> {
	AutocompleteMultiSelect {
		message: message.into(),
		options: Vec::new(),
		initial_values: Vec::new(),
		placeholder: None,
		max_items: None,
		required: false,
		filter: None,
		theme: None,
		settings: None,
		with_guide: None,
	}
}

impl<T> AutocompleteMultiSelect<T> {
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

	/// `placeholder`: what the empty search box says, and what `tab` types into it.
	///
	/// Careful here: with a placeholder that matches something, `tab` on an empty box fills the box
	/// rather than ticking the option under the cursor. That is upstream's rule and it is the reason
	/// its own test for this ends with nothing selected.
	pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
		self.placeholder = Some(placeholder.into());
		self
	}

	/// `maxItems`: draw no more than this many options, however tall the terminal is.
	pub fn max_items(mut self, max_items: usize) -> Self {
		self.max_items = Some(max_items);
		self
	}

	/// `required`: whether an empty answer is refused. **Off** by default — where a
	/// [`multiselect`](crate::multiselect)'s is on, and where its message is worded differently too.
	pub fn required(mut self, required: bool) -> Self {
		self.required = required;
		self
	}

	/// `filter`: what the search matches, in place of
	/// [`default_filter`](clark_core::autocomplete::default_filter).
	pub fn filter(mut self, filter: impl Fn(&str, &SelectOption<T>) -> bool + 'static) -> Self {
		self.filter = Some(Box::new(filter));
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

	/// Ask, and report a cancel as [`None`]. An empty `Vec` is an answer, not the absence of one.
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
	pub fn session(self) -> Session<AutocompleteState<T>>
	where
		T: Clone + PartialEq + 'static,
	{
		let mut state = match self.filter {
			Some(filter) => AutocompleteState::with_filter(self.options, move |search, option| {
				filter(search, option)
			}),
			None => AutocompleteState::with_filter(self.options, default_filter_for),
		}
		.with_multiple(true)
		.with_initial_values(self.initial_values);
		if let Some(placeholder) = &self.placeholder {
			state = state.with_placeholder(placeholder);
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
		let placeholder = self.placeholder;
		let max_items = self.max_items;
		let with_guide = self.with_guide;

		Session::new(prompt, move |prompt, columns, rows| {
			let mut widget = AutocompleteMultiSelectWidget::new(prompt, &message)
				.with_theme(&theme)
				.with_columns(columns as usize)
				.with_rows(rows as usize);
			if let Some(placeholder) = &placeholder {
				widget = widget.with_placeholder(placeholder);
			}
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

impl<T: std::fmt::Display> AutocompleteMultiSelect<T> {
	/// An option labelled by its own value.
	pub fn option(self, value: T) -> Self {
		self.choice(SelectOption::new(value))
	}
}

/// [`default_filter`](clark_core::autocomplete::default_filter) for a value that cannot print
/// itself: the label and the hint, which is all there is to search.
///
/// A builder takes its options before it knows whether `T` can be printed, so the state cannot be
/// built through `AutocompleteState::new` — that one is the `Display` half of the same function.
fn default_filter_for<T>(search: &str, option: &SelectOption<T>) -> bool {
	if search.is_empty() {
		return true;
	}
	let term = search.to_lowercase();
	option.label().to_lowercase().contains(&term)
		|| option
			.hint()
			.unwrap_or_default()
			.to_lowercase()
			.contains(&term)
}

#[cfg(test)]
mod tests {
	use super::*;
	use clark_core::line_editor::{Key, KeyName};
	use clark_core::prompt::Status;

	fn frameworks() -> Autocomplete<String> {
		autocomplete("Search for a framework")
			.option("next".to_string())
			.option("astro".to_string())
			.option("svelte".to_string())
	}

	fn typed<T: Clone + PartialEq>(session: &mut Session<AutocompleteState<T>>, text: &str) {
		for c in text.chars() {
			session.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
		}
	}

	fn one(session: &Session<AutocompleteState<String>>) -> Option<String> {
		match session.outcome() {
			Some(Outcome::Submitted(Some(values))) => values.first().cloned(),
			_ => None,
		}
	}

	fn many(session: &Session<AutocompleteState<String>>) -> Option<Vec<String>> {
		match session.outcome() {
			Some(Outcome::Submitted(value)) => value.cloned(),
			_ => None,
		}
	}

	#[test]
	fn typing_narrows_the_list_and_return_answers_with_what_is_left() {
		let mut session = frameworks().session();
		session.open();
		typed(&mut session, "sv");
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(one(&session).as_deref(), Some("svelte"));
	}

	#[test]
	fn a_bare_return_answers_with_the_first_option() {
		let mut session = frameworks().session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(one(&session).as_deref(), Some("next"));
	}

	#[test]
	fn an_initial_value_opens_the_list_on_it() {
		let mut session = frameworks().initial_value("astro".to_string()).session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(one(&session).as_deref(), Some("astro"));
	}

	#[test]
	fn the_opening_frame_carries_the_search_box_and_three_instructions() {
		let mut session = frameworks().session();
		let opening = session.open();
		for text in [
			"Search:",
			"next",
			"astro",
			"to select",
			"confirm",
			"to search",
		] {
			assert!(opening.contains(text), "{text} is missing: {opening:?}");
		}
	}

	#[test]
	fn a_placeholder_is_shown_and_tab_types_it_for_you() {
		let mut session = frameworks().placeholder("astro".to_string()).session();
		let opening = session.open();
		assert!(opening.contains("astro"), "{opening:?}");
		session.key(Some("\t"), &Key::named(KeyName::Tab));
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(one(&session).as_deref(), Some("astro"));
	}

	#[test]
	fn a_filter_of_ones_own_decides_what_the_search_finds() {
		let mut session = frameworks()
			.filter(|search, option| option.label().ends_with(search))
			.session();
		session.open();
		typed(&mut session, "o");
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(one(&session).as_deref(), Some("astro"));
	}

	#[test]
	fn a_validator_sees_the_one_value_rather_than_the_selection() {
		let mut session = frameworks()
			.validate(|value: Option<&String>| {
				(value.map(String::as_str) == Some("next")).then(|| "not that one".to_string())
			})
			.session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.status(), Status::Error);
		typed(&mut session, "sv");
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(one(&session).as_deref(), Some("svelte"));
	}

	#[test]
	fn a_search_that_matches_nothing_answers_with_nothing() {
		let mut session = frameworks().session();
		session.open();
		typed(&mut session, "zzz");
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.status(), Status::Submit);
		assert_eq!(one(&session), None);
	}

	#[test]
	fn a_cancel_is_no_answer_at_all() {
		let mut session = frameworks().session();
		session.open();
		session.key(None, &Key::named(KeyName::Escape));
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
	}

	// --- The multiple one -----------------------------------------------------------------------

	fn several() -> AutocompleteMultiSelect<String> {
		autocomplete_multiselect("Select frameworks")
			.option("next".to_string())
			.option("astro".to_string())
			.option("svelte".to_string())
	}

	#[test]
	fn tab_ticks_and_return_answers_with_everything_ticked() {
		let mut session = several().session();
		session.open();
		session.key(None, &Key::named(KeyName::Tab));
		session.key(None, &Key::named(KeyName::Down));
		session.key(Some(""), &Key::named(KeyName::Char(' ')));
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(
			many(&session).as_deref(),
			Some(["next".to_string(), "astro".to_string()].as_slice())
		);
	}

	/// Off by default, which is the other way round from a `multiselect`.
	#[test]
	fn an_empty_answer_is_allowed_unless_required_is_asked_for() {
		let mut session = several().session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.status(), Status::Submit);
		assert_eq!(many(&session).as_deref(), Some([].as_slice()));

		let mut session = several().required(true).session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.status(), Status::Error);
	}

	#[test]
	fn initial_values_start_the_boxes_ticked() {
		let mut session = several().initial_values(["svelte".to_string()]).session();
		session.open();
		session.key(None, &Key::named(KeyName::Return));
		assert_eq!(
			many(&session).as_deref(),
			Some(["svelte".to_string()].as_slice())
		);
	}

	#[test]
	fn the_opening_frame_counts_four_instructions() {
		let mut session = several().session();
		let opening = session.open();
		for text in ["Search:", "to navigate", "Tab:", "confirm", "to search"] {
			assert!(opening.contains(text), "{text} is missing: {opening:?}");
		}
	}

	/// A label of its own, for a value with no `Display`.
	#[test]
	fn a_value_can_be_labelled_rather_than_printed() {
		#[derive(Clone, PartialEq)]
		struct Framework(u32);

		let mut session = autocomplete_multiselect::<Framework>("Select frameworks")
			.labelled(Framework(1), "next")
			.labelled(Framework(2), "astro")
			.session();
		session.open();
		typed(&mut session, "as");
		session.key(None, &Key::named(KeyName::Tab));
		session.key(None, &Key::named(KeyName::Return));
		match session.outcome() {
			Some(Outcome::Submitted(Some(picked))) => {
				assert_eq!(picked.iter().map(|f| f.0).collect::<Vec<_>>(), [2]);
			}
			other => panic!("{:?}", other.is_some()),
		}
	}
}
