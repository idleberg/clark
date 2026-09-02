//! `groupMultiselect()`, ported from `@clack/prompts`' `group-multi-select.ts`.
//!
//! [`multiselect`](crate::multiselect) with headers. Options are added a group at a time, the group
//! itself takes a row of the list, and `space` on that row ticks or unticks everything under it.
//!
//! # The answer never holds a group
//!
//! A group is a heading and a shortcut, not a value. Ticking one ticks its members, and what comes
//! back is their values — so a `groupMultiselect` over `T` answers with a `Vec<T>` and the group
//! names appear nowhere in it. [`GroupMultiSelect::selectable_groups`] turns even the shortcut off,
//! and the cursor then steps over the headers rather than resting on them.

use std::fmt::Display;

use clackatui_core::group_multi_select::{GroupMultiSelectState, GroupMultiSelectWidget};
use clackatui_core::multi_select::required;
use clackatui_core::prompt::{Outcome, Prompt};
use clackatui_core::select::SelectOption;
use clackatui_core::session::Session;
use clackatui_core::settings::Settings;
use clackatui_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// A `groupMultiselect` Prompt, waiting to be configured and run.
///
/// ```no_run
/// let stack = clackatui::group_multiselect("Define your project")
///     .group("Testing", ["jest", "playwright"])
///     .group("Language", ["js", "ts"])
///     .interact()?;
/// # Ok::<_, clackatui::ClackError>(())
/// ```
pub struct GroupMultiSelect<T> {
	message: String,
	groups: Vec<(String, Vec<SelectOption<T>>)>,
	initial_values: Vec<T>,
	cursor_at: Option<T>,
	max_items: Option<usize>,
	required: bool,
	selectable_groups: bool,
	show_instructions: bool,
	group_spacing: isize,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
}

/// Ask for any number of options, arranged in groups.
pub fn group_multiselect<T>(message: impl Into<String>) -> GroupMultiSelect<T> {
	GroupMultiSelect {
		message: message.into(),
		groups: Vec::new(),
		initial_values: Vec::new(),
		cursor_at: None,
		max_items: None,
		required: true,
		selectable_groups: true,
		show_instructions: true,
		group_spacing: 0,
		theme: None,
		settings: None,
		with_guide: None,
	}
}

impl<T> GroupMultiSelect<T> {
	/// A group and the options built for it — the way to give one a hint.
	pub fn choices(
		mut self,
		name: impl Into<String>,
		options: impl IntoIterator<Item = SelectOption<T>>,
	) -> Self {
		self.groups
			.push((name.into(), options.into_iter().collect()));
		self
	}

	/// `initialValues`: the boxes that are ticked before a key is pressed.
	pub fn initial_values(mut self, values: impl IntoIterator<Item = T>) -> Self {
		self.initial_values = values.into_iter().collect();
		self
	}

	/// `cursorAt`: which option the list opens on. A value the list does not hold is ignored, as
	/// upstream's is; a group cannot be named here, for the reason
	/// [`clackatui_core::group_multi_select`] gives.
	pub fn cursor_at(mut self, value: T) -> Self {
		self.cursor_at = Some(value);
		self
	}

	/// `maxItems`: draw no more than this many rows of the list — headers included, since they are
	/// rows of it. Below five it does nothing.
	pub fn max_items(mut self, max_items: usize) -> Self {
		self.max_items = Some(max_items);
		self
	}

	/// `required`: whether an empty answer is refused. On by default.
	pub fn required(mut self, required: bool) -> Self {
		self.required = required;
		self
	}

	/// `selectableGroups`: whether a header can be reached and ticked. On by default; off, and the
	/// cursor steps over every header.
	pub fn selectable_groups(mut self, selectable: bool) -> Self {
		self.selectable_groups = selectable;
		self
	}

	/// Whether the `↑/↓ to navigate` footer is drawn under the list. On by default.
	pub fn show_instructions(mut self, show: bool) -> Self {
		self.show_instructions = show;
		self
	}

	/// `groupSpacing`: blank rows before each group. Zero, and anything below it, draws none.
	pub fn group_spacing(mut self, spacing: isize) -> Self {
		self.group_spacing = spacing;
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
		T: Clone + PartialEq + Display + 'static,
	{
		self.interact_opt()?.ok_or(ClackError::Cancelled)
	}

	/// Ask, and report a cancel as [`None`].
	///
	/// An empty `Vec` is an answer, not the absence of one — see
	/// [`multiselect`](crate::MultiSelect::interact_opt).
	pub fn interact_opt(self) -> Result<Option<Vec<T>>, ClackError>
	where
		T: Clone + PartialEq + Display + 'static,
	{
		let prompt = driver::run(self.session())?;
		Ok(match prompt.outcome() {
			Some(Outcome::Submitted(value)) => value.cloned(),
			_ => None,
		})
	}

	/// The Session this Prompt would be run as, without running it.
	pub fn session(self) -> Session<GroupMultiSelectState<T>>
	where
		T: Clone + PartialEq + 'static,
	{
		let mut state = GroupMultiSelectState::new(self.groups)
			.with_selectable_groups(self.selectable_groups)
			.with_initial_values(self.initial_values);
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
		let (group_spacing, with_guide) = (self.group_spacing, self.with_guide);

		Session::new(prompt, move |prompt, columns, rows| {
			let mut widget = GroupMultiSelectWidget::new(prompt, &message)
				.with_theme(&theme)
				.with_columns(columns as usize)
				.with_rows(rows as usize)
				.with_instructions(show_instructions)
				.with_group_spacing(group_spacing);
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

impl<T: Display> GroupMultiSelect<T> {
	/// A group, and the values under it labelled by themselves.
	pub fn group(self, name: impl Into<String>, values: impl IntoIterator<Item = T>) -> Self {
		let options = values.into_iter().map(SelectOption::new);
		self.choices(name, options.collect::<Vec<_>>())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use clackatui_core::line_editor::{Key, KeyName};
	use clackatui_core::prompt::Status;

	fn project() -> GroupMultiSelect<String> {
		group_multiselect("Define your project")
			.group("Testing", ["jest".to_string(), "playwright".to_string()])
			.group("Language", ["js".to_string(), "ts".to_string()])
	}

	fn answer(session: &Session<GroupMultiSelectState<String>>) -> Option<Vec<String>> {
		match session.outcome() {
			Some(Outcome::Submitted(value)) => value.cloned(),
			_ => None,
		}
	}

	fn press(session: &mut Session<GroupMultiSelectState<String>>, name: KeyName) -> String {
		session.key(None, &Key::named(name))
	}

	fn space(session: &mut Session<GroupMultiSelectState<String>>) -> String {
		session.key(Some(" "), &Key::named(KeyName::Char(' ')))
	}

	#[test]
	fn the_opening_frame_lists_every_group_and_every_option() {
		let mut session = project().session();
		let opening = session.open();
		for text in ["Testing", "jest", "playwright", "Language", "js", "ts"] {
			assert!(opening.contains(text), "{text} is missing: {opening:?}");
		}
	}

	#[test]
	fn a_group_answers_with_its_members() {
		let mut session = project().session();
		session.open();
		space(&mut session);
		press(&mut session, KeyName::Return);
		assert_eq!(
			answer(&session),
			Some(vec!["jest".to_string(), "playwright".to_string()])
		);
	}

	#[test]
	fn an_unselectable_group_cannot_be_ticked_and_cannot_be_reached() {
		let mut session = project().selectable_groups(false).session();
		session.open();
		// The cursor opens on the first option rather than on the header above it.
		space(&mut session);
		press(&mut session, KeyName::Return);
		assert_eq!(answer(&session), Some(vec!["jest".to_string()]));
	}

	#[test]
	fn required_refuses_an_empty_answer() {
		let mut session = project().session();
		session.open();
		press(&mut session, KeyName::Return);
		assert_eq!(session.status(), Status::Error);
		assert!(!session.is_finished());
	}

	#[test]
	fn required_off_lets_an_empty_answer_through() {
		let mut session = project().required(false).session();
		session.open();
		press(&mut session, KeyName::Return);
		assert_eq!(answer(&session), Some(Vec::new()));
	}

	#[test]
	fn initial_values_are_ticked_before_a_key_is_pressed() {
		let mut session = project().initial_values(["ts".to_string()]).session();
		session.open();
		press(&mut session, KeyName::Return);
		assert_eq!(answer(&session), Some(vec!["ts".to_string()]));
	}

	#[test]
	fn cursor_at_opens_the_list_somewhere_else() {
		let mut session = project().cursor_at("js".to_string()).session();
		session.open();
		space(&mut session);
		press(&mut session, KeyName::Return);
		assert_eq!(answer(&session), Some(vec!["js".to_string()]));
	}

	#[test]
	fn a_cancel_is_no_answer_at_all() {
		let mut session = project().session();
		session.open();
		press(&mut session, KeyName::Escape);
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
	}
}
