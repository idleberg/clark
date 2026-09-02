//! Ported from `@clack/core`'s `prompts/select-key.ts` and `@clack/prompts`' `select-key.ts`.
//!
//! A list whose options are chosen by pressing a letter rather than by walking to one. The cursor
//! exists — it decides which option is drawn highlighted — but nothing moves it: it is set once from
//! `initialValue` and then stands still for the life of the Prompt.
//!
//! # The one Prompt that settles from inside its `key` listener
//!
//! Every other list Prompt waits for `return`. This one submits the moment a key matches, and it
//! does so by setting `state = 'submit'` and emitting `submit` from inside the listener — which
//! resolves the promise, and therefore shows the cursor, before `onKeypress` has reached the render
//! that draws the settled Frame. It does not `close()` there, so the newline still lands last. That
//! is [`PromptState::submits_from_key`], and it is a smaller thing than `confirm`'s
//! [`CONFIRMS_ON_KEY`](PromptState::CONFIRMS_ON_KEY) — one sequence out of order, not three.
//!
//! # Three things reproduced rather than corrected
//!
//! - **A cancelled `selectKey` always shows the first option.** The `cancel` branch draws
//!   `this.options[0]`, never the option the cursor is on and never the one a key matched. So a
//!   Prompt cancelled after nothing at all and one cancelled with the cursor elsewhere are drawn
//!   identically.
//! - **A submitted one falls back to the first option too.** `find(o => o.value === this.value)`
//!   against an unset value finds nothing, and `?? opts.options[0]` catches it — so a bare `return`,
//!   which submits `undefined`, is drawn as though the first option had been chosen.
//! - **`initialValue` is matched against the option's first character, not its value.** Upstream
//!   builds a list of initials and then asks `keys.indexOf(opts.initialValue)`, so an
//!   `initialValue` of `"apple"` never matches and one of `"a"` matches whichever option's value
//!   begins with an `a`. Case-insensitively the initials are folded and the `initialValue` is not,
//!   so an uppercase one cannot match at all.
//!
//! The message is not wrapped here, unlike every other Prompt with a list in it: upstream
//! interpolates it into the title and leaves the Frame's own wrapping to deal with it. Only the
//! options and the settled value go through `wrapTextWithPrefix`.

use std::fmt::Display;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::Style;
use ratatui_core::widgets::Widget;

use crate::frame::{Frame, Line, Span};
use crate::line_editor::Key;
use crate::prompt::{Prompt, PromptState, Status};
use crate::select::{GUIDE_PREFIX_LENGTH, SelectOption, plain_title};
use crate::theme::Theme;
use crate::wrap::{leaked, wrap};

/// The state of a `selectKey`: a list, which of it is highlighted, and which key matched.
#[derive(Clone, Debug)]
pub struct SelectKeyState<T> {
	options: Vec<SelectOption<T>>,
	/// `keys`: each option's first character, folded unless the Prompt is case-sensitive. None where
	/// the value printed as the empty string, which matches nothing.
	keys: Vec<Option<String>>,
	case_sensitive: bool,
	cursor: usize,
	selected: Option<usize>,
}

impl<T: Display> SelectKeyState<T> {
	/// The list, case-insensitive as upstream's default is, with the first option highlighted.
	pub fn new(options: Vec<SelectOption<T>>) -> Self {
		let mut state = Self {
			options,
			keys: Vec::new(),
			case_sensitive: false,
			cursor: 0,
			selected: None,
		};
		state.index();
		state
	}

	/// `caseSensitive`: whether `A` and `a` are one key or two.
	///
	/// Off by default, and it decides both halves of the comparison — the initials are folded when
	/// the list is built and the keypress is folded when it arrives.
	pub fn with_case_sensitive(mut self, case_sensitive: bool) -> Self {
		self.case_sensitive = case_sensitive;
		self.index();
		self
	}

	/// `initialValue`: which option opens highlighted.
	///
	/// Matched against the initials rather than the values — see the module docs. Anything that does
	/// not match leaves the cursor at the first option, because upstream's `-1` is clamped to zero.
	pub fn with_initial_value(mut self, value: &str) -> Self {
		self.cursor = self
			.keys
			.iter()
			.position(|key| key.as_deref() == Some(value))
			.unwrap_or(0);
		self
	}

	/// The initials, rebuilt. Called wherever the list or the folding changes.
	fn index(&mut self) {
		let case_sensitive = self.case_sensitive;
		self.keys = self
			.options
			.iter()
			.map(|option| {
				let initial = option.value().to_string().chars().next()?;
				Some(if case_sensitive {
					initial.to_string()
				} else {
					initial.to_lowercase().to_string()
				})
			})
			.collect();
	}

	/// Which option is drawn highlighted. Never moves after the Prompt opens.
	pub fn cursor(&self) -> usize {
		self.cursor
	}

	pub fn options(&self) -> &[SelectOption<T>] {
		&self.options
	}

	/// The option a key matched, if one has.
	pub fn selected(&self) -> Option<&SelectOption<T>> {
		self.selected.and_then(|at| self.options.get(at))
	}
}

impl<T: Display> PromptState for SelectKeyState<T> {
	type Value = T;

	/// `super(opts, false)`. It navigates rather than types — though there is nothing to navigate.
	const TRACKS_INPUT: bool = false;

	/// The whole of upstream's listener. An unmatched key is not an error, it is nothing at all.
	///
	/// Upstream opens with `if (!key) return`, which turns away the empty string `escape` arrives
	/// as. There is nothing here to turn away: an option whose value is empty has no initial, so it
	/// is indexed as `None` and an empty keypress matches it no more than any other key does — which
	/// is the same thing upstream's `keys.includes('')` decides one line further down.
	fn key(&mut self, s: Option<&str>, _key: &Key) {
		let Some(pressed) = s else {
			return;
		};
		let pressed = if self.case_sensitive {
			pressed.to_owned()
		} else {
			pressed.to_lowercase()
		};
		if let Some(at) = self
			.keys
			.iter()
			.position(|key| key.as_deref() == Some(&pressed))
		{
			self.selected = Some(at);
		}
	}

	fn submits_from_key(&self) -> bool {
		self.selected.is_some()
	}

	/// `this.value`, which is set by the listener and by nothing else — so a `return` that reaches
	/// the Prompt before any key has matched submits no value at all.
	fn value(&self) -> Option<&T> {
		self.selected().map(SelectOption::value)
	}
}

/// A `selectKey` Prompt drawn as a Frame — the `render` callback of `@clack/prompts`' `selectKey()`.
pub struct SelectKeyWidget<'a, T: Display> {
	prompt: &'a Prompt<SelectKeyState<T>>,
	message: &'a str,
	theme: &'a Theme,
	/// The Prompt's own output stream's width, which is what the options are measured against — not
	/// the width the Frame is wrapped to. Same three widths as `select` (ADR-0021).
	columns: usize,
	with_guide: Option<bool>,
}

impl<'a, T: Display> SelectKeyWidget<'a, T> {
	pub fn new(prompt: &'a Prompt<SelectKeyState<T>>, message: &'a str) -> Self {
		Self {
			prompt,
			message,
			theme: &THEME,
			columns: 80,
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
		for line in self.title(status, guide) {
			frame.push(line);
		}

		match status {
			// One option, dimmed — the one a key matched, or the first if none did.
			Status::Submit => {
				let label = state
					.selected()
					.or_else(|| state.options().first())
					.map(SelectOption::label)
					.unwrap_or("");
				for line in self.value_rows(label, styles.submitted, guide) {
					frame.push(line);
				}
			}

			// The *first* option, struck through, whatever the cursor was on. See the module docs.
			Status::Cancel => {
				let label = state
					.options()
					.first()
					.map(SelectOption::label)
					.unwrap_or("");
				for line in self.value_rows(label, styles.cancelled, guide) {
					frame.push(line);
				}
				if guide {
					frame.push(Line::from(Span::styled(symbols.bar, styles.guide)));
				}
			}

			// The list. Every option is drawn — a `selectKey` has no `limitOptions` and no footer,
			// so a list taller than the terminal simply scrolls off it.
			_ => {
				let width = self.width(guide);
				for (index, option) in state.options().iter().enumerate() {
					let spans = self.option(option, index == state.cursor());
					for paragraph in spans.paragraphs() {
						for row in paragraph.wrap(width) {
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
					}
				}
				// `\n${defaultPrefixEnd}\n`: unguided the end bar is the empty string, and the row
				// it was going to sit on is written either way.
				frame.push(if guide {
					Line::from(Span::styled(symbols.bar_end, styles.guide_active))
				} else {
					Line::blank()
				});
				frame.push(Line::blank());
			}
		}

		frame
	}

	/// The bar above the message, and the message itself.
	///
	/// Unwrapped, unlike `select`'s — so a message with a line break in it is two rows and the
	/// second carries no prefix at all.
	fn title(&self, status: Status, guide: bool) -> Vec<Line> {
		plain_title(self.theme, self.message, status, guide)
	}

	/// `opt(option, state)` for the two states a drawn list has: the highlighted one and the rest.
	///
	/// They differ only in the chip's colours. The hint is drawn on both, which is where this parts
	/// company with `select` — there the inactive branch drops it.
	fn option(&self, option: &SelectOption<T>, active: bool) -> Line {
		let styles = &self.theme.styles;
		let chip = if active {
			styles.key_active
		} else {
			styles.key_inactive
		};

		let mut line = Line::blank();
		line.push(Span::styled(format!(" {} ", option.value()), chip));
		line.push(Span::raw(" "));
		line.push(Span::styled(option.label(), styles.message));
		if let Some(hint) = option.hint() {
			line.push(Span::raw(" "));
			line.push(Span::styled(format!("({hint})"), styles.hint));
		}
		line
	}

	/// A settled value: one label, wrapped under the gray prefix and styled as a whole.
	///
	/// The same shape as `select`'s, including the leak — a cancelled label's strikethrough is
	/// opened once and closed at the very end, so it is still open across the bars between its rows.
	/// See [`leaked`].
	fn value_rows(&self, label: &str, style: Style, guide: bool) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let open = leaked(style);

		let mut out = Vec::new();
		for (index, row) in wrap(label, self.width(guide)).split('\n').enumerate() {
			let mut line = Line::blank();
			if guide {
				let carried = if index == 0 { Style::new() } else { open };
				line.push(Span::styled(symbols.bar, styles.guide.patch(carried)));
				line.push(Span::styled("  ", carried));
			}
			line.push(Span::styled(row, style));
			out.push(line);
		}
		out
	}

	/// `columns - prefix.length`, where the prefix is a styled bar and two spaces — or nothing.
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

impl<T: Display> Widget for &SelectKeyWidget<'_, T> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		(&self.frame()).render(area, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::line_editor::KeyName;
	use crate::prompt::Outcome;

	fn options(values: &[&str]) -> Vec<SelectOption<String>> {
		values
			.iter()
			.map(|value| {
				SelectOption::labelled(
					value.to_string(),
					format!("Option {}", value.to_uppercase()),
				)
			})
			.collect()
	}

	fn select_key(values: &[&str]) -> Prompt<SelectKeyState<String>> {
		Prompt::new(SelectKeyState::new(options(values)))
	}

	fn typed(prompt: &mut Prompt<SelectKeyState<String>>, c: char) {
		prompt.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
	}

	fn answer(prompt: &Prompt<SelectKeyState<String>>) -> Option<String> {
		match prompt.outcome() {
			Some(Outcome::Submitted(value)) => value.cloned(),
			_ => None,
		}
	}

	fn drawn(widget: &SelectKeyWidget<'_, String>) -> Vec<String> {
		widget
			.frame()
			.lines
			.iter()
			.map(|line| line.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn a_matching_key_submits_where_it_stands() {
		let mut prompt = select_key(&["a", "b"]);
		typed(&mut prompt, 'b');
		assert_eq!(prompt.status(), Status::Submit);
		assert_eq!(answer(&prompt).as_deref(), Some("b"));
	}

	/// The reason [`Prompt::resolved_early`] exists — nothing else in clack reports it.
	#[test]
	fn a_matching_key_resolves_before_the_frame_is_drawn() {
		let mut prompt = select_key(&["a", "b"]);
		typed(&mut prompt, 'z');
		assert!(!prompt.resolved_early());
		typed(&mut prompt, 'a');
		assert!(prompt.resolved_early());
	}

	#[test]
	fn a_key_that_matches_nothing_leaves_the_prompt_open() {
		let mut prompt = select_key(&["a", "b"]);
		typed(&mut prompt, 'z');
		assert!(!prompt.status().is_finished());
		assert_eq!(prompt.state().selected(), None);
	}

	#[test]
	fn keys_are_case_insensitive_by_default() {
		let mut prompt = select_key(&["a", "B"]);
		typed(&mut prompt, 'A');
		assert_eq!(answer(&prompt).as_deref(), Some("a"));

		// And the option's own case is folded too, not only the keypress.
		let mut prompt = select_key(&["a", "B"]);
		typed(&mut prompt, 'b');
		assert_eq!(answer(&prompt).as_deref(), Some("B"));
	}

	#[test]
	fn case_sensitive_tells_the_two_apart() {
		let state = SelectKeyState::new(options(&["a", "A"])).with_case_sensitive(true);
		let mut prompt = Prompt::new(state);
		typed(&mut prompt, 'A');
		assert_eq!(answer(&prompt).as_deref(), Some("A"));

		let state = SelectKeyState::new(options(&["A", "b"])).with_case_sensitive(true);
		let mut prompt = Prompt::new(state);
		typed(&mut prompt, 'a');
		assert!(!prompt.status().is_finished());
	}

	/// `initialValue` is compared with the initials, so it only ever matches one character — and
	/// case-insensitively it cannot match an uppercase one at all. See the module docs.
	#[test]
	fn an_initial_value_is_matched_against_the_first_character() {
		let state = SelectKeyState::new(options(&["apple", "banana"])).with_initial_value("b");
		assert_eq!(state.cursor(), 1);

		let state = SelectKeyState::new(options(&["apple", "banana"])).with_initial_value("banana");
		assert_eq!(state.cursor(), 0);

		let state = SelectKeyState::new(options(&["apple", "Banana"])).with_initial_value("B");
		assert_eq!(state.cursor(), 0);
	}

	/// Every other list Prompt walks; this one has nothing to walk with.
	#[test]
	fn the_arrows_do_not_move_the_cursor() {
		let mut prompt = select_key(&["a", "b"]);
		prompt.key(None, &Key::named(KeyName::Down));
		assert_eq!(prompt.state().cursor(), 0);
	}

	#[test]
	fn escape_cancels_without_matching_a_key() {
		let mut prompt = select_key(&["a", "b"]);
		// `escape` arrives with an empty string rather than with no string at all.
		prompt.key(Some(""), &Key::named(KeyName::Escape));
		assert_eq!(prompt.outcome(), Some(Outcome::Cancelled));
	}

	/// The empty keypress upstream guards against, put against the one list that could confuse it.
	#[test]
	fn an_option_with_no_value_is_not_matched_by_the_empty_keypress() {
		let list = vec![
			SelectOption::labelled(String::new(), "nameless"),
			SelectOption::labelled("b".to_string(), "Option B"),
		];
		let mut prompt = Prompt::new(SelectKeyState::new(list));
		prompt.key(Some(""), &Key::named(KeyName::Char(' ')));
		assert_eq!(prompt.state().selected(), None);
		assert!(!prompt.status().is_finished());
	}

	/// A label with a break in it is two rows, and the chip stays on the first.
	#[test]
	fn a_multi_line_label_takes_the_chip_on_its_first_row_only() {
		let list = vec![
			SelectOption::labelled("a".to_string(), "one\ntwo"),
			SelectOption::labelled("b".to_string(), "Option B"),
		];
		let prompt = Prompt::new(SelectKeyState::new(list));
		let widget = SelectKeyWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│   a  one");
		assert_eq!(drawn(&widget)[3], "│  two");
	}

	/// A bare `return` submits before any key has matched, so there is no value — and yet the Frame
	/// draws the first option, because `?? opts.options[0]` catches the miss.
	#[test]
	fn return_submits_nothing_and_draws_the_first_option() {
		let mut prompt = select_key(&["a", "b"]);
		prompt.key(None, &Key::named(KeyName::Return));
		assert_eq!(prompt.outcome(), Some(Outcome::Submitted(None)));
		let widget = SelectKeyWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◇  foo", "│  Option A"]);
	}

	// --- The widget -----------------------------------------------------------------------------

	#[test]
	fn an_opening_frame_is_the_title_the_list_and_an_end_bar() {
		let prompt = select_key(&["a", "b"]);
		let widget = SelectKeyWidget::new(&prompt, "foo");
		assert_eq!(
			drawn(&widget),
			["│", "◆  foo", "│   a  Option A", "│   b  Option B", "└", ""]
		);
	}

	/// The chip is cyan on the option the cursor opened on and inverse-on-white on the rest.
	#[test]
	fn the_chip_says_which_option_the_cursor_is_on() {
		let state = SelectKeyState::new(options(&["a", "b"])).with_initial_value("b");
		let prompt = Prompt::new(state);
		let widget = SelectKeyWidget::new(&prompt, "foo");
		let styles = Theme::clack().styles;
		let frame = widget.frame();
		assert_eq!(frame.lines[2].spans[2].style, styles.key_inactive);
		assert_eq!(frame.lines[3].spans[2].style, styles.key_active);
	}

	#[test]
	fn a_hint_is_drawn_on_every_option_not_only_the_highlighted_one() {
		let list = vec![
			SelectOption::labelled("a".to_string(), "Option A").with_hint("first"),
			SelectOption::labelled("b".to_string(), "Option B").with_hint("second"),
		];
		let prompt = Prompt::new(SelectKeyState::new(list));
		let widget = SelectKeyWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│   a  Option A (first)");
		assert_eq!(drawn(&widget)[3], "│   b  Option B (second)");
	}

	#[test]
	fn a_submitted_frame_is_the_matched_label_alone() {
		let mut prompt = select_key(&["a", "b"]);
		typed(&mut prompt, 'b');
		let widget = SelectKeyWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◇  foo", "│  Option B"]);
	}

	/// Cancelling draws the first option whatever the cursor was on — upstream's `this.options[0]`.
	#[test]
	fn a_cancelled_frame_is_always_the_first_option() {
		let state = SelectKeyState::new(options(&["a", "b"])).with_initial_value("b");
		let mut prompt = Prompt::new(state);
		prompt.key(Some(""), &Key::named(KeyName::Escape));
		let widget = SelectKeyWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "■  foo", "│  Option A", "│"]);
	}

	#[test]
	fn without_a_guide_the_list_sits_in_the_margin() {
		let prompt = select_key(&["a", "b"]);
		let widget = SelectKeyWidget::new(&prompt, "foo").with_guide(false);
		assert_eq!(
			drawn(&widget),
			["◆  foo", " a  Option A", " b  Option B", "", ""]
		);
	}

	/// The message is not wrapped and not prefixed — the one list Prompt where a second row of it
	/// starts in the margin.
	#[test]
	fn a_message_with_a_break_in_it_takes_no_prefix_on_its_second_row() {
		let prompt = select_key(&["a"]);
		let widget = SelectKeyWidget::new(&prompt, "one\ntwo");
		assert_eq!(drawn(&widget)[1], "◆  one");
		assert_eq!(drawn(&widget)[2], "two");
	}

	/// Thirteen columns for a three-column prefix, once — the option wraps and the chip stays on
	/// the first row. The trailing space is upstream's `trim: false`.
	#[test]
	fn an_option_is_wrapped_ten_columns_early() {
		let list = vec![SelectOption::labelled(
			"a".to_string(),
			"one two three four five six seven",
		)];
		let prompt = Prompt::new(SelectKeyState::new(list));
		let widget = SelectKeyWidget::new(&prompt, "foo").with_columns(40);
		let drawn = drawn(&widget);
		assert_eq!(drawn[2], "│   a  one two three four five");
		assert_eq!(drawn[3], "│   six seven");
	}

	/// The bars between the rows of a cancelled value are struck through, because the escape that
	/// opened it is never closed until the end. Same leak as `select`'s.
	#[test]
	fn a_cancelled_value_strikes_the_bars_between_its_rows() {
		use ratatui_core::style::Modifier;

		let list = vec![SelectOption::labelled(
			"a".to_string(),
			"one two three four five six seven eight nine",
		)];
		let mut prompt = Prompt::new(SelectKeyState::new(list));
		prompt.key(Some(""), &Key::named(KeyName::Escape));
		let widget = SelectKeyWidget::new(&prompt, "foo").with_columns(40);
		let frame = widget.frame();
		let bar = frame.lines[3].spans[0].style;
		assert!(bar.add_modifier.contains(Modifier::CROSSED_OUT));
		assert!(!bar.add_modifier.contains(Modifier::DIM));
	}
}
