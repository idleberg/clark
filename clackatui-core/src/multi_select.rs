//! Ported from `@clack/core`'s `prompts/multi-select.ts` and `@clack/prompts`' `multi-select.ts`.
//!
//! [`select`](crate::select) with a set instead of a cursor position for an answer. The list, the
//! cursor walk, the option type and the arithmetic are all shared with it; what is new is a
//! selection that `space` toggles, two whole-list shortcuts, and — for the first time in a Prompt
//! that draws a list — a validation failure with a Frame of its own.
//!
//! # The error Frame is the widget's, not the validator's
//!
//! Upstream's `multiselect` supplies its own `validate`, and the string it returns has escapes in
//! it: a second line reading `Press ␣space␣ to select, ␣enter␣ to submit`, with the two key names
//! inverted on a white ground. A Frame here holds styling as a [`Style`] per span and no escapes at
//! all (ADR-0011), so that line cannot travel inside a [`Prompt`]'s error string. It is drawn by
//! [`MultiSelectWidget`] instead, from the Theme, and the Prompt carries only the sentence —
//! [`REQUIRED_ERROR`]. The same pixels, reached from the other side.
//!
//! It is also the only error a `multiselect` has. Upstream's `validate` option is overwritten by the
//! one `multiselect()` installs, so there is no user predicate to compose with and no second message
//! to draw.
//!
//! # What a break leaves open, once there are several values
//!
//! A settled `multiselect` draws every chosen label at once, joined with a dim `, `. Each label
//! carries its own styling and the separator carries its own, so the leak
//! [`wrap::leaked`](crate::wrap::leaked) describes is no longer a property of the whole value: a row
//! that breaks inside a label leaves the strikethrough open across the next row's Guide bar, and a
//! row that breaks at a separator does not. What is still open at a break is what the text on *both*
//! sides of it carries, which is what [`carried`] computes.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::Style;
use ratatui_core::widgets::Widget;

use crate::cursor::find_cursor;
use crate::frame::{Frame, Line, Span};
use crate::limit_options::LimitOptions;
use crate::line_editor::{Key, KeyName};
use crate::prompt::{Prompt, PromptState, Status};
use crate::select::{GUIDE_PREFIX_LENGTH, SelectOption};
use crate::settings::Action;
use crate::theme::Theme;
use crate::wrap::leaked;

/// `MULTISELECT_INSTRUCTIONS`. `select`'s two with the space key wedged into the middle.
pub const INSTRUCTIONS: [(&str, &str); 3] = [
	("↑/↓", " to navigate"),
	("Space:", " select"),
	("Enter:", " confirm"),
];

/// The separator upstream joins the instructions with.
pub const INSTRUCTION_SEPARATOR: &str = " • ";

/// The `, ` between the labels of a settled `multiselect`.
pub const VALUE_SEPARATOR: &str = ", ";

/// What a `multiselect` that chose nothing submits as, when it is allowed to.
pub const NOTHING: &str = "none";

/// The sentence upstream's `validate` returns. The advice under it is [`MultiSelectWidget`]'s — see
/// the module docs.
pub const REQUIRED_ERROR: &str = "Please select at least one option.";

/// The advice, as the pieces it is drawn from: the prose either side of each key, and the key.
///
/// Written out rather than assembled from the instruction footer because it is not the same text and
/// does not read the same way — the footer names keys, this one asks for them.
pub const ERROR_HINT: [(&str, &str); 3] = [
	("Press ", " space "),
	(" to select, ", " enter "),
	(" to submit", ""),
];

/// The state of a `multiselect`: a list, a cursor, and the values that have been ticked.
#[derive(Clone, Debug)]
pub struct MultiSelectState<T> {
	options: Vec<SelectOption<T>>,
	cursor: usize,
	/// In the order they were ticked, which is upstream's `[...this.value, this._value]` and is not
	/// the order they are drawn in — every Frame filters the option list instead.
	selected: Vec<T>,
}

impl<T> MultiSelectState<T> {
	/// The list, with nothing ticked and the cursor on the first option that can be chosen.
	pub fn new(options: Vec<SelectOption<T>>) -> Self {
		let mut state = Self {
			options,
			cursor: 0,
			selected: Vec::new(),
		};
		state.settle_cursor(0);
		state
	}

	/// `cursorAt`: open with the cursor on the option holding this value.
	///
	/// A value the list does not hold leaves it on the first option, for the reason `select`'s does:
	/// upstream turns the `-1` from `findIndex` into a `0` rather than using it.
	pub fn with_cursor_at(mut self, value: &T) -> Self
	where
		T: PartialEq,
	{
		let at = self
			.options
			.iter()
			.position(|option| option.value() == value)
			.unwrap_or(0);
		self.settle_cursor(at);
		self
	}

	/// `initialValues`: the values ticked before a key is pressed.
	///
	/// Copied verbatim, as upstream's `[...(opts.initialValues ?? [])]` is. A value the list does not
	/// hold is kept and drawn nowhere, and a disabled option's value is kept and drawn ticked — this
	/// is the one way a `multiselect` can start out holding something the user could not have picked.
	pub fn with_initial_values(mut self, values: impl IntoIterator<Item = T>) -> Self {
		self.selected = values.into_iter().collect();
		self
	}

	pub fn cursor(&self) -> usize {
		self.cursor
	}

	pub fn options(&self) -> &[SelectOption<T>] {
		&self.options
	}

	/// The values ticked so far, in the order they were ticked.
	pub fn selected(&self) -> &[T] {
		&self.selected
	}

	/// Whether an option is ticked. `Array.prototype.includes`, which for the values a Prompt can
	/// hold is equality.
	pub fn is_selected(&self, option: &SelectOption<T>) -> bool
	where
		T: PartialEq,
	{
		self.selected.contains(option.value())
	}

	/// The ticked options, in the order the list draws them — upstream's `options.filter(…)`.
	pub fn chosen(&self) -> impl Iterator<Item = &SelectOption<T>>
	where
		T: PartialEq,
	{
		self.options
			.iter()
			.filter(|option| self.selected.contains(option.value()))
	}

	/// See [`SelectState::settle_cursor`](crate::select::SelectState). The same rule, because it is
	/// the same two lines of upstream.
	fn settle_cursor(&mut self, at: usize) {
		self.cursor = if self.options.get(at).is_some_and(|o| o.disabled()) {
			find_cursor(at, 1, &self.options, |o| o.disabled())
		} else {
			at
		};
	}

	/// `toggleValue`: tick the option under the cursor, or untick it.
	///
	/// Upstream does not ask whether that option is disabled, and neither does this. The cursor
	/// cannot ordinarily reach one — but a list in which *every* option is disabled leaves it where
	/// [`find_cursor`] gave up, and `space` there ticks something the list draws as unchoosable.
	fn toggle(&mut self)
	where
		T: Clone + PartialEq,
	{
		let Some(value) = self.options.get(self.cursor).map(SelectOption::value) else {
			return;
		};
		match self.selected.iter().position(|held| held == value) {
			Some(at) => {
				self.selected.remove(at);
			}
			None => self.selected.push(value.clone()),
		}
	}

	/// `toggleAll`, bound to `a`: everything, or nothing.
	///
	/// "Everything" is counted rather than compared — upstream asks only whether as many values are
	/// held as there are options that can be chosen. Start a Prompt with an `initialValues` holding a
	/// disabled option's value and the count can be reached without every enabled option being in it,
	/// and the first `a` will clear the list rather than fill it.
	fn toggle_all(&mut self)
	where
		T: Clone,
	{
		let enabled = || self.options.iter().filter(|o| !o.disabled());
		self.selected = if self.selected.len() == enabled().count() {
			Vec::new()
		} else {
			enabled().map(|o| o.value().clone()).collect()
		};
	}

	/// `toggleInvert`, bound to `i`: everything that can be chosen and is not held.
	///
	/// A value held for an option that cannot be chosen is dropped rather than kept, since the new
	/// selection is built from the enabled options alone.
	fn toggle_invert(&mut self)
	where
		T: Clone + PartialEq,
	{
		self.selected = self
			.options
			.iter()
			.filter(|option| !option.disabled() && !self.selected.contains(option.value()))
			.map(|option| option.value().clone())
			.collect();
	}
}

impl<T: Clone + PartialEq> PromptState for MultiSelectState<T> {
	type Value = Vec<T>;

	/// `super(opts, false)`, as `select`'s is — so `j` and `k` walk the list here too.
	const TRACKS_INPUT: bool = false;

	fn cursor(&mut self, action: Action) {
		match action {
			Action::Left | Action::Up => {
				self.cursor = find_cursor(self.cursor, -1, &self.options, |o| o.disabled());
			}
			Action::Down | Action::Right => {
				self.cursor = find_cursor(self.cursor, 1, &self.options, |o| o.disabled());
			}
			Action::Space => self.toggle(),
			_ => {}
		}
	}

	/// `on('key')`: the two shortcuts, read off the key's *name* rather than the character.
	///
	/// Which means they are not aliases and cannot be rebound, and that a capital `A` does nothing —
	/// readline names a shifted letter by its lowercase, but only after setting `shift`, and upstream
	/// compares the name without looking at it. That is reproduced: the name is what is matched.
	fn key(&mut self, _s: Option<&str>, key: &Key) {
		match key.name {
			Some(KeyName::Char('a')) => self.toggle_all(),
			Some(KeyName::Char('i')) => self.toggle_invert(),
			_ => {}
		}
	}

	/// Always a list, never nothing: upstream's constructor assigns one before any key arrives, so
	/// the validator sees an empty array rather than `undefined`.
	fn value(&self) -> Option<&Vec<T>> {
		Some(&self.selected)
	}
}

/// Upstream's `validate`, less the advice the widget draws — see the module docs.
///
/// Handed to [`Prompt::with_validator`](crate::prompt::Prompt::with_validator) by whatever builds
/// the Prompt, and only when `required` is on.
pub fn required<T>(value: Option<&Vec<T>>) -> Option<String> {
	value
		.is_none_or(Vec::is_empty)
		.then(|| REQUIRED_ERROR.to_owned())
}

/// A `multiselect` Prompt drawn as a Frame — the `render` callback of `@clack/prompts`'
/// `multiselect()`.
pub struct MultiSelectWidget<'a, T: Clone + PartialEq> {
	prompt: &'a Prompt<MultiSelectState<T>>,
	message: &'a str,
	theme: &'a Theme,
	/// The Prompt's own output stream. See [`select`](crate::select)'s module docs: it is not the
	/// width the finished Frame is wrapped to.
	columns: usize,
	rows: usize,
	max_items: Option<usize>,
	show_instructions: bool,
	with_guide: Option<bool>,
}

impl<'a, T: Clone + PartialEq> MultiSelectWidget<'a, T> {
	pub fn new(prompt: &'a Prompt<MultiSelectState<T>>, message: &'a str) -> Self {
		Self {
			prompt,
			message,
			theme: &THEME,
			columns: 80,
			rows: 20,
			max_items: None,
			show_instructions: true,
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

	pub fn with_guide(mut self, with_guide: bool) -> Self {
		self.with_guide = Some(with_guide);
		self
	}

	fn guided(&self) -> bool {
		self.with_guide.unwrap_or(self.prompt.settings().with_guide)
	}
}

impl<T: Clone + PartialEq> MultiSelectWidget<'_, T> {
	/// The Frame, branch for branch as upstream's `render` writes it.
	pub fn frame(&self) -> Frame {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let guide = self.guided();
		let status = self.prompt.status();
		let state = self.prompt.state();

		let mut frame = Frame::new();
		let title = self.title(status);
		let title_rows = title.len() + 1;
		for line in title {
			frame.push(line);
		}

		match status {
			Status::Submit | Status::Cancel => {
				// `label.trim() === ''`, asked of the joined labels rather than of the selection, so
				// that one option labelled with spaces settles the same way none at all does.
				let blank = state
					.chosen()
					.map(SelectOption::label)
					.collect::<Vec<_>>()
					.join(VALUE_SEPARATOR)
					.trim()
					.is_empty();

				if !(status == Status::Cancel && blank) {
					for line in self.value_rows(status, guide) {
						frame.push(line);
					}
				}
				// A cancelled `multiselect` with nothing to show returns before it consults the
				// Guide, so the bar it closes with is drawn whether or not there is a Guide for it to
				// belong to — the third of `select`'s family of unguided bars.
				if status == Status::Cancel && (guide || blank) {
					frame.push(Line::from(Span::styled(symbols.bar, styles.guide)));
				}
			}

			// The list again, under a yellow Guide, with the validation message where the
			// instructions were.
			Status::Error => {
				let footer = self.error_footer(guide);
				let row_padding = title_rows + footer.len() + 1;
				let bar = Span::styled(symbols.bar, styles.guide_error);
				for line in self.list(guide.then_some(bar), row_padding) {
					frame.push(line);
				}
				for line in footer {
					frame.push(line);
				}
				// The `\n` upstream writes after the footer, which is a row like any other.
				frame.push(Line::blank());
			}

			_ => {
				let footer = self.footer(guide);
				let row_padding = title_rows + footer.len() + 1;
				let bar = Span::styled(symbols.bar, styles.guide_active);
				for line in self.list(guide.then_some(bar), row_padding) {
					frame.push(line);
				}
				// `${…}\n${footerText}\n`: an empty footer is still a row, for the reason
				// `select` gives.
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

	/// The option list, cut to what is left of the terminal, each row behind `bar` if there is one.
	fn list(&self, bar: Option<Span>, row_padding: usize) -> Vec<Line> {
		let state = self.prompt.state();
		let column_padding = if bar.is_some() {
			GUIDE_PREFIX_LENGTH
		} else {
			0
		};

		let mut limit = LimitOptions::new(state.options(), state.cursor())
			.with_columns(self.columns)
			.with_rows(self.rows)
			.with_column_padding(column_padding)
			.with_row_padding(row_padding)
			.with_theme(self.theme);
		if let Some(max_items) = self.max_items {
			limit = limit.with_max_items(max_items);
		}

		limit
			.lines(|option, active| self.option(option, active))
			.into_iter()
			.map(|row| {
				let mut line = Line::blank();
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

	/// `wrapTextWithPrefix(output, message, …)`, plus the bar above it. `select`'s, unchanged —
	/// including the continuation bar that a `withGuide: false` does not switch off.
	fn title(&self, status: Status) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;

		let mut lines = Vec::new();
		if self.guided() {
			lines.push(Line::from(Span::styled(symbols.bar, styles.guide)));
		}

		let wrapped = crate::wrap::wrap(
			self.message,
			self.columns.saturating_sub(GUIDE_PREFIX_LENGTH),
		);
		for (index, text) in wrapped.split('\n').enumerate() {
			let mut line = Line::blank();
			line.push(if index == 0 {
				self.theme.step(status)
			} else {
				self.theme.bar(status)
			});
			line.push(Span::raw("  "));
			line.push(Span::styled(text, styles.message));
			lines.push(line);
		}
		lines
	}

	/// `formatInstructionFooter(MULTISELECT_INSTRUCTIONS, hasGuide)`, and its two other shapes.
	fn footer(&self, guide: bool) -> Vec<Line> {
		footer(self.theme, guide, self.show_instructions)
	}

	/// The validation message and the advice under it — see [`error_footer`].
	fn error_footer(&self, guide: bool) -> Vec<Line> {
		error_footer(self.theme, self.prompt.error(), guide)
	}

	/// `opt(option, state)`: one option, as the rows it occupies before the list wraps them.
	///
	/// Five states rather than `select`'s three, and the two questions they answer are independent:
	/// the box says whether the option is ticked, the label says whether the cursor is on it. Hence
	/// an active option's label is drawn plainly and every other one is dimmed — including a ticked
	/// one, whose tick is carried entirely by the green box.
	///
	/// The hint follows the label everywhere except on an option that is neither ticked nor under the
	/// cursor, which is `select`'s rule with the ticked states folded in.
	fn option(&self, option: &SelectOption<T>, active: bool) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let selected = self.prompt.state().is_selected(option);

		let (box_symbol, box_style, label_style, hint) = if option.disabled() {
			(
				symbols.checkbox_inactive,
				styles.option_disabled,
				styles.option_disabled_label,
				option.hint(),
			)
		} else if active && selected {
			(
				symbols.checkbox_selected,
				styles.checkbox_selected,
				styles.message,
				option.hint(),
			)
		} else if selected {
			(
				symbols.checkbox_selected,
				styles.checkbox_selected,
				styles.option_selected,
				option.hint(),
			)
		} else if active {
			(
				symbols.checkbox_active,
				styles.checkbox_active,
				styles.message,
				option.hint(),
			)
		} else {
			(
				symbols.checkbox_inactive,
				styles.checkbox_inactive,
				styles.option_unselected,
				None,
			)
		};

		let rows: Vec<&str> = option.label().split('\n').collect();
		let last = rows.len() - 1;
		rows.into_iter()
			.enumerate()
			.map(|(index, text)| {
				let mut line = Line::blank();
				if index == 0 {
					line.push(Span::styled(box_symbol, box_style));
					line.push(Span::raw(" "));
				}
				line.push(Span::styled(text, label_style));
				if index == last {
					if let Some(hint) = hint {
						line.push(Span::raw(" "));
						line.push(Span::styled(format!("({hint})"), styles.hint));
					}
				}
				line
			})
			.collect()
	}

	/// The settled value: every chosen label, joined, wrapped, and drawn under the Guide.
	///
	/// Upstream builds one styled string and hands it to `wrapTextWithPrefix`, so the labels and the
	/// separators between them break as one piece of text rather than one at a time. Here that is a
	/// single [`Line`] of spans wrapped as a unit, which is the same thing said structurally.
	fn value_rows(&self, status: Status, guide: bool) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let state = self.prompt.state();
		let cancelled = status == Status::Cancel;
		let style = if cancelled {
			styles.cancelled
		} else {
			styles.submitted
		};

		let mut spans = Vec::new();
		for option in state.chosen() {
			if !spans.is_empty() {
				spans.push(Span::styled(VALUE_SEPARATOR, styles.separator));
			}
			spans.push(Span::styled(option.label(), style));
		}

		// A submitted `multiselect` that chose nothing says so, because `||` catches the empty string
		// on its way past. The cancelled case never reaches here — see `frame`.
		if spans.is_empty() {
			spans.push(Span::styled(NOTHING, styles.submitted));
		}

		let width = if guide {
			self.columns.saturating_sub(GUIDE_PREFIX_LENGTH)
		} else {
			self.columns
		};

		let mut out = Vec::new();
		// A line break inside a label is upstream's `computeLabel` splitting it and styling each row
		// on its own, so nothing leaks across one. Each paragraph is therefore wrapped by itself.
		let mut whole = Line::blank();
		for span in spans {
			whole.push(span);
		}
		for paragraph in whole.paragraphs() {
			let rows = paragraph.wrap(width);
			for (index, row) in rows.iter().enumerate() {
				let mut line = Line::blank();
				if guide {
					let open = if index == 0 {
						Style::new()
					} else {
						carried(&rows[index - 1], row)
					};
					line.push(Span::styled(symbols.bar, styles.guide.patch(open)));
					line.push(Span::styled("  ", open));
				}
				for span in &row.spans {
					line.push(span.clone());
				}
				out.push(line);
			}
		}
		out
	}
}

/// `formatInstructionFooter(MULTISELECT_INSTRUCTIONS, hasGuide)`, and its two other shapes.
///
/// Shared with `groupMultiselect`, which imports the same constant from `multi-select.ts` and draws
/// the same three rows under its own list.
pub fn footer(theme: &Theme, guide: bool, show_instructions: bool) -> Vec<Line> {
	let styles = &theme.styles;
	let symbols = &theme.symbols;

	if !show_instructions {
		return if guide {
			vec![Line::from(Span::styled(
				symbols.bar_end,
				styles.guide_active,
			))]
		} else {
			Vec::new()
		};
	}

	let mut line = Line::blank();
	if guide {
		line.push(Span::styled(symbols.bar, styles.guide_active));
		line.push(Span::raw("  "));
	}
	for (index, (key, verb)) in INSTRUCTIONS.iter().enumerate() {
		if index > 0 {
			line.push(Span::raw(INSTRUCTION_SEPARATOR));
		}
		line.push(Span::styled(*key, styles.instruction_key));
		line.push(Span::raw(*verb));
	}

	let mut lines = vec![line];
	if guide {
		lines.push(Line::from(Span::styled(
			symbols.bar_end,
			styles.guide_active,
		)));
	}
	lines
}

/// The validation message and the advice under it.
///
/// Two rows whatever else is switched off. The first takes the Guide's closing bar when there is a
/// Guide; the second is indented by three literal spaces and takes no bar at all, guided or not —
/// upstream writes `'   ' + line` for every line of the message after the first and never asks
/// whether the Guide is on. Shared with `groupMultiselect`, whose error branch is the same one.
pub fn error_footer(theme: &Theme, error: &str, guide: bool) -> Vec<Line> {
	let styles = &theme.styles;
	let symbols = &theme.symbols;

	let mut first = Line::blank();
	if guide {
		first.push(Span::styled(symbols.bar_end, styles.guide_error));
		first.push(Span::raw("  "));
	}
	first.push(Span::styled(error, styles.error));

	let mut second = Line::from(Span::raw("   "));
	for (prose, key) in ERROR_HINT {
		second.push(Span::styled(prose, styles.error_hint));
		if !key.is_empty() {
			second.push(Span::styled(key, styles.error_key));
		}
	}

	vec![first, second]
}

/// What is still open where two rows of a wrapped value meet.
///
/// `wrapAnsi` reopens what it recognises at the start of each row and leaves the rest open across
/// the break — but a style it does not recognise is still *closed* wherever the text that opened it
/// ends, and a value with several labels in it has such an end between every pair. So what survives
/// a break is what both sides of it carry: break inside a label and the strikethrough is open across
/// the Guide bar that follows; break at a separator, on either side of it, and it is not.
fn carried(previous: &Line, next: &Line) -> Style {
	let visible = |line: &Line, from_end: bool| -> Style {
		let mut spans = line.spans.iter().filter(|span| !span.text.is_empty());
		if from_end {
			spans.next_back()
		} else {
			spans.next()
		}
		.map(|span| span.style)
		.unwrap_or_default()
	};

	let both = visible(previous, true)
		.add_modifier
		.intersection(visible(next, false).add_modifier);
	leaked(Style::new().add_modifier(both))
}

/// The default Theme, as somewhere to borrow from.
static THEME: Theme = Theme::clack();

impl<T: Clone + PartialEq> Widget for &MultiSelectWidget<'_, T> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		(&self.frame()).render(area, buf);
	}
}

#[cfg(test)]
mod tests {
	use ratatui_core::style::Modifier;

	use super::*;
	use crate::prompt::Outcome;

	fn options(labels: &[&str]) -> Vec<SelectOption<String>> {
		labels
			.iter()
			.map(|label| SelectOption::new(label.to_string()))
			.collect()
	}

	fn multiselect(labels: &[&str]) -> Prompt<MultiSelectState<String>> {
		Prompt::new(MultiSelectState::new(options(labels)))
	}

	fn press(prompt: &mut Prompt<MultiSelectState<String>>, name: KeyName) {
		prompt.key(None, &Key::named(name));
	}

	fn typed(prompt: &mut Prompt<MultiSelectState<String>>, c: char) {
		prompt.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
	}

	fn space(prompt: &mut Prompt<MultiSelectState<String>>) {
		prompt.key(Some(" "), &Key::named(KeyName::Char(' ')));
	}

	fn held(prompt: &Prompt<MultiSelectState<String>>) -> Vec<&str> {
		prompt
			.state()
			.selected()
			.iter()
			.map(String::as_str)
			.collect()
	}

	#[test]
	fn space_ticks_the_option_under_the_cursor_and_untick_it_again() {
		let mut prompt = multiselect(&["a", "b"]);
		space(&mut prompt);
		assert_eq!(held(&prompt), ["a"]);
		space(&mut prompt);
		assert!(held(&prompt).is_empty());
	}

	/// The answer is in the order the boxes were ticked, not the order the list draws them — which
	/// is what upstream's `[...this.value, this._value]` builds, and is only visible from here.
	#[test]
	fn the_answer_keeps_the_order_the_boxes_were_ticked_in() {
		let mut prompt = multiselect(&["a", "b", "c"]);
		press(&mut prompt, KeyName::Down);
		press(&mut prompt, KeyName::Down);
		space(&mut prompt);
		press(&mut prompt, KeyName::Up);
		press(&mut prompt, KeyName::Up);
		space(&mut prompt);
		assert_eq!(held(&prompt), ["c", "a"]);
	}

	#[test]
	fn a_ticks_everything_and_then_nothing() {
		let mut prompt = multiselect(&["a", "b", "c"]);
		typed(&mut prompt, 'a');
		assert_eq!(held(&prompt), ["a", "b", "c"]);
		typed(&mut prompt, 'a');
		assert!(held(&prompt).is_empty());
	}

	#[test]
	fn a_leaves_the_options_that_cannot_be_chosen_out() {
		let mut list = options(&["a", "b"]);
		list[1] = list[1].clone().with_disabled(true);
		let mut prompt = Prompt::new(MultiSelectState::new(list));
		typed(&mut prompt, 'a');
		assert_eq!(held(&prompt), ["a"]);
	}

	/// `toggleAll` counts rather than compares. A selection that is the right *size* without being
	/// the right selection reads as "everything", and the first `a` empties it.
	#[test]
	fn a_clears_a_selection_that_is_merely_the_right_size() {
		let mut list = options(&["a", "b"]);
		list[1] = list[1].clone().with_disabled(true);
		let state = MultiSelectState::new(list).with_initial_values(["b".to_string()]);
		let mut prompt = Prompt::new(state);
		typed(&mut prompt, 'a');
		assert!(held(&prompt).is_empty());
	}

	#[test]
	fn i_swaps_the_ticked_for_the_unticked() {
		let mut prompt = multiselect(&["a", "b", "c"]);
		space(&mut prompt);
		typed(&mut prompt, 'i');
		assert_eq!(held(&prompt), ["b", "c"]);
	}

	#[test]
	fn the_cursor_steps_over_a_disabled_option() {
		let mut list = options(&["a", "b", "c"]);
		list[1] = list[1].clone().with_disabled(true);
		let mut prompt = Prompt::new(MultiSelectState::new(list));
		press(&mut prompt, KeyName::Down);
		space(&mut prompt);
		assert_eq!(held(&prompt), ["c"]);
	}

	/// The one way a disabled option can be ticked by hand: with nothing else to walk to, the cursor
	/// stays on it and `space` does not ask.
	#[test]
	fn a_list_of_nothing_but_disabled_options_can_still_be_ticked() {
		let list = options(&["a", "b"])
			.into_iter()
			.map(|option| option.with_disabled(true))
			.collect();
		let mut prompt = Prompt::new(MultiSelectState::new(list));
		space(&mut prompt);
		assert_eq!(held(&prompt), ["a"]);
	}

	#[test]
	fn the_vim_aliases_walk_the_list() {
		let mut prompt = multiselect(&["a", "b"]);
		typed(&mut prompt, 'j');
		assert_eq!(prompt.state().cursor(), 1);
		typed(&mut prompt, 'k');
		assert_eq!(prompt.state().cursor(), 0);
	}

	#[test]
	fn cursor_at_opens_the_list_somewhere_else() {
		let state =
			MultiSelectState::new(options(&["a", "b", "c"])).with_cursor_at(&"c".to_string());
		assert_eq!(state.cursor(), 2);
	}

	#[test]
	fn an_untouched_multiselect_submits_an_empty_list_rather_than_nothing() {
		let mut prompt = multiselect(&["a"]);
		press(&mut prompt, KeyName::Return);
		match prompt.outcome() {
			Some(Outcome::Submitted(Some(values))) => assert!(values.is_empty()),
			other => panic!("{:?}", other.is_some()),
		}
	}

	#[test]
	fn required_rejects_an_empty_selection_and_nothing_else() {
		assert_eq!(
			required(Some(&Vec::<String>::new())).as_deref(),
			Some(REQUIRED_ERROR)
		);
		assert_eq!(required::<String>(None).as_deref(), Some(REQUIRED_ERROR));
		assert_eq!(required(Some(&vec!["a".to_string()])), None);
	}

	// --- The widget -----------------------------------------------------------------------------

	fn drawn(widget: &MultiSelectWidget<'_, String>) -> Vec<String> {
		widget
			.frame()
			.lines
			.iter()
			.map(|line| line.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn an_opening_frame_is_the_title_the_list_and_three_instructions() {
		let prompt = multiselect(&["a", "b"]);
		let widget = MultiSelectWidget::new(&prompt, "foo");
		assert_eq!(
			drawn(&widget),
			[
				"│",
				"◆  foo",
				"│  ◻ a",
				"│  ◻ b",
				"│  ↑/↓ to navigate • Space: select • Enter: confirm",
				"└",
				"",
			]
		);
	}

	#[test]
	fn a_submitted_frame_joins_the_labels_with_a_comma() {
		let mut prompt = multiselect(&["a", "b", "c"]);
		space(&mut prompt);
		press(&mut prompt, KeyName::Down);
		press(&mut prompt, KeyName::Down);
		space(&mut prompt);
		press(&mut prompt, KeyName::Return);
		assert_eq!(
			drawn(&MultiSelectWidget::new(&prompt, "foo")),
			["│", "◇  foo", "│  a, c"]
		);
	}

	#[test]
	fn a_submitted_frame_with_nothing_ticked_says_so() {
		let mut prompt = multiselect(&["a"]);
		press(&mut prompt, KeyName::Return);
		assert_eq!(
			drawn(&MultiSelectWidget::new(&prompt, "foo")),
			["│", "◇  foo", "│  none"]
		);
	}

	/// Upstream returns before it consults the Guide here, so the closing bar is drawn even where
	/// every other bar has been switched off.
	#[test]
	fn a_cancelled_frame_with_nothing_ticked_is_a_bar_the_guide_did_not_ask_for() {
		let mut prompt = multiselect(&["a"]);
		press(&mut prompt, KeyName::Escape);
		let widget = MultiSelectWidget::new(&prompt, "foo").with_guide(false);
		assert_eq!(drawn(&widget), ["■  foo", "│"]);
	}

	#[test]
	fn a_cancelled_frame_with_values_closes_with_a_second_guide() {
		let mut prompt = multiselect(&["a", "b"]);
		space(&mut prompt);
		press(&mut prompt, KeyName::Escape);
		assert_eq!(
			drawn(&MultiSelectWidget::new(&prompt, "foo")),
			["│", "■  foo", "│  a", "│"]
		);
	}

	/// The box says whether an option is ticked and the label says whether the cursor is on it, so a
	/// ticked option under the cursor differs from a ticked one elsewhere only in the label's style.
	#[test]
	fn a_ticked_option_is_green_wherever_the_cursor_is() {
		let mut prompt = multiselect(&["a", "b"]);
		space(&mut prompt);
		press(&mut prompt, KeyName::Down);
		let widget = MultiSelectWidget::new(&prompt, "foo");
		let list = &widget.frame().lines[2..4];
		assert_eq!(list[0].spans[2].text, "◼");
		assert_eq!(
			list[0].spans[2].style,
			Theme::clack().styles.checkbox_selected
		);
		assert_eq!(
			list[0].spans[4].style,
			Theme::clack().styles.option_selected
		);
		// And the one under the cursor is drawn plainly, ticked or not.
		assert_eq!(list[1].spans[2].text, "◻");
		assert_eq!(list[1].spans[4].style, Theme::clack().styles.message);
	}

	#[test]
	fn a_hint_follows_every_option_but_the_one_that_is_neither_ticked_nor_under_the_cursor() {
		let list = vec![
			SelectOption::new("a".to_string()).with_hint("first"),
			SelectOption::new("b".to_string()).with_hint("second"),
			SelectOption::new("c".to_string()).with_hint("third"),
		];
		let state = MultiSelectState::new(list).with_initial_values(["b".to_string()]);
		let prompt = Prompt::new(state);
		let drawn = drawn(&MultiSelectWidget::new(&prompt, "foo"));
		assert_eq!(drawn[2], "│  ◻ a (first)");
		assert_eq!(drawn[3], "│  ◼ b (second)");
		assert_eq!(drawn[4], "│  ◻ c");
	}

	/// `select` draws a disabled option gray and legible; this one strikes it through as well.
	#[test]
	fn a_disabled_option_is_struck_through() {
		let mut list = options(&["a", "b"]);
		list[1] = list[1].clone().with_disabled(true);
		let prompt = Prompt::new(MultiSelectState::new(list));
		let line = &MultiSelectWidget::new(&prompt, "foo").frame().lines[3];
		assert_eq!(line.spans[2].style, Theme::clack().styles.option_disabled);
		assert_eq!(
			line.spans[4].style,
			Theme::clack().styles.option_disabled_label
		);
	}

	/// Two rows whatever else is off, and the second is indented by three spaces rather than barred.
	#[test]
	fn an_error_frame_puts_the_advice_under_the_message() {
		let mut prompt =
			Prompt::new(MultiSelectState::new(options(&["a"]))).with_validator(required::<String>);
		prompt.key(None, &Key::named(KeyName::Return));
		assert_eq!(prompt.status(), Status::Error);
		let widget = MultiSelectWidget::new(&prompt, "foo");
		assert_eq!(
			drawn(&widget),
			[
				"│",
				"▲  foo",
				"│  ◻ a",
				"└  Please select at least one option.",
				"   Press  space  to select,  enter  to submit",
				"",
			]
		);

		// An error Frame is never the last one a Scenario draws, so the Grid comparison never sees
		// it and the stream comparison strips the styles off it. These are what stand in for that.
		let styles = Theme::clack().styles;
		let lines = widget.frame().lines;
		assert_eq!(
			lines[2].spans[0].style, styles.guide_error,
			"the list's bar"
		);
		assert_eq!(
			lines[3].spans[0].style, styles.guide_error,
			"the closing bar"
		);
		assert_eq!(lines[3].spans[2].style, styles.error, "the message");
		assert_eq!(lines[4].spans[1].style, styles.error_hint, "the advice");
		assert_eq!(lines[4].spans[2].text, " space ");
		// Spelled out rather than compared to the Theme's own constant, which a mutation of that
		// constant would satisfy: a chip is inverse text on a white ground or it is not a chip.
		let chip = lines[4].spans[2].style;
		assert!(
			chip.add_modifier.contains(Modifier::REVERSED),
			"the chip is inverse"
		);
		assert_eq!(chip.bg, Some(ratatui_core::style::Color::Gray), "on white");
	}

	/// The advice takes two rows, and they are held back from the list like any other footer.
	#[test]
	fn an_error_frame_counts_its_advice_against_the_list() {
		let list: Vec<SelectOption<String>> = (0..10)
			.map(|i| SelectOption::new(format!("Option {i}")))
			.collect();
		let mut prompt =
			Prompt::new(MultiSelectState::new(list)).with_validator(required::<String>);
		prompt.key(None, &Key::named(KeyName::Return));
		let widget = MultiSelectWidget::new(&prompt, "foo").with_rows(10);
		let drawn = drawn(&widget);
		assert_eq!(drawn[drawn.len() - 4], "│  ...");
		// And the count, because a `...` appears whether the advice was counted or not.
		assert_eq!(drawn.iter().filter(|row| row.contains("Option")).count(), 3);
	}

	#[test]
	fn an_unguided_error_frame_keeps_the_three_spaces() {
		let mut prompt =
			Prompt::new(MultiSelectState::new(options(&["a"]))).with_validator(required::<String>);
		prompt.key(None, &Key::named(KeyName::Return));
		let widget = MultiSelectWidget::new(&prompt, "foo").with_guide(false);
		let drawn = drawn(&widget);
		assert_eq!(drawn[2], "Please select at least one option.");
		assert_eq!(drawn[3], "   Press  space  to select,  enter  to submit");
	}

	#[test]
	fn without_a_guide_the_list_sits_in_the_margin() {
		let prompt = multiselect(&["a"]);
		let widget = MultiSelectWidget::new(&prompt, "foo").with_guide(false);
		assert_eq!(
			drawn(&widget),
			[
				"◆  foo",
				"◻ a",
				"↑/↓ to navigate • Space: select • Enter: confirm",
				"",
			]
		);
	}

	#[test]
	fn an_empty_footer_is_still_two_rows() {
		let prompt = multiselect(&["a"]);
		let widget = MultiSelectWidget::new(&prompt, "foo")
			.with_guide(false)
			.with_instructions(false);
		assert_eq!(drawn(&widget), ["◆  foo", "◻ a", "", ""]);
	}

	/// The strikethrough a break leaves open is a property of the text either side of it, not of the
	/// value as a whole. Two labels wide enough to break at the boundary between them: the label's own
	/// `9m` was closed there, so nothing survives onto the bar that follows.
	#[test]
	fn a_break_at_a_label_boundary_leaves_nothing_open() {
		let list = vec![
			SelectOption::labelled("a".to_string(), "abcdefghij"),
			SelectOption::labelled("b".to_string(), "klm"),
		];
		let mut prompt = Prompt::new(MultiSelectState::new(list));
		space(&mut prompt);
		press(&mut prompt, KeyName::Down);
		space(&mut prompt);
		press(&mut prompt, KeyName::Escape);

		let widget = MultiSelectWidget::new(&prompt, "foo").with_columns(GUIDE_PREFIX_LENGTH + 10);
		let lines = widget.frame().lines;
		assert_eq!(drawn(&widget)[2], "│  abcdefghij");
		assert_eq!(drawn(&widget)[3], "│  , klm");
		assert_eq!(
			lines[3].spans[0].style,
			Theme::clack().styles.guide,
			"the break fell between a label and the separator, where the strikethrough was closed"
		);
	}

	/// And the other way: a break *inside* a label is inside its `9m`, which was opened before the
	/// row and is not reopened per row, so the bar beside the next row is drawn struck through.
	#[test]
	fn a_break_inside_a_label_strikes_the_bar_beside_the_next_row() {
		let list = vec![SelectOption::labelled("a".to_string(), "abcdefghij klm")];
		let mut prompt = Prompt::new(MultiSelectState::new(list));
		space(&mut prompt);
		press(&mut prompt, KeyName::Escape);

		let widget = MultiSelectWidget::new(&prompt, "foo").with_columns(GUIDE_PREFIX_LENGTH + 10);
		let lines = widget.frame().lines;
		// The leading space is upstream's `trim: false` keeping the space the row broke on.
		assert_eq!(drawn(&widget)[3], "│   klm");
		assert!(
			lines[3].spans[0]
				.style
				.add_modifier
				.contains(Modifier::CROSSED_OUT),
			"the bar of a continuation row is drawn inside the strikethrough"
		);
		// And the dim is not, because `wrapAnsi` closes and reopens that one per row.
		assert!(!lines[3].spans[0].style.add_modifier.contains(Modifier::DIM));
	}

	/// The list is cut to the terminal, and the error footer's two rows are counted as the
	/// instruction footer's would be.
	#[test]
	fn a_short_terminal_cuts_the_list() {
		let list: Vec<SelectOption<String>> = (0..10)
			.map(|i| SelectOption::new(format!("Option {i}")))
			.collect();
		let prompt = Prompt::new(MultiSelectState::new(list));
		let widget = MultiSelectWidget::new(&prompt, "foo").with_rows(12);
		let drawn = drawn(&widget);
		assert_eq!(drawn[2], "│  ◻ Option 0");
		assert_eq!(drawn[drawn.len() - 4], "│  ...");
	}
}
