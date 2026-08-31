//! Ported from `@clack/core`'s `prompts/select.ts` and `@clack/prompts`' `select.ts`.
//!
//! The first Prompt with a list in it. The state is four lines of upstream — a cursor walked by
//! [`find_cursor`](crate::cursor::find_cursor) — and everything interesting is in the drawing:
//! the list is cut to the terminal by [`LimitOptions`], and how many rows it may occupy depends on
//! how tall the title and the footer turned out, which depends on how the message wrapped.
//!
//! # Three widths, and none of them the Frame's
//!
//! Upstream reads `columns` and `rows` off the Prompt's own output stream — `getColumns(opts.output)`
//! — while `Prompt.render` wraps the finished Frame against `process.stdout.columns`. The two are one
//! terminal outside a test harness. Within this widget the stream's width is taken off *twice more*:
//! the message wraps to `columns - 13` and each option wraps to `columns - 13` again, both because a
//! styled bar and two spaces are thirteen characters and three columns (ADR-0019). So a `select` in
//! an 80-column terminal breaks its text at 67 and draws it at 3.
//!
//! # Two things reproduced rather than corrected
//!
//! - **A Guide that turns itself off leaves its continuation bars behind.** `withGuide: false` drops
//!   the bar above the title and the prefix beside the options, but the bar every message row after
//!   the first is prefixed with is passed to `wrapTextWithPrefix` unconditionally. An unguided
//!   `select` with a one-line message therefore looks unguided, and one whose message wraps does not.
//! - **That bar is not the Guide's colour.** It is `symbolBar(state)`, so it is cyan while the Prompt
//!   is open and green once it is submitted — where `confirm` and `text` draw theirs gray.

use std::fmt::Display;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::Style;
use ratatui_core::widgets::Widget;

use crate::cursor::find_cursor;
use crate::frame::{Frame, Line, Span};
use crate::limit_options::LimitOptions;
use crate::prompt::{Prompt, PromptState, Status};
use crate::settings::Action;
use crate::theme::Theme;
use crate::wrap::{leaked, wrap};

/// The columns upstream takes off the terminal for a prefix that draws three of them.
///
/// The same thirteen as [`confirm::GUIDE_PREFIX_LENGTH`](crate::confirm::GUIDE_PREFIX_LENGTH), and
/// for the same reason: `ESC[36m`, the bar, `ESC[39m`, and two spaces. `select` subtracts it in two
/// places — once for the message and once inside the option list.
pub const GUIDE_PREFIX_LENGTH: usize = 13;

/// `SELECT_INSTRUCTIONS`, the footer under the list. The keys are dim and the verbs are not.
pub const INSTRUCTIONS: [(&str, &str); 2] = [("↑/↓", " to navigate"), ("Enter:", " confirm")];

/// The separator upstream joins the instructions with.
pub const INSTRUCTION_SEPARATOR: &str = " • ";

/// One choice in a list: a value, the text it is drawn as, and whether it can be chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption<T> {
	value: T,
	label: String,
	hint: Option<String>,
	disabled: bool,
}

impl<T: Display> SelectOption<T> {
	/// An option labelled by its own value — upstream's `option.label ?? String(option.value)`.
	pub fn new(value: T) -> Self {
		let label = value.to_string();
		Self::labelled(value, label)
	}
}

impl<T> SelectOption<T> {
	/// An option with a label of its own. The only way to build one whose value cannot be printed,
	/// which is upstream's rule too: its `Option<Value>` type requires a `label` for anything that is
	/// not a string, a number or a boolean.
	pub fn labelled(value: T, label: impl Into<String>) -> Self {
		Self {
			value,
			label: label.into(),
			hint: None,
			disabled: false,
		}
	}

	/// `hint`: a note beside the option, drawn in brackets.
	///
	/// Only ever seen on the active option and on a disabled one — the inactive branch drops it.
	pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
		self.hint = Some(hint.into());
		self
	}

	/// `disabled`: visible, greyed out, and skipped by the cursor.
	pub fn with_disabled(mut self, disabled: bool) -> Self {
		self.disabled = disabled;
		self
	}

	pub fn value(&self) -> &T {
		&self.value
	}

	/// The text drawn for this option. May contain line breaks, in which case it is several rows.
	pub fn label(&self) -> &str {
		&self.label
	}

	pub fn hint(&self) -> Option<&str> {
		self.hint.as_deref()
	}

	pub fn disabled(&self) -> bool {
		self.disabled
	}
}

/// The state of a `select`: a list, and which of it the cursor is on.
#[derive(Clone, Debug)]
pub struct SelectState<T> {
	options: Vec<SelectOption<T>>,
	cursor: usize,
}

impl<T> SelectState<T> {
	/// The list, with the cursor on the first option that can be chosen.
	pub fn new(options: Vec<SelectOption<T>>) -> Self {
		let mut state = Self { options, cursor: 0 };
		state.settle_cursor(0);
		state
	}

	/// `initialValue`: open on the option holding this value, if the list has one.
	///
	/// A value the list does not hold leaves the cursor where upstream's `findIndex` leaves it — at
	/// the first option, because `-1` is turned into `0` rather than used.
	pub fn with_initial_value(mut self, value: &T) -> Self
	where
		T: PartialEq,
	{
		let at = self
			.options
			.iter()
			.position(|option| &option.value == value)
			.unwrap_or(0);
		self.settle_cursor(at);
		self
	}

	/// Which option is under the cursor. Past the end only where the list is empty.
	pub fn cursor(&self) -> usize {
		self.cursor
	}

	pub fn options(&self) -> &[SelectOption<T>] {
		&self.options
	}

	/// The option under the cursor, or none where the list is empty.
	pub fn selected(&self) -> Option<&SelectOption<T>> {
		self.options.get(self.cursor)
	}

	/// The constructor's own cursor rule: land on `at`, unless it is disabled, in which case walk
	/// forwards from it. Upstream does this once, in the constructor, and it is the only place a
	/// cursor moves without a keypress.
	fn settle_cursor(&mut self, at: usize) {
		self.cursor = if self.options.get(at).is_some_and(|o| o.disabled) {
			find_cursor(at, 1, &self.options, |o| o.disabled)
		} else {
			at
		};
	}
}

impl<T> PromptState for SelectState<T> {
	type Value = T;

	/// `super(opts, false)`. A `select` navigates rather than types, so `j` and `k` reach it.
	const TRACKS_INPUT: bool = false;

	/// Upstream reads `left` and `up` as one direction and `down` and `right` as the other, which is
	/// why a `select` drawn as a single column still answers to the horizontal arrows.
	fn cursor(&mut self, action: Action) {
		let delta = match action {
			Action::Left | Action::Up => -1,
			Action::Down | Action::Right => 1,
			_ => return,
		};
		self.cursor = find_cursor(self.cursor, delta, &self.options, |o| o.disabled);
	}

	/// `changeValue`: the value is whatever the cursor is on, and nothing where the list is empty.
	fn value(&self) -> Option<&T> {
		self.options.get(self.cursor).map(|option| &option.value)
	}
}

/// A `select` Prompt drawn as a Frame — the `render` callback of `@clack/prompts`' `select()`.
pub struct SelectWidget<'a, T> {
	prompt: &'a Prompt<SelectState<T>>,
	message: &'a str,
	theme: &'a Theme,
	/// The Prompt's own output stream, which is what the message and the list are measured against.
	/// See the module docs: it is not the width the Frame is wrapped to.
	columns: usize,
	rows: usize,
	max_items: Option<usize>,
	show_instructions: bool,
	/// `opts.withGuide ?? settings.withGuide`.
	with_guide: Option<bool>,
}

impl<'a, T> SelectWidget<'a, T> {
	pub fn new(prompt: &'a Prompt<SelectState<T>>, message: &'a str) -> Self {
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

	/// The terminal's width, as the Prompt's own stream reports it.
	pub fn with_columns(mut self, columns: usize) -> Self {
		self.columns = columns;
		self
	}

	/// The terminal's height, which decides how much of the list is drawn.
	pub fn with_rows(mut self, rows: usize) -> Self {
		self.rows = rows;
		self
	}

	/// `maxItems`: a ceiling on the options drawn, over and above the terminal's.
	pub fn with_max_items(mut self, max_items: usize) -> Self {
		self.max_items = Some(max_items);
		self
	}

	/// `showInstructions`: the `↑/↓ to navigate` footer. On by default.
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
			// The settled value: the label alone, wrapped under the same prefix and dimmed — or
			// struck through as well, once it is a value nobody asked for.
			Status::Submit | Status::Cancel => {
				let style = if status == Status::Submit {
					styles.submitted
				} else {
					styles.cancelled
				};
				let label = state.selected().map(SelectOption::label).unwrap_or("");
				let width = self.width(guide);
				// What a break leaves open behind it — see [`leaked`]. Nothing, on a submitted value.
				let leaked = if status == Status::Cancel {
					leaked(style)
				} else {
					Style::new()
				};
				for (index, row) in wrap(label, width).split('\n').enumerate() {
					let mut line = Line::blank();
					if guide {
						// The prefix of the first row is written before the value's styling opens; every
						// row after it is written after, and inherits whatever is still open.
						let carried = if index == 0 { Style::new() } else { leaked };
						line.push(Span::styled(symbols.bar, styles.guide.patch(carried)));
						line.push(Span::styled("  ", carried));
					}
					line.push(Span::styled(row, style));
					frame.push(line);
				}
				if status == Status::Cancel && guide {
					frame.push(Line::from(Span::styled(symbols.bar, styles.guide)));
				}
			}

			// A `select` takes no validator, so `error` is unreachable and upstream's `switch` falls
			// into `default` here as it does there.
			_ => {
				let footer = self.footer(guide);
				let row_padding = title_rows + footer.len() + 1;
				let column_padding = if guide { GUIDE_PREFIX_LENGTH } else { 0 };

				let mut limit = LimitOptions::new(state.options(), state.cursor())
					.with_columns(self.columns)
					.with_rows(self.rows)
					.with_column_padding(column_padding)
					.with_row_padding(row_padding)
					.with_theme(self.theme);
				if let Some(max_items) = self.max_items {
					limit = limit.with_max_items(max_items);
				}

				for row in limit.lines(|option, active| self.option(option, active)) {
					let mut line = Line::blank();
					if guide {
						line.push(Span::styled(symbols.bar, styles.guide_active));
						line.push(Span::raw("  "));
					}
					for span in row.spans {
						line.push(span);
					}
					frame.push(line);
				}

				// `${…}\n${footerText}\n`: an empty footer is still a row, because joining nothing
				// gives an empty string and the newline either side of it is written regardless.
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

	/// `wrapTextWithPrefix(output, message, titlePrefixBar, titlePrefix)`, plus the bar above it.
	///
	/// The first row carries the step symbol; every row after it carries a bar in the step's colour,
	/// Guide or no Guide — see the module docs.
	fn title(&self, status: Status) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let guide = self.guided();

		let mut lines = Vec::new();
		if guide {
			lines.push(Line::from(Span::styled(symbols.bar, styles.guide)));
		}

		let wrapped = wrap(
			self.message,
			self.columns.saturating_sub(GUIDE_PREFIX_LENGTH),
		);
		for (index, text) in wrapped.split('\n').enumerate() {
			let mut line = Line::blank();
			if index == 0 {
				line.push(self.theme.step(status));
			} else {
				line.push(self.theme.bar(status));
			}
			line.push(Span::raw("  "));
			line.push(Span::styled(text, styles.message));
			lines.push(line);
		}
		lines
	}

	/// `formatInstructionFooter`, and what stands in for it when there are no instructions to show.
	///
	/// Three shapes, and the empty one is a shape: an unguided `select` with the footer turned off
	/// draws nothing under its list at all.
	fn footer(&self, guide: bool) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;

		if !self.show_instructions {
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

	/// `opt(option, state)`: one option, as the rows it occupies before the list wraps them.
	///
	/// The three branches differ in more than colour. The active one draws its label unstyled and
	/// *whole*, so a label with a line break in it keeps the break but takes the radio only on its
	/// first row; the other two style each row separately. And the hint is drawn for the active and
	/// the disabled option but not the inactive one, always after the last row of the label.
	fn option(&self, option: &SelectOption<T>, active: bool) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;

		let (radio, radio_style, label_style, hint) = if option.disabled {
			(
				symbols.radio_inactive,
				styles.option_disabled,
				styles.option_disabled,
				option.hint(),
			)
		} else if active {
			(
				symbols.radio_active,
				styles.radio_selected,
				styles.message,
				option.hint(),
			)
		} else {
			(
				symbols.radio_inactive,
				styles.radio_unselected,
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
					line.push(Span::styled(radio, radio_style));
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

	/// The width a settled value is wrapped to: the terminal, less the prefix it is drawn under.
	///
	/// Unguided there is no prefix, and no thirteen columns lost to one either.
	fn width(&self, guide: bool) -> usize {
		if guide {
			self.columns.saturating_sub(GUIDE_PREFIX_LENGTH)
		} else {
			self.columns
		}
	}
}

/// The default Theme, as somewhere to borrow from.
static THEME: Theme = Theme::clack();

impl<T> Widget for &SelectWidget<'_, T> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		(&self.frame()).render(area, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::line_editor::{Key, KeyName};
	use crate::prompt::Outcome;

	fn options(labels: &[&str]) -> Vec<SelectOption<String>> {
		labels
			.iter()
			.map(|label| SelectOption::new(label.to_string()))
			.collect()
	}

	fn select(labels: &[&str]) -> Prompt<SelectState<String>> {
		Prompt::new(SelectState::new(options(labels)))
	}

	fn press(prompt: &mut Prompt<SelectState<String>>, name: KeyName) {
		prompt.key(None, &Key::named(name));
	}

	fn typed(prompt: &mut Prompt<SelectState<String>>, c: char) {
		prompt.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
	}

	fn answer(prompt: &Prompt<SelectState<String>>) -> Option<String> {
		match prompt.outcome() {
			Some(Outcome::Submitted(value)) => value.cloned(),
			_ => None,
		}
	}

	#[test]
	fn a_select_opens_on_the_first_option() {
		let mut prompt = select(&["a", "b", "c"]);
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt).as_deref(), Some("a"));
	}

	#[test]
	fn the_arrows_walk_the_list_both_ways() {
		let mut prompt = select(&["a", "b", "c"]);
		press(&mut prompt, KeyName::Down);
		assert_eq!(prompt.state().cursor(), 1);
		press(&mut prompt, KeyName::Up);
		assert_eq!(prompt.state().cursor(), 0);
		// Upstream reads the horizontal arrows as the vertical ones.
		press(&mut prompt, KeyName::Right);
		assert_eq!(prompt.state().cursor(), 1);
		press(&mut prompt, KeyName::Left);
		assert_eq!(prompt.state().cursor(), 0);
	}

	#[test]
	fn the_vim_aliases_reach_a_prompt_that_does_not_type() {
		let mut prompt = select(&["a", "b"]);
		typed(&mut prompt, 'j');
		assert_eq!(prompt.state().cursor(), 1);
		typed(&mut prompt, 'k');
		assert_eq!(prompt.state().cursor(), 0);
	}

	#[test]
	fn an_initial_value_opens_the_list_on_it() {
		let state =
			SelectState::new(options(&["a", "b", "c"])).with_initial_value(&"c".to_string());
		let mut prompt = Prompt::new(state);
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt).as_deref(), Some("c"));
	}

	#[test]
	fn an_initial_value_the_list_does_not_hold_is_the_first_option() {
		let state = SelectState::new(options(&["a", "b"])).with_initial_value(&"z".to_string());
		assert_eq!(state.cursor(), 0);
	}

	#[test]
	fn a_disabled_first_option_is_stepped_off_before_the_prompt_opens() {
		let mut list = options(&["a", "b"]);
		list[0] = list[0].clone().with_disabled(true);
		let state = SelectState::new(list);
		assert_eq!(state.cursor(), 1);
	}

	#[test]
	fn an_empty_list_has_no_value_to_submit() {
		let mut prompt: Prompt<SelectState<String>> = Prompt::new(SelectState::new(Vec::new()));
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.outcome(), Some(Outcome::Submitted(None)));
	}

	#[test]
	fn escape_cancels_without_moving_the_cursor() {
		// Unlike `confirm`, whose cursor listener flips its answer on the way out — this one reads
		// the action and an `escape` is neither direction.
		let mut prompt = select(&["a", "b"]);
		press(&mut prompt, KeyName::Down);
		press(&mut prompt, KeyName::Escape);
		assert_eq!(prompt.outcome(), Some(Outcome::Cancelled));
		assert_eq!(prompt.state().cursor(), 1);
	}

	// --- The widget -----------------------------------------------------------------------------

	fn drawn(widget: &SelectWidget<'_, String>) -> Vec<String> {
		widget
			.frame()
			.lines
			.iter()
			.map(|line| line.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn an_opening_frame_is_the_title_the_list_and_the_footer() {
		let prompt = select(&["a", "b"]);
		let widget = SelectWidget::new(&prompt, "foo");
		assert_eq!(
			drawn(&widget),
			[
				"│",
				"◆  foo",
				"│  ● a",
				"│  ○ b",
				"│  ↑/↓ to navigate • Enter: confirm",
				"└",
				"",
			]
		);
	}

	#[test]
	fn a_submitted_frame_is_the_label_alone() {
		let mut prompt = select(&["a", "b"]);
		press(&mut prompt, KeyName::Down);
		press(&mut prompt, KeyName::Return);
		let widget = SelectWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◇  foo", "│  b"]);
	}

	#[test]
	fn a_cancelled_frame_closes_with_a_second_guide() {
		let mut prompt = select(&["a", "b"]);
		press(&mut prompt, KeyName::Escape);
		let widget = SelectWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "■  foo", "│  a", "│"]);
	}

	#[test]
	fn a_hint_is_drawn_beside_the_active_option_and_nowhere_else() {
		let list = vec![
			SelectOption::new("a".to_string()).with_hint("first"),
			SelectOption::new("b".to_string()).with_hint("second"),
		];
		let prompt = Prompt::new(SelectState::new(list));
		let widget = SelectWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  ● a (first)");
		assert_eq!(drawn(&widget)[3], "│  ○ b");
	}

	#[test]
	fn a_disabled_option_keeps_its_hint() {
		let list = vec![
			SelectOption::new("a".to_string()),
			SelectOption::new("b".to_string())
				.with_hint("nope")
				.with_disabled(true),
		];
		let prompt = Prompt::new(SelectState::new(list));
		let widget = SelectWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[3], "│  ○ b (nope)");
	}

	#[test]
	fn a_multi_line_label_takes_the_radio_on_its_first_row_only() {
		let list = vec![
			SelectOption::labelled("a".to_string(), "one\ntwo"),
			SelectOption::new("b".to_string()),
		];
		let prompt = Prompt::new(SelectState::new(list));
		let widget = SelectWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  ● one");
		assert_eq!(drawn(&widget)[3], "│  two");
	}

	#[test]
	fn without_a_guide_the_list_sits_in_the_margin() {
		let prompt = select(&["a", "b"]);
		let widget = SelectWidget::new(&prompt, "foo").with_guide(false);
		assert_eq!(
			drawn(&widget),
			[
				"◆  foo",
				"● a",
				"○ b",
				"↑/↓ to navigate • Enter: confirm",
				""
			]
		);
	}

	/// The quirk the module docs name: the bar the message's later rows are prefixed with is not the
	/// Guide and is not turned off with it.
	#[test]
	fn an_unguided_select_still_bars_the_rows_of_a_wrapped_message() {
		let prompt = select(&["a"]);
		let widget = SelectWidget::new(&prompt, "one\ntwo").with_guide(false);
		assert_eq!(drawn(&widget)[0], "◆  one");
		assert_eq!(drawn(&widget)[1], "│  two");
	}

	#[test]
	fn the_footer_can_be_turned_off_on_its_own() {
		let prompt = select(&["a"]);
		let widget = SelectWidget::new(&prompt, "foo").with_instructions(false);
		assert_eq!(drawn(&widget), ["│", "◆  foo", "│  ● a", "└", ""]);
	}

	/// With neither Guide nor footer there is nothing under the list, and upstream still writes the
	/// two newlines that were going to go either side of it.
	#[test]
	fn an_empty_footer_is_still_two_rows() {
		let prompt = select(&["a"]);
		let widget = SelectWidget::new(&prompt, "foo")
			.with_guide(false)
			.with_instructions(false);
		assert_eq!(drawn(&widget), ["◆  foo", "● a", "", ""]);
	}

	/// The list is cut to the terminal, and the rows the title and footer take are counted first.
	#[test]
	fn a_short_terminal_cuts_the_list() {
		let labels: Vec<String> = (0..10).map(|i| format!("Option {i}")).collect();
		let list: Vec<SelectOption<String>> = labels
			.iter()
			.map(|label| SelectOption::new(label.clone()))
			.collect();
		let prompt = Prompt::new(SelectState::new(list));
		let widget = SelectWidget::new(&prompt, "foo").with_rows(12);
		let drawn = drawn(&widget);
		assert_eq!(drawn[2], "│  ● Option 0");
		assert_eq!(drawn[drawn.len() - 4], "│  ...");
	}

	/// Thirteen columns for a prefix three columns wide, twice over — the message breaks early and
	/// so does the option beside it.
	#[test]
	fn the_message_and_the_options_are_both_wrapped_ten_columns_early() {
		let list = vec![SelectOption::labelled(
			"a".to_string(),
			"one two three four five six seven",
		)];
		let prompt = Prompt::new(SelectState::new(list));
		let widget = SelectWidget::new(&prompt, "one two three four five six seven")
			.with_columns(40)
			.with_rows(40);
		let drawn = drawn(&widget);
		assert_eq!(drawn[1], "◆  one two three four five six");
		assert_eq!(drawn[2], "│   seven");
		// The trailing space is upstream's `trim: false` keeping the space a row broke on.
		assert_eq!(drawn[3], "│  ● one two three four five ");
		assert_eq!(drawn[4], "│  six seven");
	}
}
