//! Ported from `@clack/core`'s `prompts/group-multiselect.ts` and `@clack/prompts`'
//! `group-multi-select.ts`.
//!
//! [`multi_select`](crate::multi_select) with headers in the list. A map of group name to options is
//! flattened into one list — a row for the group, then a row for each of its options — and the
//! cursor walks all of them. The selection, the validator, the instruction footer and the error
//! Frame are `multiselect`'s and are imported from it rather than written again; what is new is the
//! flattening, a `space` on a header that ticks the whole group, and the branch drawn down the left
//! of each group.
//!
//! # A list whose rows are not all the same kind
//!
//! Upstream flattens to `{ value: key, group: true, label: key }` for a header and `{ ...opt, group:
//! key }` for an option, so a header's *value* is the group's name. Two comparisons upstream makes
//! are therefore between a name and a `Value`: `cursorAt` against every row, and the selection
//! against every row. In JavaScript those are `===` between a string and whatever `Value` is, and
//! they are false for every `Value` that is not the very string naming a group. Here they cannot be
//! written at all — a [`Row::Group`] holds no `T` — so they are simply the false they almost always
//! are. The exception, a `Value` of `String` that equals one of the group names, is the one place
//! this port and upstream can disagree, and nothing in either suite goes there.
//!
//! # Three things reproduced rather than corrected
//!
//! - **An empty group draws as selected.** `isGroupSelected` is `items.every(…)`, and every array
//!   method of that shape is true for no items. So a group with nothing under it opens ticked, and
//!   `space` on it ticks nothing and leaves it ticked.
//! - **The wrap width is charged for characters that draw nothing.** Each option is wrapped by
//!   `wrapTextWithPrefix`, which subtracts `prefix.length` from the terminal's width — and the
//!   prefix it is handed has already been through `styleText`. A dim `│ ` is eleven characters and
//!   two columns, so an option under a selectable group wraps nine columns earlier than one under a
//!   group the cursor is on, whose prefix is passed unstyled. See [`DIM_ESCAPES`].
//! - **`groupSpacing` is charged for its newlines too.** The blank rows before a group are a
//!   `'\n'.repeat(n)` on the front of the same prefix, so they come off the width as well — and they
//!   are drawn again before *every* row of a header that wraps, not only its first.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::Style;
use ratatui_core::widgets::Widget;

use crate::frame::{Frame, Line, Span};
use crate::limit_options::LimitOptions;
use crate::multi_select::{VALUE_SEPARATOR, error_footer, footer};
use crate::prompt::{Prompt, PromptState, Status};
use crate::select::{GUIDE_PREFIX_LENGTH, SelectOption, plain_title};
use crate::settings::Action;
use crate::theme::Theme;

/// The characters `styleText('dim', …)` wraps its argument in: `ESC[2m` and `ESC[22m`, four and
/// five.
///
/// They draw nothing and are counted anyway — see the module docs. Nine even around an empty
/// string, which is what a group header's prefix is.
pub const DIM_ESCAPES: usize = 9;

/// One row of the flattened list: a group's name, or one of its options.
#[derive(Clone, Debug, PartialEq)]
pub enum Row<T> {
	/// `{ value: key, group: true, label: key }` — the header, drawn with the group's name as its
	/// label.
	Group(String),
	/// `{ ...opt, group: key }` — an option, and the group it was listed under.
	Item {
		option: SelectOption<T>,
		group: String,
	},
}

impl<T> Row<T> {
	/// What this row is drawn as: a group's name, or an option's label.
	pub fn label(&self) -> &str {
		match self {
			Row::Group(name) => name,
			Row::Item { option, .. } => option.label(),
		}
	}

	/// The option this row holds, if it is not a header.
	pub fn option(&self) -> Option<&SelectOption<T>> {
		match self {
			Row::Group(_) => None,
			Row::Item { option, .. } => Some(option),
		}
	}

	/// `typeof option.group === 'string'`.
	pub fn is_item(&self) -> bool {
		matches!(self, Row::Item { .. })
	}
}

/// The state of a `groupMultiselect`: the flattened list, a cursor over all of it, and the values
/// that have been ticked.
#[derive(Clone, Debug)]
pub struct GroupMultiSelectState<T> {
	rows: Vec<Row<T>>,
	cursor: usize,
	/// In the order they were ticked, as `multiselect`'s are — never a group's name, since ticking a
	/// group ticks its members instead.
	selected: Vec<T>,
	selectable_groups: bool,
	/// `cursorAt` resolved to a row, kept because the floor under it moves with
	/// [`with_selectable_groups`](Self::with_selectable_groups).
	cursor_at: Option<usize>,
}

impl<T> GroupMultiSelectState<T> {
	/// The groups in the order `Object.entries` hands them over — which is the order they were
	/// written in, for the string keys a `groupMultiselect` has.
	pub fn new(groups: impl IntoIterator<Item = (String, Vec<SelectOption<T>>)>) -> Self {
		let mut rows = Vec::new();
		for (name, options) in groups {
			rows.push(Row::Group(name.clone()));
			for option in options {
				rows.push(Row::Item {
					option,
					group: name.clone(),
				});
			}
		}

		let mut state = Self {
			rows,
			cursor: 0,
			selected: Vec::new(),
			selectable_groups: true,
			cursor_at: None,
		};
		state.settle_cursor();
		state
	}

	/// `selectableGroups`: whether a header can be reached and ticked. On by default.
	pub fn with_selectable_groups(mut self, selectable_groups: bool) -> Self {
		self.selectable_groups = selectable_groups;
		self.settle_cursor();
		self
	}

	/// `initialValues`: the values ticked before a key is pressed. Copied verbatim, duplicates and
	/// values the list does not hold included — as `multiselect`'s are.
	pub fn with_initial_values(mut self, values: impl IntoIterator<Item = T>) -> Self {
		self.selected = values.into_iter().collect();
		self
	}

	/// `cursorAt`: open with the cursor on the option holding this value.
	///
	/// A value the list does not hold leaves the cursor where the floor puts it, because upstream
	/// takes `Math.max` of the `-1` from `findIndex` rather than testing it. A group cannot be named
	/// here — see the module docs.
	pub fn with_cursor_at(mut self, value: &T) -> Self
	where
		T: PartialEq,
	{
		self.cursor_at = self
			.rows
			.iter()
			.position(|row| row.option().is_some_and(|option| option.value() == value));
		self.settle_cursor();
		self
	}

	pub fn cursor(&self) -> usize {
		self.cursor
	}

	pub fn rows(&self) -> &[Row<T>] {
		&self.rows
	}

	pub fn selectable_groups(&self) -> bool {
		self.selectable_groups
	}

	/// The values ticked so far, in the order they were ticked.
	pub fn selected(&self) -> &[T] {
		&self.selected
	}

	/// The ticked options, in the order the list draws them. Headers are never among them.
	pub fn chosen(&self) -> impl Iterator<Item = &SelectOption<T>>
	where
		T: PartialEq,
	{
		self.rows
			.iter()
			.filter_map(Row::option)
			.filter(|option| self.selected.contains(option.value()))
	}

	/// Whether a row is drawn ticked: an option that is held, or a group all of whose options are.
	pub fn is_selected(&self, row: &Row<T>) -> bool
	where
		T: PartialEq,
	{
		match row {
			Row::Group(name) => self.is_group_selected(name),
			Row::Item { option, .. } => self.selected.contains(option.value()),
		}
	}

	/// `isGroupSelected`: every one of the group's options is held.
	///
	/// True for a group with no options at all — see the module docs.
	pub fn is_group_selected(&self, group: &str) -> bool
	where
		T: PartialEq,
	{
		self.items(group)
			.all(|option| self.selected.contains(option.value()))
	}

	/// `getGroupItems`.
	fn items<'a>(&'a self, group: &'a str) -> impl Iterator<Item = &'a SelectOption<T>> {
		self.rows.iter().filter_map(move |row| match row {
			Row::Item { option, group: at } if at == group => Some(option),
			_ => None,
		})
	}

	/// `Math.max(findIndex(…), selectableGroups ? 0 : 1)`.
	///
	/// The floor is a floor and not a fallback upstream, where `findIndex` can return zero: a header
	/// whose *name* is the `cursorAt` sits at row zero, and the floor raises past it. It cannot
	/// return zero here, because a header holds no value to match — so `unwrap_or(0).max(floor)` and
	/// `unwrap_or(floor)` are the same function, and nothing can tell them apart. Written the way
	/// upstream writes it.
	fn settle_cursor(&mut self) {
		let floor = usize::from(!self.selectable_groups);
		self.cursor = self.cursor_at.unwrap_or(0).max(floor);
	}

	/// `toggleValue`: tick what the cursor is on, or untick it.
	fn toggle(&mut self)
	where
		T: Clone + PartialEq,
	{
		let Some(row) = self.rows.get(self.cursor) else {
			return;
		};

		match row {
			Row::Group(name) => {
				let items: Vec<T> = self.items(name).map(|o| o.value().clone()).collect();
				if self.is_group_selected(name) {
					self.selected.retain(|held| !items.contains(held));
				} else {
					self.selected.extend(items);
				}
				// `Array.from(new Set(this.value))`, which is the whole selection and not only what
				// was just added: a duplicate an `initialValues` carried in is dropped here too, and
				// the first of each pair is the one that stays.
				let mut seen = Vec::new();
				self.selected.retain(|value| {
					let fresh = !seen.contains(value);
					if fresh {
						seen.push(value.clone());
					}
					fresh
				});
			}
			Row::Item { option, .. } => {
				let value = option.value();
				match self.selected.iter().position(|held| held == value) {
					Some(at) => {
						self.selected.remove(at);
					}
					None => self.selected.push(value.clone()),
				}
			}
		}
	}

	/// One step of the cursor, and a second one if the first landed on a header that cannot be
	/// ticked.
	///
	/// Only ever a second one. Two headers in a row is what it would take for that not to be enough,
	/// and the flattening cannot produce it — but a list of nothing but empty groups can, and there
	/// the cursor comes to rest on a header regardless. Upstream's `if`, not a loop, reproduced.
	fn walk(&mut self, forward: bool) {
		let step = |cursor: usize, len: usize| -> usize {
			if forward {
				if cursor == len - 1 { 0 } else { cursor + 1 }
			} else if cursor == 0 {
				len - 1
			} else {
				cursor - 1
			}
		};

		let len = self.rows.len();
		if len == 0 {
			return;
		}
		self.cursor = step(self.cursor, len);
		if !self.selectable_groups && self.rows.get(self.cursor).is_some_and(|r| !r.is_item()) {
			self.cursor = step(self.cursor, len);
		}
	}
}

impl<T: Clone + PartialEq> PromptState for GroupMultiSelectState<T> {
	type Value = Vec<T>;

	/// `super(opts, false)`, as every list Prompt's is.
	const TRACKS_INPUT: bool = false;

	/// No `find_cursor` here: a `groupMultiselect` has no notion of a disabled option — upstream's
	/// `Option` carries the field and this Prompt's cursor never asks — so the walk is upstream's own
	/// two lines and wraps at both ends.
	fn cursor(&mut self, action: Action) {
		match action {
			Action::Left | Action::Up => self.walk(false),
			Action::Down | Action::Right => self.walk(true),
			Action::Space => self.toggle(),
			_ => {}
		}
	}

	/// Always a list: upstream's constructor assigns one from `initialValues ?? []` before any key
	/// arrives, so the validator sees an empty array rather than nothing.
	fn value(&self) -> Option<&Vec<T>> {
		Some(&self.selected)
	}
}

/// How one row is drawn — upstream's eight `state` strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Look {
	Inactive,
	Active,
	Selected,
	ActiveSelected,
	/// An option of the group the cursor's header names. Its prefix is drawn undimmed, which is the
	/// only thing that marks the group out.
	GroupActive,
	GroupActiveSelected,
}

/// A `groupMultiselect` Prompt drawn as a Frame — the `render` callback of `@clack/prompts`'
/// `groupMultiselect()`.
pub struct GroupMultiSelectWidget<'a, T: Clone + PartialEq> {
	prompt: &'a Prompt<GroupMultiSelectState<T>>,
	message: &'a str,
	theme: &'a Theme,
	/// The Prompt's own output stream. See [`select`](crate::select)'s module docs.
	columns: usize,
	rows: usize,
	max_items: Option<usize>,
	show_instructions: bool,
	group_spacing: usize,
	with_guide: Option<bool>,
}

impl<'a, T: Clone + PartialEq> GroupMultiSelectWidget<'a, T> {
	pub fn new(prompt: &'a Prompt<GroupMultiSelectState<T>>, message: &'a str) -> Self {
		Self {
			prompt,
			message,
			theme: &THEME,
			columns: 80,
			rows: 20,
			max_items: None,
			show_instructions: true,
			group_spacing: 0,
			with_guide: None,
		}
	}

	pub fn with_theme(mut self, theme: &'a Theme) -> Self {
		self.theme = theme;
		self
	}

	pub fn with_columns(mut self, columns: usize) -> Self {
		self.columns = columns;
		self
	}

	pub fn with_rows(mut self, rows: usize) -> Self {
		self.rows = rows;
		self
	}

	pub fn with_max_items(mut self, max_items: usize) -> Self {
		self.max_items = Some(max_items);
		self
	}

	pub fn with_instructions(mut self, show: bool) -> Self {
		self.show_instructions = show;
		self
	}

	/// `groupSpacing`: blank rows before each group. Zero, and anything below it, draws none —
	/// upstream asks `groupSpacing > 0`, so a negative number is not an error and not an indent.
	pub fn with_group_spacing(mut self, spacing: isize) -> Self {
		self.group_spacing = spacing.max(0) as usize;
		self
	}

	pub fn with_guide(mut self, with_guide: bool) -> Self {
		self.with_guide = Some(with_guide);
		self
	}

	fn guided(&self) -> bool {
		self.with_guide.unwrap_or(self.prompt.settings().with_guide)
	}
}

impl<T: Clone + PartialEq> GroupMultiSelectWidget<'_, T> {
	/// The Frame, branch for branch as upstream's `render` writes it.
	pub fn frame(&self) -> Frame {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let guide = self.guided();
		let status = self.prompt.status();

		let mut frame = Frame::new();
		let title = plain_title(self.theme, self.message, status, guide);
		let title_rows = title.len() + 1;
		for line in title {
			frame.push(line);
		}

		match status {
			// One row, however many values are on it: neither branch wraps, so a settled
			// `groupMultiselect` is the one list Prompt that can write past the right margin and be
			// wrapped by `Prompt.render` instead of by itself.
			Status::Submit | Status::Cancel => {
				let cancelled = status == Status::Cancel;
				let style = if cancelled {
					styles.cancelled
				} else {
					styles.submitted
				};

				let mut values = Line::blank();
				for option in self.prompt.state().chosen() {
					if !values.spans.is_empty() {
						values.push(Span::styled(VALUE_SEPARATOR, styles.separator));
					}
					values.push(Span::styled(option.label(), style));
				}
				// The two branches ask different questions of the same emptiness. A submitted one
				// counts the options — nothing chosen, and neither the value nor the two spaces
				// before it are written. A cancelled one trims the joined labels, so an option
				// labelled with spaces reads as nothing chosen; but its two spaces are part of the
				// Guide's prefix and are written whether or not anything follows them.
				let nothing = values.spans.is_empty();
				let blank = values.spans.iter().all(|span| span.text.trim().is_empty());

				let mut line = Line::blank();
				if guide {
					line.push(Span::styled(symbols.bar, styles.guide));
					if cancelled {
						line.push(Span::raw("  "));
					}
				}
				if if cancelled { !blank } else { !nothing } {
					if !cancelled {
						line.push(Span::raw("  "));
					}
					for span in values.spans {
						line.push(span);
					}
				}
				for paragraph in line.paragraphs() {
					frame.push(paragraph);
				}

				// A cancelled Frame closes with a bar of its own, and only when it had something to
				// close under. A submitted one never does.
				if cancelled && !blank && guide {
					frame.push(Line::from(Span::styled(symbols.bar, styles.guide)));
				}
			}

			Status::Error => {
				let footer = error_footer(self.theme, self.prompt.error(), guide);
				let row_padding = title_rows + footer.len() + 1;
				let bar = Span::styled(symbols.bar, styles.guide_error);
				for line in self.list(guide.then_some(bar), row_padding) {
					frame.push(line);
				}
				for line in footer {
					frame.push(line);
				}
				frame.push(Line::blank());
			}

			_ => {
				let footer = footer(self.theme, guide, self.show_instructions);
				let row_padding = title_rows + footer.len() + 1;
				let bar = Span::styled(symbols.bar, styles.guide_active);
				for line in self.list(guide.then_some(bar), row_padding) {
					frame.push(line);
				}
				if footer.is_empty() {
					frame.push(Line::blank());
				}
				for line in footer {
					frame.push(line);
				}
				frame.push(Line::blank());
			}
		}

		frame
	}

	/// The list, cut to what is left of the terminal, each row behind `bar` if there is one.
	///
	/// The window is walked by index rather than by row, because what a row looks like depends on
	/// where it sits: whether the next row starts a new group, and whether the cursor is on its
	/// header. Upstream reaches for `options.indexOf(option)` to ask the same question.
	fn list(&self, bar: Option<Span>, row_padding: usize) -> Vec<Line> {
		let state = self.prompt.state();
		let indices: Vec<usize> = (0..state.rows().len()).collect();
		let column_padding = if bar.is_some() {
			GUIDE_PREFIX_LENGTH
		} else {
			0
		};

		let mut limit = LimitOptions::new(&indices, state.cursor())
			.with_columns(self.columns)
			.with_rows(self.rows)
			.with_column_padding(column_padding)
			.with_row_padding(row_padding)
			.with_theme(self.theme);
		if let Some(max_items) = self.max_items {
			limit = limit.with_max_items(max_items);
		}

		limit
			.lines(|index, active| self.option(*index, active))
			.into_iter()
			.map(|row| {
				let mut line = Line::blank();
				// A blank row from `groupSpacing` gets the Guide too. Upstream's newlines are inside
				// a prefix, but `limitOptions` wraps that whole string and splits it, so they are
				// rows of the array by the time the Guide is joined onto every one of them.
				if let Some(bar) = &bar {
					line.push(bar.clone());
					line.push(Span::raw("  "));
				}
				for span in row.spans {
					line.push(span);
				}
				line
			})
			.collect()
	}

	/// `styleOption` and `opt` together: one row, as the rows it occupies before the list wraps them.
	fn option(&self, index: usize, active: bool) -> Vec<Line> {
		let state = self.prompt.state();
		let Some(row) = state.rows().get(index) else {
			return vec![Line::blank()];
		};

		let selected = state.is_selected(row);
		// `this.options[this.cursor]?.value === option.group`: true only for an option whose header
		// the cursor is on, since a header's value is the group's name.
		let group_active = !active
			&& match (row, state.rows().get(state.cursor())) {
				(Row::Item { group, .. }, Some(Row::Group(name))) => group == name,
				_ => false,
			};

		let look = match (group_active, active, selected) {
			(true, _, true) => Look::GroupActiveSelected,
			(true, _, false) => Look::GroupActive,
			(false, true, true) => Look::ActiveSelected,
			(false, true, false) => Look::Active,
			(false, false, true) => Look::Selected,
			(false, false, false) => Look::Inactive,
		};

		self.rows_for(index, row, look)
	}

	/// `wrapTextWithPrefix(output, label, prefix, startPrefix, endPrefix, formatter)`.
	fn rows_for(&self, index: usize, row: &Row<T>, look: Look) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let state = self.prompt.state();
		let item = row.is_item();

		// `isLast`: the next row starts a new group, or there is no next row.
		let last_of_group = item
			&& state
				.rows()
				.get(index + 1)
				.is_none_or(|next| !next.is_item());
		let (prefix, prefix_end) = match (item, state.selectable_groups()) {
			(false, _) => ("", ""),
			// Without selectable groups there is no branch to draw, and no closing corner either:
			// upstream leaves `prefixEnd` at the empty string it was declared with, so the rows of a
			// wrapped option after the first are indented two columns less than the first.
			(true, false) => ("  ", ""),
			(true, true) if last_of_group => (symbols.bar_end, "  "),
			(true, true) => (symbols.bar, symbols.bar),
		};
		// `${S_BAR} `, and `'  '` is already two.
		let pad = |prefix: &str| {
			if prefix.is_empty() || prefix == "  " {
				prefix.to_owned()
			} else {
				format!("{prefix} ")
			}
		};
		let (prefix, prefix_end) = (pad(prefix), pad(prefix_end));

		// A group the cursor is on is the one look whose prefix is passed unstyled — which is what
		// distinguishes it, and what makes it wrap nine columns later than the others.
		let dimmed = !matches!(look, Look::GroupActive | Look::GroupActiveSelected);
		let prefix_style = if dimmed {
			styles.option_unselected
		} else {
			Style::new()
		};
		let spacing = if !item { self.group_spacing } else { 0 };

		let checkbox = match look {
			Look::Active | Look::GroupActive => {
				Some((symbols.checkbox_active, styles.checkbox_active))
			}
			Look::ActiveSelected | Look::GroupActiveSelected => {
				Some((symbols.checkbox_selected, styles.checkbox_selected))
			}
			// A header with unselectable groups has no box at all — nothing about it can be ticked,
			// so nothing is drawn to say it is not.
			Look::Selected if item || state.selectable_groups() => {
				Some((symbols.checkbox_selected, styles.checkbox_selected))
			}
			Look::Inactive if item || state.selectable_groups() => {
				Some((symbols.checkbox_inactive, styles.checkbox_inactive))
			}
			_ => None,
		};

		// The label, and the hint where there is one. An option that is neither ticked nor anywhere
		// near the cursor has its hint dropped, which is `select`'s rule; a ticked one keeps it and
		// draws it in the same dim as the label rather than as a hint.
		let (label_style, hint_style) = match look {
			Look::Active | Look::ActiveSelected => (styles.message, Some(styles.hint)),
			// A ticked option's hint goes through the same `dim` formatter as its label rather than
			// being styled as a hint — which is the same `Style` either way, so only the reading
			// says so.
			Look::Selected => (styles.option_selected, Some(styles.option_selected)),
			_ => (styles.option_unselected, None),
		};
		let mut text = Line::from(Span::styled(row.label(), label_style));
		if let Some(hint) = hint_style.zip(row.option().and_then(SelectOption::hint)) {
			text.push(Span::raw(" "));
			text.push(Span::styled(format!("({})", hint.1), hint.0));
		}

		// `columns - prefix.length`, with the prefix counted as the escaped string it is by then.
		let width = self.columns.saturating_sub(
			spacing + if dimmed { DIM_ESCAPES } else { 0 } + prefix.chars().count() + 1,
		);

		let wrapped: Vec<Line> = text
			.paragraphs()
			.iter()
			.flat_map(|paragraph| paragraph.wrap(width))
			.collect();
		let last = wrapped.len() - 1;

		let mut out = Vec::new();
		for (at, row) in wrapped.into_iter().enumerate() {
			for _ in 0..spacing {
				out.push(Line::blank());
			}
			let mut line = Line::blank();
			let opening = if at == 0 {
				&prefix
			} else if at == last {
				&prefix_end
			} else {
				&prefix
			};
			if !opening.is_empty() {
				line.push(Span::styled(opening.clone(), prefix_style));
			}
			if at == 0
				&& let Some((symbol, style)) = checkbox
			{
				line.push(Span::styled(symbol, style));
			}
			line.push(Span::raw(" "));
			for span in row.spans {
				line.push(span);
			}
			out.push(line);
		}
		out
	}
}

impl<T: Clone + PartialEq> Widget for &GroupMultiSelectWidget<'_, T> {
	fn render(self, area: Rect, buffer: &mut Buffer) {
		(&self.frame()).render(area, buffer);
	}
}

static THEME: Theme = Theme::clack();

#[cfg(test)]
mod tests {
	use super::*;
	use crate::line_editor::{Key, KeyName};
	use crate::multi_select::required;

	fn groups() -> Vec<(String, Vec<SelectOption<String>>)> {
		vec![
			(
				"group1".to_owned(),
				vec![
					SelectOption::new("group1value0".to_owned()),
					SelectOption::new("group1value1".to_owned()),
				],
			),
			(
				"group2".to_owned(),
				vec![SelectOption::new("group2value0".to_owned())],
			),
		]
	}

	fn state() -> GroupMultiSelectState<String> {
		GroupMultiSelectState::new(groups())
	}

	fn prompt(state: GroupMultiSelectState<String>) -> Prompt<GroupMultiSelectState<String>> {
		Prompt::new(state)
	}

	fn press(prompt: &mut Prompt<GroupMultiSelectState<String>>, name: KeyName) {
		prompt.key(None, &Key::named(name));
	}

	fn space(prompt: &mut Prompt<GroupMultiSelectState<String>>) {
		prompt.key(Some(" "), &Key::named(KeyName::Char(' ')));
	}

	fn held(prompt: &Prompt<GroupMultiSelectState<String>>) -> Vec<&str> {
		prompt
			.state()
			.selected()
			.iter()
			.map(String::as_str)
			.collect()
	}

	fn drawn(widget: &GroupMultiSelectWidget<'_, String>) -> Vec<String> {
		widget
			.frame()
			.lines
			.iter()
			.map(|line| line.spans.iter().map(|span| span.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn the_groups_are_flattened_header_first() {
		let state = state();
		let labels: Vec<&str> = state.rows().iter().map(Row::label).collect();
		assert_eq!(
			labels,
			[
				"group1",
				"group1value0",
				"group1value1",
				"group2",
				"group2value0"
			]
		);
	}

	#[test]
	fn the_cursor_wraps_at_both_ends() {
		let mut prompt = prompt(state());
		press(&mut prompt, KeyName::Up);
		assert_eq!(prompt.state().cursor(), 4);
		press(&mut prompt, KeyName::Down);
		assert_eq!(prompt.state().cursor(), 0);
	}

	#[test]
	fn unselectable_groups_are_stepped_over() {
		let mut prompt = prompt(state().with_selectable_groups(false));
		// The floor, rather than the header at zero.
		assert_eq!(prompt.state().cursor(), 1);
		press(&mut prompt, KeyName::Down);
		assert_eq!(prompt.state().cursor(), 2);
		// Row three is `group2`'s header, so the walk takes a second step.
		press(&mut prompt, KeyName::Down);
		assert_eq!(prompt.state().cursor(), 4);
	}

	#[test]
	fn space_on_a_header_ticks_the_whole_group() {
		let mut prompt = prompt(state());
		space(&mut prompt);
		assert_eq!(held(&prompt), ["group1value0", "group1value1"]);
		space(&mut prompt);
		assert!(held(&prompt).is_empty());
	}

	#[test]
	fn a_group_whose_members_are_all_ticked_is_ticked_itself() {
		let mut prompt = prompt(state());
		press(&mut prompt, KeyName::Down);
		space(&mut prompt);
		assert!(!prompt.state().is_group_selected("group1"));
		press(&mut prompt, KeyName::Down);
		space(&mut prompt);
		assert!(prompt.state().is_group_selected("group1"));
	}

	/// `items.every(…)` over no items.
	#[test]
	fn unticking_a_group_leaves_the_other_groups_alone() {
		let mut prompt = prompt(state().with_initial_values([
			"group2value0".to_owned(),
			"group1value0".to_owned(),
			"group1value1".to_owned(),
		]));
		// `group1` opens complete, so the first `space` on its header unticks it — and only it.
		space(&mut prompt);
		assert_eq!(held(&prompt), ["group2value0"]);
	}

	#[test]
	fn a_group_with_nothing_in_it_opens_ticked() {
		let state = GroupMultiSelectState::<String>::new([("empty".to_owned(), Vec::new())]);
		assert!(state.is_group_selected("empty"));
	}

	#[test]
	fn ticking_a_group_drops_a_duplicate_the_initial_values_carried_in() {
		let mut prompt = prompt(state().with_initial_values([
			"group2value0".to_owned(),
			"group2value0".to_owned(),
			"group1value0".to_owned(),
		]));
		// Ticking `group1` completes it, and the dedupe that follows is over the whole selection.
		space(&mut prompt);
		assert_eq!(
			held(&prompt),
			["group2value0", "group1value0", "group1value1"]
		);
	}

	#[test]
	fn cursor_at_names_an_option_and_the_floor_does_not_lower_it() {
		let state = state().with_cursor_at(&"group2value0".to_owned());
		assert_eq!(state.cursor(), 4);
		assert_eq!(state.with_selectable_groups(false).cursor(), 4);
	}

	#[test]
	fn a_cursor_at_nothing_holds_falls_to_the_floor() {
		assert_eq!(state().with_cursor_at(&"nowhere".to_owned()).cursor(), 0);
	}

	#[test]
	fn the_opening_frame_draws_a_branch_down_each_group() {
		let prompt = prompt(state());
		assert_eq!(
			drawn(&GroupMultiSelectWidget::new(&prompt, "foo")),
			[
				"│",
				"◆  foo",
				"│  ◻ group1",
				"│  │ ◻ group1value0",
				"│  └ ◻ group1value1",
				"│  ◻ group2",
				"│  └ ◻ group2value0",
				"│  ↑/↓ to navigate • Space: select • Enter: confirm",
				"└",
				"",
			]
		);
	}

	#[test]
	fn a_group_the_cursor_is_on_has_its_branch_undimmed() {
		let prompt = prompt(state());
		let frame = GroupMultiSelectWidget::new(&prompt, "foo").frame();
		let branch = |row: usize| frame.lines[row].spans[2].style;
		// Row three is the first option of the group the cursor's header names; row six belongs to
		// the other group.
		assert_eq!(branch(3), Style::new());
		assert_eq!(branch(6), Theme::clack().styles.option_unselected);
	}

	#[test]
	fn an_unselectable_group_header_has_no_checkbox() {
		let prompt = prompt(state().with_selectable_groups(false));
		let rows = drawn(&GroupMultiSelectWidget::new(&prompt, "foo"));
		assert_eq!(rows[2], "│   group1");
		assert_eq!(rows[3], "│    ◻ group1value0");
	}

	/// The blank rows carry the Guide, because they are rows of `limitOptions`' array by the time it
	/// is joined onto every one of them — not part of the prefix any more.
	#[test]
	fn group_spacing_draws_blank_rows_behind_the_guide() {
		let prompt = prompt(state());
		let rows = drawn(&GroupMultiSelectWidget::new(&prompt, "foo").with_group_spacing(2));
		assert_eq!(
			&rows[2..8],
			[
				"│  ",
				"│  ",
				"│  ◻ group1",
				"│  │ ◻ group1value0",
				"│  └ ◻ group1value1",
				"│  "
			]
		);
	}

	#[test]
	fn negative_spacing_draws_none() {
		let prompt = prompt(state());
		let rows = drawn(&GroupMultiSelectWidget::new(&prompt, "foo").with_group_spacing(-2));
		assert_eq!(rows[2], "│  ◻ group1");
	}

	/// The hint follows the label on the option under the cursor and on a ticked one, and is dropped
	/// from an option that is neither — `select`'s rule, with the ticked states folded in.
	#[test]
	fn a_hint_is_drawn_beside_the_active_option_and_nowhere_idle() {
		let prompt = prompt(GroupMultiSelectState::new([(
			"g".to_owned(),
			vec![
				SelectOption::new("a".to_owned()).with_hint("first"),
				SelectOption::new("b".to_owned()).with_hint("second"),
			],
		)]));
		let rows = drawn(&GroupMultiSelectWidget::new(&prompt, "foo"));
		// The cursor is on the header, so its group's options are `group-active` and keep no hint.
		assert_eq!(&rows[3..5], ["│  │ ◻ a", "│  └ ◻ b"]);

		let mut prompt = prompt;
		press(&mut prompt, KeyName::Down);
		let rows = drawn(&GroupMultiSelectWidget::new(&prompt, "foo"));
		assert_eq!(&rows[3..5], ["│  │ ◻ a (first)", "│  └ ◻ b"]);
	}

	/// `label.trim()`, so an option labelled with spaces reads as nothing chosen — and a cancelled
	/// Frame then draws neither the label nor the bar that would close under it.
	#[test]
	fn a_cancelled_value_of_nothing_but_spaces_is_no_value() {
		let mut prompt = prompt(
			GroupMultiSelectState::new([(
				"g".to_owned(),
				vec![SelectOption::labelled("a".to_owned(), "   ")],
			)])
			.with_initial_values(["a".to_owned()]),
		);
		press(&mut prompt, KeyName::Escape);
		assert_eq!(
			drawn(&GroupMultiSelectWidget::new(&prompt, "foo")),
			["│", "■  foo", "│  "]
		);
	}

	/// With the groups unselectable there is no closing corner either — `prefixEnd` is left at the
	/// empty string it was declared with, so a wrapped option's later rows sit a column to the left
	/// of its first. The rows are `narrow › an unselectable group wraps its option back to the
	/// margin` in the authored Fixture.
	#[test]
	fn an_unselectable_group_wraps_its_option_back_to_the_margin() {
		let prompt = prompt(
			GroupMultiSelectState::new([(
				"Language".to_owned(),
				vec![SelectOption::labelled(
					"ts".to_owned(),
					"TypeScript, which is a static type checker",
				)],
			)])
			.with_selectable_groups(false),
		);
		let rows = drawn(&GroupMultiSelectWidget::new(&prompt, "Pick").with_columns(40));
		assert_eq!(
			&rows[3..5],
			["│    ◻ TypeScript, which is a ", "│   static type checker"]
		);
	}

	#[test]
	fn a_submitted_frame_is_one_row_of_labels() {
		let mut prompt = prompt(
			state().with_initial_values(["group1value1".to_owned(), "group2value0".to_owned()]),
		);
		press(&mut prompt, KeyName::Return);
		assert_eq!(
			drawn(&GroupMultiSelectWidget::new(&prompt, "foo")),
			["│", "◇  foo", "│  group1value1, group2value0"]
		);
	}

	/// No `none`: `groupMultiselect`'s submit branch has no fallback, and the two spaces belong to
	/// the value rather than to the bar — so a Prompt that chose nothing settles as a bare bar,
	/// where a cancelled one keeps its spaces.
	#[test]
	fn a_submitted_frame_with_nothing_chosen_says_nothing() {
		let mut prompt = prompt(state());
		press(&mut prompt, KeyName::Return);
		assert_eq!(
			drawn(&GroupMultiSelectWidget::new(&prompt, "foo")),
			["│", "◇  foo", "│"]
		);
	}

	#[test]
	fn a_cancelled_frame_closes_with_a_bar_of_its_own() {
		let mut prompt = prompt(state().with_initial_values(["group1value0".to_owned()]));
		press(&mut prompt, KeyName::Escape);
		assert_eq!(
			drawn(&GroupMultiSelectWidget::new(&prompt, "foo")),
			["│", "■  foo", "│  group1value0", "│"]
		);
	}

	#[test]
	fn a_cancelled_frame_with_nothing_chosen_draws_no_closing_bar() {
		let mut prompt = prompt(state());
		press(&mut prompt, KeyName::Escape);
		assert_eq!(
			drawn(&GroupMultiSelectWidget::new(&prompt, "foo")),
			["│", "■  foo", "│  "]
		);
	}

	#[test]
	fn the_error_frame_is_the_multiselects() {
		let mut prompt = prompt(state()).with_validator(required::<String>);
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Error);
		let rows = drawn(&GroupMultiSelectWidget::new(&prompt, "foo"));
		assert_eq!(rows[7], "└  Please select at least one option.");
		assert_eq!(rows[8], "   Press  space  to select,  enter  to submit");
	}

	/// The same label breaks in two places depending on whether its branch was dimmed: an option
	/// under the cursor's own group is wrapped against `columns - 3` and every other against
	/// `columns - 12`, because the prefix is measured after `styleText` has been at it.
	///
	/// The rows are `narrow › a group option wraps differently under a dimmed branch` in the
	/// authored Fixture, which is where the numbers come from.
	#[test]
	fn a_dimmed_branch_wraps_its_option_nine_columns_early() {
		let prompt = prompt(GroupMultiSelectState::new([
			(
				"Testing".to_owned(),
				vec![SelectOption::labelled(
					"jest".to_owned(),
					"Jest, a JavaScript testing framework",
				)],
			),
			(
				"Language".to_owned(),
				vec![SelectOption::labelled(
					"ts".to_owned(),
					"TypeScript, which is a static type checker",
				)],
			),
		]));
		let rows = drawn(&GroupMultiSelectWidget::new(&prompt, "Pick").with_columns(40));
		assert_eq!(
			&rows[3..8],
			[
				// Thirty-seven columns for the label, so it is `limitOptions` that breaks this one
				// and the second row takes no indent at all.
				"│  └ ◻ Jest, a JavaScript ",
				"│  testing framework",
				"│  ◻ Language",
				// Twenty-eight, so the break is the option's own and the row under it is indented
				// by the `prefixEnd` that goes with it.
				"│  └ ◻ TypeScript, which is a ",
				"│     static type checker",
			]
		);
	}
}
