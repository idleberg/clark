//! Ported from `@clack/core`'s `prompts/multi-line.ts` and `@clack/prompts`' `multi-line.ts`.
//!
//! The last Prompt in the port, and the only one that keeps a whole editor of its own. `text` hands
//! its keys to readline and reads the line back; `multiline` is `super(opts, false)` — untracked —
//! and does every insertion, deletion and cursor move itself, over a string that has newlines in it.
//! So the state here owns the text and the cursor outright, and
//! [`Prompt::user_input`](crate::prompt::Prompt::user_input) is not what gets drawn.
//!
//! # `return` is not submission
//!
//! It is the reason [`PromptState::should_submit`] exists. A `return` inserts a newline and answers
//! *no* — twice in a row at the end of the text answers yes, and takes the first newline back out
//! again on the way. With `showSubmit` on, the rule changes entirely: `return` always inserts, tab
//! moves the focus to a `[ submit ]` button, and `return` on the button is what settles the Prompt.
//!
//! Because `_shouldSubmit` runs *after* the `key` listener and *before* validation, the newline it
//! inserts is in the text the validator sees — which is why the hook takes `&mut self` here.
//!
//! # Three things reproduced rather than corrected
//!
//! Per ADR-0013, wherever a terminal can see it. All three are in [`MultiLineWidget::frame`] and
//! ADR-0027 sets them out:
//!
//! - the error foot is drawn whether or not there is a Guide, alone among the Prompts;
//! - a settled Frame with no value still draws a bar and two trailing spaces;
//! - the text is wrapped thirteen columns early, the ADR-0019 defect again.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::widgets::Widget;

use crate::confirm::GUIDE_PREFIX_LENGTH;
use crate::cursor::find_text_cursor;
use crate::frame::{Frame, Line, Span};
use crate::line_editor::{Key, KeyName};
use crate::prompt::{Prompt, PromptState, Status};
use crate::text::{CURSOR_BLOCK, placeholder_spans};
use crate::theme::Theme;

/// Which of the two things a `showSubmit` Prompt has tab moves between.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
	#[default]
	Editor,
	Submit,
}

/// The state of a `multiline` Prompt: a block of text, and a cursor somewhere in it.
#[derive(Clone, Debug, Default)]
pub struct MultiLineState {
	/// `this.userInput`, which this Prompt maintains itself because it is untracked.
	text: String,
	/// `this._cursor`, as a count of characters — see [`find_text_cursor`] for the one place that
	/// differs from upstream's UTF-16 index.
	cursor: usize,
	default_value: Option<String>,
	show_submit: bool,
	focused: Focus,
	/// `#lastKeyWasReturn`. Cleared by every key that is not a `return`, including the arrows, which
	/// are the only keys that clear it *before* they do anything else.
	last_key_was_return: bool,
	/// `this.value`, which the `userInput` listener keeps equal to the text. `None` until the first
	/// edit, which is what a validator sees on a Prompt that was submitted untouched.
	value: Option<String>,
}

impl MultiLineState {
	pub fn new() -> Self {
		Self::default()
	}

	/// `defaultValue`: what an empty answer means. Not typed into the field — see the Scenario
	/// `multiline › defaultValue sets the value but does not render`.
	pub fn with_default_value(mut self, value: impl Into<String>) -> Self {
		self.default_value = Some(value.into());
		self
	}

	/// `showSubmit`: draw a `[ submit ]` button, and let `return` be an ordinary newline.
	pub fn with_show_submit(mut self, show_submit: bool) -> Self {
		self.show_submit = show_submit;
		self
	}

	/// The text as it stands, newlines and all. This is what the widget draws, not
	/// [`Prompt::user_input`](crate::prompt::Prompt::user_input).
	pub fn text(&self) -> &str {
		&self.text
	}

	/// The cursor as a count of characters into [`text`](Self::text).
	pub fn cursor(&self) -> usize {
		self.cursor
	}

	pub fn focused(&self) -> Focus {
		self.focused
	}

	pub fn show_submit(&self) -> bool {
		self.show_submit
	}

	/// `_setUserInput`, which for this Prompt is only ever its own doing. The `userInput` listener
	/// hangs off it and sets the value, so the two never come apart.
	fn set_text(&mut self, text: String) {
		self.text = text;
		self.value = Some(self.text.clone());
	}

	/// The byte offset of character `at`, or the end of the text.
	fn offset(&self, at: usize) -> usize {
		self.text
			.char_indices()
			.nth(at)
			.map(|(offset, _)| offset)
			.unwrap_or(self.text.len())
	}

	fn length(&self) -> usize {
		self.text.chars().count()
	}

	/// `#insertAtCursor`. Upstream's empty-text special case is the same thing this does anyway.
	fn insert_at_cursor(&mut self, text: &str) {
		let at = self.offset(self.cursor);
		let mut next = self.text.clone();
		next.insert_str(at, text);
		self.set_text(next);
	}

	/// The character at `from`, taken out. Nothing, where `from` is past the end — which is where
	/// `key`'s `delete` guard would land if it were not there, so a mutation that removes the guard
	/// survives. That is equivalent rather than untested: the only trace it leaves is a `value` of
	/// `Some("")` where there was `None`, and upstream's `!this.value` treats the two alike wherever
	/// either is read.
	fn remove(&mut self, from: usize) {
		let (start, end) = (self.offset(from), self.offset(from + 1));
		let mut next = self.text.clone();
		next.replace_range(start..end, "");
		self.set_text(next);
	}
}

impl PromptState for MultiLineState {
	type Value = String;

	/// `super(opts, false)`. Nothing is read back off readline, so the vim aliases reach this Prompt
	/// — and pass straight through it, because it subscribes to `key` and never to `cursor`. An `h`
	/// typed here is an `h`, by a different route from the one that makes it an `h` in a `text`.
	const TRACKS_INPUT: bool = false;

	/// `initialUserInput`, which upstream's constructor resolves out of `initialValue` and then puts
	/// the cursor at the end of. The two happen in different places up there — the field is written
	/// by `prompt()` and the cursor is set by the constructor — and land in the same place.
	fn user_input(&mut self, input: &str) {
		self.set_text(input.to_string());
		self.cursor = self.length();
	}

	fn key(&mut self, s: Option<&str>, key: &Key) {
		if let Some(name) = key.name {
			let direction = match name {
				KeyName::Up => Some((0, -1)),
				KeyName::Down => Some((0, 1)),
				KeyName::Left => Some((-1, 0)),
				KeyName::Right => Some((1, 0)),
				_ => None,
			};
			if let Some((dx, dy)) = direction {
				self.last_key_was_return = false;
				self.cursor = find_text_cursor(self.cursor, dx, dy, &self.text);
				return;
			}
		}

		// Tab is the focus toggle, and only where there is something to focus. Without `showSubmit`
		// it falls through to the insertion below and types a literal tab — which upstream's
		// `_isActionKey` would have taken back out of readline, had this Prompt been reading it.
		if s == Some("\t") && self.show_submit {
			self.focused = match self.focused {
				Focus::Editor => Focus::Submit,
				Focus::Submit => Focus::Editor,
			};
			return;
		}

		// `return` is left entirely to `should_submit`, and is the one key that does not clear
		// `#lastKeyWasReturn` — which is the whole mechanism by which two of them in a row settle.
		if key.name == Some(KeyName::Return) {
			return;
		}
		self.last_key_was_return = false;

		if key.name == Some(KeyName::Backspace) && self.cursor > 0 {
			self.remove(self.cursor - 1);
			self.cursor -= 1;
			return;
		}
		if key.name == Some(KeyName::Delete) && self.cursor < self.length() {
			self.remove(self.cursor);
			return;
		}

		// `if (char)`: readline reports the empty string for a bare `return` and for the arrows, and
		// an empty string is falsy, so neither is typed.
		if let Some(char) = s.filter(|c| !c.is_empty()) {
			if self.show_submit && self.focused == Focus::Submit {
				self.focused = Focus::Editor;
			}
			self.insert_at_cursor(char);
			// `this._cursor++`, whatever was inserted. A `char` of more than one character — a paste
			// arriving as one keypress — moves the cursor one place and leaves the rest behind it.
			self.cursor += 1;
		}
	}

	/// `_shouldSubmit`, which is where a `return` is spent.
	fn should_submit(&mut self, _s: Option<&str>, _key: &Key) -> bool {
		if self.show_submit {
			if self.focused == Focus::Submit {
				return true;
			}
			self.insert_at_cursor("\n");
			self.cursor += 1;
			return false;
		}

		let was_return = self.last_key_was_return;
		self.last_key_was_return = true;

		if was_return && self.cursor == self.length() {
			// The newline the previous `return` inserted is taken back out. Upstream guards this on
			// the character actually being one, and the guard cannot fail: the flag is only ever set
			// by a `return`, a `return` only ever sets it after inserting a `\n` at the cursor and
			// stepping past it, and every other key clears the flag. Kept because it is written down
			// there; a mutation that widens it to "remove whatever is there" survives, and this is why.
			if self.cursor > 0 && self.text.chars().nth(self.cursor - 1) == Some('\n') {
				self.remove(self.cursor - 1);
				self.cursor -= 1;
			}
			return true;
		}

		self.insert_at_cursor("\n");
		self.cursor += 1;
		false
	}

	/// `!this.value` is true of the empty string as well as of no value at all, so the default
	/// applies to text that was erased as much as to text that was never begun.
	fn finalize(&mut self) {
		if self.value.as_deref().unwrap_or("").is_empty() {
			self.value = self.default_value.clone();
		}
		if self.value.is_none() {
			self.value = Some(String::new());
		}
	}

	fn value(&self) -> Option<&String> {
		self.value.as_ref()
	}
}

/// A `multiline` Prompt drawn as a Frame — the `render` callback of `@clack/prompts`' `multiline()`.
pub struct MultiLineWidget<'a> {
	prompt: &'a Prompt<MultiLineState>,
	message: &'a str,
	placeholder: Option<&'a str>,
	theme: &'a Theme,
	with_guide: Option<bool>,
	/// `getColumns(opts.output)`, which is what `wrapTextWithPrefix` measures against.
	columns: usize,
}

impl<'a> MultiLineWidget<'a> {
	pub fn new(prompt: &'a Prompt<MultiLineState>, message: &'a str) -> Self {
		Self {
			prompt,
			message,
			placeholder: None,
			theme: &THEME,
			with_guide: None,
			columns: 80,
		}
	}

	pub fn with_placeholder(mut self, placeholder: &'a str) -> Self {
		self.placeholder = Some(placeholder);
		self
	}

	pub fn with_theme(mut self, theme: &'a Theme) -> Self {
		self.theme = theme;
		self
	}

	pub fn with_guide(mut self, with_guide: bool) -> Self {
		self.with_guide = Some(with_guide);
		self
	}

	pub fn with_columns(mut self, columns: usize) -> Self {
		self.columns = columns;
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

		if guide {
			frame.push(Span::styled(symbols.bar, styles.guide));
		}
		frame.push(Line::from_iter([
			self.theme.step(status),
			Span::raw("  "),
			Span::styled(self.message, styles.message),
		]));

		let value = state.value().map(String::as_str).unwrap_or("");

		match status {
			Status::Error => {
				let bar = Span::styled(symbols.bar, styles.guide_error);
				frame.lines.extend(self.wrapped(self.field(), guide, bar));

				// The one Prompt whose foot is drawn whether or not there is a Guide: upstream reads
				// `hasGuide` for the lines above and not for this. ADR-0027.
				let mut end = Line::from(Span::styled(symbols.bar_end, styles.guide_error));
				end.push(Span::raw("  "));
				end.push(Span::styled(self.prompt.error(), styles.error));
				frame.push(end);

				self.push_submit_button(&mut frame);
				frame.push(Line::blank());
			}

			Status::Submit | Status::Cancel => {
				let style = if status == Status::Submit {
					styles.submitted
				} else {
					styles.cancelled
				};
				let bar = Span::styled(symbols.bar, styles.guide);
				if guide {
					// `wrapTextWithPrefix` over an empty string is one line, so a Prompt that settled
					// on nothing still draws a bar — and the two spaces after it, which nothing else
					// in the port leaves trailing. ADR-0027.
					let settled = Line::from(Span::styled(value, style));
					frame.lines.extend(self.wrapped(settled, true, bar));
				} else if !value.is_empty() {
					frame
						.lines
						.extend(Line::from(Span::styled(value, style)).paragraphs());
				} else {
					frame.push(Line::blank());
				}
			}

			Status::Initial | Status::Active => {
				let bar = Span::styled(symbols.bar, styles.guide_active);
				frame.lines.extend(self.wrapped(self.field(), guide, bar));

				let mut end = Line::blank();
				if guide {
					end.push(Span::styled(symbols.bar_end, styles.guide_active));
				}
				frame.push(end);

				self.push_submit_button(&mut frame);
				frame.push(Line::blank());
			}
		}

		frame
	}

	/// `submitButton`, which is a line of its own because it is written as `\n  [ submit ]`.
	fn push_submit_button(&self, frame: &mut Frame) {
		let state = self.prompt.state();
		if !state.show_submit() {
			return;
		}
		let style = if state.focused() == Focus::Submit {
			self.theme.styles.submit_focused
		} else {
			self.theme.styles.submit_unfocused
		};
		frame.push(Line::from_iter([
			Span::raw("  "),
			Span::styled("[ submit ]", style),
		]));
	}

	/// `wrapTextWithPrefix`: the text broken to the terminal less the prefix, then prefixed.
	///
	/// The width taken off is [`GUIDE_PREFIX_LENGTH`], because upstream takes `prefix.length` of a
	/// string whose bar is wrapped in two escape sequences — the same thirteen columns for three,
	/// and the same defect ADR-0019 records for `confirm`.
	///
	/// Unguided there is no prefix and no wrap at all: the text is handed on whole, and the only
	/// break it gets is the outer one every Frame gets.
	fn wrapped(&self, line: Line, guide: bool, bar: Span) -> Vec<Line> {
		if !guide {
			return line.paragraphs();
		}
		let columns = self.columns.saturating_sub(GUIDE_PREFIX_LENGTH);
		line.paragraphs()
			.iter()
			.flat_map(|paragraph| paragraph.wrap(columns))
			.map(|row| {
				let mut out = Line::from(bar.clone());
				out.push(Span::raw("  "));
				out.spans.extend(row.spans);
				out
			})
			.collect()
	}

	/// `userInput`: the text with its cursor in it, or the placeholder standing in for it.
	///
	/// One [`Line`] whose spans still carry the newlines — [`wrapped`](Self::wrapped) is what cuts
	/// them into rows, because upstream wraps the whole string in one call too.
	fn field(&self) -> Line {
		let styles = &self.theme.styles;
		let state = self.prompt.state();
		let text = state.text();

		if text.is_empty() {
			return Line::from_iter(placeholder_spans(self.placeholder, styles));
		}

		let cursor = state.cursor();
		let mut chars = text.char_indices().map(|(offset, _)| offset);
		let Some(at) = chars.nth(cursor) else {
			// `this.cursor >= userInput.length`: the block goes after the text rather than over a
			// character, and it is unstyled — an inverse block would be invisible.
			return Line::from_iter([Span::raw(text), Span::raw(CURSOR_BLOCK)]);
		};
		let end = chars.next().unwrap_or(text.len());
		let (before, over, after) = (&text[..at], &text[at..end], &text[end..]);

		// A cursor resting on a newline cannot be inverted — there is nothing on the cell to invert
		// — so upstream draws a block in front of it and keeps the break.
		if over == "\n" {
			return Line::from_iter([
				Span::raw(before),
				Span::raw(CURSOR_BLOCK),
				Span::raw("\n"),
				Span::raw(after),
			]);
		}
		Line::from_iter([
			Span::raw(before),
			Span::styled(over, styles.cursor),
			Span::raw(after),
		])
	}
}

/// The default Theme, as somewhere to borrow from.
static THEME: Theme = Theme::clack();

impl Widget for &MultiLineWidget<'_> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		(&self.frame()).render(area, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::prompt::Outcome;

	fn multiline() -> Prompt<MultiLineState> {
		Prompt::new(MultiLineState::new())
	}

	fn typed(prompt: &mut Prompt<MultiLineState>, s: &str) {
		for c in s.chars() {
			prompt.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
		}
	}

	/// A bare `return`, as readline reports it: an empty character and a name.
	fn enter(prompt: &mut Prompt<MultiLineState>) {
		prompt.key(Some(""), &Key::named(KeyName::Return));
	}

	fn press(prompt: &mut Prompt<MultiLineState>, name: KeyName) {
		prompt.key(Some(""), &Key::named(name));
	}

	fn answer(prompt: &Prompt<MultiLineState>) -> Option<&str> {
		match prompt.outcome() {
			Some(Outcome::Submitted(v)) => v.map(String::as_str),
			_ => None,
		}
	}

	fn text(prompt: &Prompt<MultiLineState>) -> &str {
		prompt.state().text()
	}

	// --- the editor -----------------------------------------------------------------------------

	#[test]
	fn one_return_is_a_newline_and_two_are_a_submission() {
		let mut prompt = multiline();
		typed(&mut prompt, "xy");
		enter(&mut prompt);
		assert_eq!(text(&prompt), "xy\n");
		assert!(!prompt.status().is_finished());

		enter(&mut prompt);
		assert_eq!(prompt.status(), Status::Submit);
		// The newline the first one inserted is taken back out on the way.
		assert_eq!(answer(&prompt), Some("xy"));
	}

	#[test]
	fn two_returns_on_an_empty_prompt_answer_with_nothing() {
		let mut prompt = multiline();
		enter(&mut prompt);
		enter(&mut prompt);
		assert_eq!(answer(&prompt), Some(""));
	}

	/// The pair has to be consecutive: anything in between clears the flag and the next `return`
	/// starts the count again.
	#[test]
	fn a_key_between_two_returns_breaks_the_pair() {
		let mut prompt = multiline();
		enter(&mut prompt);
		typed(&mut prompt, "x");
		enter(&mut prompt);
		assert!(!prompt.status().is_finished());
		assert_eq!(text(&prompt), "\nx\n");
		enter(&mut prompt);
		assert_eq!(answer(&prompt), Some("\nx"));
	}

	/// The second `return` only settles at the end of the text. From the middle it inserts, and the
	/// flag it sets means the one after it can settle.
	#[test]
	fn a_second_return_away_from_the_end_inserts_instead() {
		let mut prompt = multiline();
		typed(&mut prompt, "ab");
		press(&mut prompt, KeyName::Left);
		enter(&mut prompt);
		assert_eq!(text(&prompt), "a\nb");
		enter(&mut prompt);
		assert!(
			!prompt.status().is_finished(),
			"the cursor is not at the end"
		);
		assert_eq!(text(&prompt), "a\n\nb");
	}

	/// The take-back looks at the character before the cursor, not at whether a newline was ever
	/// inserted — so a `return` on the empty text of a Prompt whose last character is not one
	/// settles without removing anything.
	#[test]
	fn the_take_back_only_removes_a_newline() {
		let mut prompt = multiline();
		enter(&mut prompt);
		assert_eq!(text(&prompt), "\n");
		press(&mut prompt, KeyName::Left);
		press(&mut prompt, KeyName::Right);
		enter(&mut prompt);
		// The arrows cleared the flag, so this one inserts.
		assert_eq!(text(&prompt), "\n\n");
		enter(&mut prompt);
		assert_eq!(answer(&prompt), Some("\n"));
	}

	#[test]
	fn backspace_removes_the_character_before_the_cursor() {
		let mut prompt = multiline();
		typed(&mut prompt, "abc");
		press(&mut prompt, KeyName::Left);
		press(&mut prompt, KeyName::Backspace);
		assert_eq!(text(&prompt), "ac");
		assert_eq!(prompt.state().cursor(), 1);
	}

	#[test]
	fn backspace_at_the_start_does_nothing() {
		let mut prompt = multiline();
		typed(&mut prompt, "a");
		press(&mut prompt, KeyName::Left);
		press(&mut prompt, KeyName::Backspace);
		assert_eq!(text(&prompt), "a");
		assert_eq!(prompt.state().cursor(), 0);
	}

	/// Delete takes the character *under* the cursor and leaves the cursor where it is.
	#[test]
	fn delete_removes_the_character_at_the_cursor() {
		let mut prompt = multiline();
		typed(&mut prompt, "abc");
		press(&mut prompt, KeyName::Left);
		press(&mut prompt, KeyName::Delete);
		assert_eq!(text(&prompt), "ab");
		assert_eq!(prompt.state().cursor(), 2);

		// And at the end there is nothing under it.
		press(&mut prompt, KeyName::Delete);
		assert_eq!(text(&prompt), "ab");
	}

	/// The text is indexed by character and sliced by byte, and the two are only the same number in
	/// ASCII. A `remove` that took one byte would leave half a character behind — or panic.
	#[test]
	fn backspace_takes_a_whole_character_however_many_bytes_it_is() {
		let mut prompt = multiline();
		typed(&mut prompt, "aéb");
		press(&mut prompt, KeyName::Left);
		press(&mut prompt, KeyName::Backspace);
		assert_eq!(text(&prompt), "ab");
		assert_eq!(prompt.state().cursor(), 1);

		// And forwards, which slices at the other end of the same character.
		let mut prompt = multiline();
		typed(&mut prompt, "aéb");
		press(&mut prompt, KeyName::Left);
		press(&mut prompt, KeyName::Left);
		press(&mut prompt, KeyName::Delete);
		assert_eq!(text(&prompt), "ab");
	}

	/// `this._cursor++`, whatever was inserted. readline reports a paste as one keypress carrying the
	/// whole of it, and the cursor still moves one place — so the rest of it ends up to the right of
	/// the caret, which is not where it was typed.
	#[test]
	fn a_multi_character_keypress_still_moves_the_cursor_one_place() {
		let mut prompt = multiline();
		prompt.key(Some("abc"), &Key::default());
		assert_eq!(text(&prompt), "abc");
		assert_eq!(prompt.state().cursor(), 1);
	}

	#[test]
	fn a_newline_can_be_backspaced_like_any_other_character() {
		let mut prompt = multiline();
		typed(&mut prompt, "a");
		enter(&mut prompt);
		typed(&mut prompt, "b");
		assert_eq!(text(&prompt), "a\nb");
		press(&mut prompt, KeyName::Left);
		press(&mut prompt, KeyName::Backspace);
		assert_eq!(text(&prompt), "ab");
	}

	#[test]
	fn the_arrows_walk_the_text_by_rows_and_columns() {
		let mut prompt = multiline();
		typed(&mut prompt, "ab");
		enter(&mut prompt);
		typed(&mut prompt, "cde");
		assert_eq!(text(&prompt), "ab\ncde");
		assert_eq!(prompt.state().cursor(), 6);

		press(&mut prompt, KeyName::Up);
		assert_eq!(
			prompt.state().cursor(),
			2,
			"column 3 clamps to the end of a two-character row"
		);
		press(&mut prompt, KeyName::Right);
		assert_eq!(
			prompt.state().cursor(),
			3,
			"over the newline onto the next row"
		);
	}

	/// Untracked, so the vim aliases are emitted — and go nowhere, because this Prompt subscribes to
	/// `key` and not to `cursor`. An `h` is a character like any other.
	#[test]
	fn the_vim_aliases_are_typed_rather_than_obeyed() {
		let mut prompt = multiline();
		typed(&mut prompt, "hjkl");
		assert_eq!(text(&prompt), "hjkl");
		assert_eq!(prompt.state().cursor(), 4);
	}

	/// Without `showSubmit` there is nothing for tab to focus, so it types one.
	#[test]
	fn a_tab_is_a_tab_where_there_is_no_button_to_focus() {
		let mut prompt = multiline();
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		assert_eq!(text(&prompt), "\t");
	}

	// --- showSubmit -----------------------------------------------------------------------------

	fn with_button() -> Prompt<MultiLineState> {
		Prompt::new(MultiLineState::new().with_show_submit(true))
	}

	#[test]
	fn a_return_never_submits_while_the_editor_is_focused() {
		let mut prompt = with_button();
		typed(&mut prompt, "xy");
		enter(&mut prompt);
		enter(&mut prompt);
		assert!(!prompt.status().is_finished());
		assert_eq!(text(&prompt), "xy\n\n");
	}

	#[test]
	fn tab_focuses_the_button_and_a_return_on_it_submits() {
		let mut prompt = with_button();
		typed(&mut prompt, "xy");
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		assert_eq!(prompt.state().focused(), Focus::Submit);
		enter(&mut prompt);
		assert_eq!(answer(&prompt), Some("xy"));
	}

	/// The focus toggle is `char === '\t'` and not `key.name === 'tab'` — the only place in clack
	/// where the two are different keys. A terminal that names the key without sending the character
	/// does not move the focus, and the `date` recordings send exactly that shape of tab.
	#[test]
	fn a_tab_with_no_character_does_not_move_the_focus() {
		let mut prompt = with_button();
		prompt.key(Some(""), &Key::named(KeyName::Tab));
		assert_eq!(prompt.state().focused(), Focus::Editor);
	}

	#[test]
	fn tab_toggles_back_to_the_editor() {
		let mut prompt = with_button();
		for _ in 0..2 {
			prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		}
		assert_eq!(prompt.state().focused(), Focus::Editor);
		assert_eq!(text(&prompt), "", "no tab was typed either time");
	}

	/// Typing while the button is focused takes the focus back — the character lands in the editor
	/// and the button stops being the thing `return` would press.
	#[test]
	fn typing_takes_the_focus_off_the_button() {
		let mut prompt = with_button();
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		typed(&mut prompt, "z");
		assert_eq!(prompt.state().focused(), Focus::Editor);
		assert_eq!(text(&prompt), "z");
		enter(&mut prompt);
		assert!(!prompt.status().is_finished());
	}

	/// The arrows do not: they move the cursor and leave the focus alone, so `return` still submits.
	#[test]
	fn an_arrow_leaves_the_focus_on_the_button() {
		let mut prompt = with_button();
		typed(&mut prompt, "ab");
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		press(&mut prompt, KeyName::Left);
		assert_eq!(prompt.state().focused(), Focus::Submit);
		assert_eq!(prompt.state().cursor(), 1);
		enter(&mut prompt);
		assert_eq!(answer(&prompt), Some("ab"));
	}

	// --- the value ------------------------------------------------------------------------------

	#[test]
	fn the_default_value_fills_in_for_an_empty_answer() {
		let mut prompt = Prompt::new(MultiLineState::new().with_default_value("bar"));
		enter(&mut prompt);
		enter(&mut prompt);
		assert_eq!(answer(&prompt), Some("bar"));
	}

	#[test]
	fn the_default_value_does_not_override_an_answer() {
		let mut prompt = Prompt::new(MultiLineState::new().with_default_value("bar"));
		typed(&mut prompt, "xy");
		enter(&mut prompt);
		enter(&mut prompt);
		assert_eq!(answer(&prompt), Some("xy"));
	}

	#[test]
	fn an_initial_value_is_editable_text_with_the_cursor_at_its_end() {
		let mut prompt = Prompt::new(MultiLineState::new()).with_initial_user_input("a\nb");
		assert_eq!(prompt.state().cursor(), 3);
		typed(&mut prompt, "!");
		enter(&mut prompt);
		enter(&mut prompt);
		assert_eq!(answer(&prompt), Some("a\nb!"));
	}

	#[test]
	fn escape_cancels() {
		let mut prompt = multiline();
		typed(&mut prompt, "xy");
		prompt.key(None, &Key::named(KeyName::Escape));
		assert_eq!(prompt.outcome(), Some(Outcome::Cancelled));
	}

	/// A validator sees the text as `_shouldSubmit` left it — with the newline already taken back
	/// out, because the take-back happens before the check.
	#[test]
	fn validation_sees_the_text_the_submission_settled_on() {
		let mut prompt =
			Prompt::new(MultiLineState::new()).with_validator(|value: Option<&String>| match value
				.map(String::as_str)
			{
				Some("xy") => None,
				_ => Some("should be xy".to_string()),
			});
		typed(&mut prompt, "x");
		enter(&mut prompt);
		enter(&mut prompt);
		assert_eq!(prompt.status(), Status::Error);
		assert_eq!(prompt.error(), "should be xy");

		typed(&mut prompt, "y");
		enter(&mut prompt);
		enter(&mut prompt);
		assert_eq!(answer(&prompt), Some("xy"));
	}

	// --- the widget -----------------------------------------------------------------------------

	fn drawn(widget: &MultiLineWidget<'_>) -> Vec<String> {
		widget
			.frame()
			.lines
			.iter()
			.map(|line| line.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn an_opening_frame_has_a_guide_a_block_and_a_foot() {
		let prompt = multiline();
		let widget = MultiLineWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◆  foo", "│  _", "└", ""]);
	}

	#[test]
	fn every_row_of_the_text_gets_a_bar_of_its_own() {
		let mut prompt = multiline();
		typed(&mut prompt, "ab");
		enter(&mut prompt);
		typed(&mut prompt, "cd");
		let widget = MultiLineWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◆  foo", "│  ab", "│  cd█", "└", ""]);
	}

	/// The cursor cannot be drawn over a newline, so it is drawn in front of one.
	#[test]
	fn a_cursor_on_a_newline_becomes_a_block_before_it() {
		let mut prompt = multiline();
		typed(&mut prompt, "ab");
		enter(&mut prompt);
		typed(&mut prompt, "cd");
		press(&mut prompt, KeyName::Up);
		assert_eq!(prompt.state().cursor(), 2);

		let widget = MultiLineWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  ab█");
		assert_eq!(drawn(&widget)[3], "│  cd");
	}

	#[test]
	fn the_placeholder_stands_in_until_something_is_typed() {
		let prompt = multiline();
		let widget = MultiLineWidget::new(&prompt, "foo").with_placeholder("bar");
		assert_eq!(drawn(&widget)[2], "│  bar");
	}

	#[test]
	fn without_a_guide_there_is_no_bar_and_no_foot() {
		let prompt = multiline();
		let widget = MultiLineWidget::new(&prompt, "foo").with_guide(false);
		assert_eq!(drawn(&widget), ["◆  foo", "_", "", ""]);
	}

	#[test]
	fn the_button_sits_under_the_foot() {
		let prompt = with_button();
		let widget = MultiLineWidget::new(&prompt, "foo");
		assert_eq!(
			drawn(&widget),
			["│", "◆  foo", "│  _", "└", "  [ submit ]", ""]
		);
	}

	#[test]
	fn a_focused_button_is_cyan_and_an_unfocused_one_is_dim() {
		let mut prompt = with_button();
		let unfocused = MultiLineWidget::new(&prompt, "foo").frame().lines[4].spans[1].style;
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		let focused = MultiLineWidget::new(&prompt, "foo").frame().lines[4].spans[1].style;
		assert_ne!(unfocused, focused);
		assert_eq!(focused, Theme::clack().styles.submit_focused);
	}

	#[test]
	fn a_submitted_frame_shows_the_value_and_no_foot() {
		let mut prompt = multiline();
		typed(&mut prompt, "xy");
		enter(&mut prompt);
		enter(&mut prompt);
		let widget = MultiLineWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◇  foo", "│  xy"]);
	}

	/// A settled Prompt with nothing to show still draws a bar — and two spaces after it, which
	/// `wrapTextWithPrefix` leaves there because the empty string is still one line. ADR-0027.
	#[test]
	fn a_settled_frame_with_no_value_still_draws_a_bar_and_two_spaces() {
		let mut prompt = multiline();
		enter(&mut prompt);
		enter(&mut prompt);
		let widget = MultiLineWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◇  foo", "│  "]);
	}

	#[test]
	fn a_cancelled_frame_strikes_the_value_through() {
		let mut prompt = multiline();
		typed(&mut prompt, "xy");
		prompt.key(None, &Key::named(KeyName::Escape));
		let widget = MultiLineWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "■  foo", "│  xy"]);
		assert_eq!(
			widget.frame().lines[2].spans[2].style,
			Theme::clack().styles.cancelled
		);
	}

	/// Alone among the Prompts, the error foot is drawn with no Guide asked for. ADR-0027.
	#[test]
	fn the_error_foot_is_drawn_whether_or_not_there_is_a_guide() {
		let mut prompt = Prompt::new(MultiLineState::new())
			.with_validator(|_: Option<&String>| Some("nope".to_string()));
		enter(&mut prompt);
		enter(&mut prompt);
		assert_eq!(prompt.status(), Status::Error);

		let widget = MultiLineWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "▲  foo", "│  _", "└  nope", ""]);

		let widget = MultiLineWidget::new(&prompt, "foo").with_guide(false);
		assert_eq!(drawn(&widget), ["▲  foo", "_", "└  nope", ""]);
	}

	/// Thirteen columns for a prefix that draws three — ADR-0019's defect, in a third place.
	#[test]
	fn the_text_is_wrapped_ten_columns_early() {
		let mut prompt = multiline();
		typed(&mut prompt, "aaaa bbbb cccc");
		let widget = MultiLineWidget::new(&prompt, "foo").with_columns(GUIDE_PREFIX_LENGTH + 10);
		assert_eq!(drawn(&widget)[2], "│  aaaa bbbb ");
		assert_eq!(drawn(&widget)[3], "│  cccc█");
		// Three visible columns of prefix and ten of text, on a terminal twenty-three wide.
		assert_eq!(widget.frame().lines[2].width(), 13);
	}
}
