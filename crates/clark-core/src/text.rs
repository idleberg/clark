//! Ported from `@clack/core`'s `prompts/text.ts`.
//!
//! `TextPrompt` is 44 lines and three of them matter: the value is the user input, verbatim, on
//! every keypress; on finalize an empty value falls back to the default; and a value that is still
//! missing becomes the empty string. Everything else it does — the cursor split, the `_cursor`
//! getter — belongs to the shared machinery and lives on [`Prompt`](crate::prompt::Prompt).
//!
//! `initialValue` is not a field here. Upstream `TextPrompt` forwards it to `initialUserInput`,
//! which is the base class's, so it is
//! [`Prompt::with_initial_user_input`](crate::prompt::Prompt::with_initial_user_input). The `text()`
//! builder in `clark` will accept both names and resolve them the way upstream does.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::widgets::Widget;

use crate::frame::{Frame, Line, Span};
use crate::prompt::{InputWithCursor, Prompt, PromptState, Status};
use crate::theme::{Styles, Theme};

/// The state of a `text` Prompt: a string, and what to fall back to when it is empty.
#[derive(Clone, Debug, Default)]
pub struct TextState {
	value: Option<String>,
	default_value: Option<String>,
}

impl TextState {
	pub fn new() -> Self {
		Self::default()
	}

	/// `defaultValue`: what an empty answer means.
	///
	/// Note this is not `initialValue` — nothing is typed for the user, and the fallback applies at
	/// the moment the Prompt settles rather than at the moment it opens.
	pub fn with_default_value(mut self, value: impl Into<String>) -> Self {
		self.default_value = Some(value.into());
		self
	}

	/// The answer. `None` until the Prompt settles, after which it is always `Some`.
	pub fn value(&self) -> Option<&str> {
		self.value.as_deref()
	}
}

impl PromptState for TextState {
	type Value = String;

	fn user_input(&mut self, input: &str) {
		self.value = Some(input.to_string());
	}

	/// Upstream tests `!this.value`, which for a `string | undefined` is true of both the empty
	/// string and no value at all — so the default applies to an answer the user erased, not only to
	/// one they never began.
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

/// The cursor block upstream appends when the cursor is past the last character.
pub const CURSOR_BLOCK: &str = "█";

/// The placeholder standing in for an empty field, as `text` and `multiline` both draw it.
///
/// The two write the same three lines of TypeScript, so they share one here. `None`, or a
/// placeholder with nothing in it, is upstream's `styleText(['inverse', 'hidden'], '_')` — a
/// character that reserves a cell and shows nothing in it.
pub fn placeholder_spans(placeholder: Option<&str>, styles: &Styles) -> Vec<Span> {
	match placeholder {
		Some(placeholder) if !placeholder.is_empty() => {
			// The first character is inverted to stand in for the cursor. Upstream slices it off
			// with `placeholder[0]`, a UTF-16 index, which halves an astral character; taken whole
			// here, as `input_with_cursor` takes its own.
			let mut chars = placeholder.chars();
			let first = chars.next().map(String::from).unwrap_or_default();
			let rest = chars.as_str();
			let mut spans = vec![Span::styled(first, styles.placeholder_cursor)];
			if !rest.is_empty() {
				spans.push(Span::styled(rest, styles.placeholder));
			}
			spans
		}
		_ => vec![Span::styled("_", styles.placeholder_empty)],
	}
}

/// A `text` Prompt drawn as a Frame — the `render` callback of `@clack/prompts`' `text()`.
///
/// The options it carries are the ones that never reach the state machine. Upstream's `text()`
/// closes over `message` and `placeholder` in the callback it builds and passes the core Prompt
/// only the rest, which is why they are here and not on [`TextState`]; the Recorder had to be built
/// around the same split (ADR-0010).
pub struct TextWidget<'a> {
	prompt: &'a Prompt<TextState>,
	message: &'a str,
	placeholder: Option<&'a str>,
	theme: &'a Theme,
	/// `opts.withGuide ?? settings.withGuide` — `None` defers to the Prompt's Settings.
	with_guide: Option<bool>,
}

impl<'a> TextWidget<'a> {
	pub fn new(prompt: &'a Prompt<TextState>, message: &'a str) -> Self {
		Self {
			prompt,
			message,
			placeholder: None,
			theme: &THEME,
			with_guide: None,
		}
	}

	/// `placeholder`: a hint shown while the field is empty. Never a value — see the Scenario
	/// `text › placeholder is not used as value when pressing enter`.
	pub fn with_placeholder(mut self, placeholder: &'a str) -> Self {
		self.placeholder = Some(placeholder);
		self
	}

	pub fn with_theme(mut self, theme: &'a Theme) -> Self {
		self.theme = theme;
		self
	}

	/// `withGuide`, as an option on the Prompt rather than on the flow.
	pub fn with_guide(mut self, with_guide: bool) -> Self {
		self.with_guide = Some(with_guide);
		self
	}

	fn guided(&self) -> bool {
		self.with_guide.unwrap_or(self.prompt.settings().with_guide)
	}

	/// The Frame, branch for branch as upstream's `render` writes it.
	///
	/// Upstream returns a string with `\n` in it, and the driver splits on those. So does this: one
	/// [`Line`] per `\n`-separated part, *including* the empty part a trailing newline leaves
	/// behind. That empty Line is a row — `restoreCursor` counts it when it walks back up the
	/// Frame — and dropping it would move every later Frame one row up the screen.
	pub fn frame(&self) -> Frame {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let guide = self.guided();
		let status = self.prompt.status();

		let mut frame = Frame::new();

		// `titlePrefix`, whose newline makes the Guide above the question a line of its own.
		if guide {
			frame.push(Span::styled(symbols.bar, styles.guide));
		}
		let title = Line::from_iter([
			self.theme.step(status),
			Span::raw("  "),
			Span::styled(self.message, styles.message),
		]);

		let value = self.prompt.state().value().unwrap_or("");

		match status {
			Status::Error => {
				// `title.trim()` — the trailing newline goes, and with it any trailing whitespace of
				// the question. Nothing is trimmed from the front: the step symbol's colour puts an
				// escape there, which is not whitespace to `String.prototype.trim`.
				frame.push(trim_end(title));

				let mut input = Line::blank();
				if guide {
					input.push(Span::styled(symbols.bar, styles.guide_error));
					input.push(Span::raw("  "));
				}
				input.spans.extend(self.user_input());
				frame.push(input);

				let mut end = Line::blank();
				if guide {
					end.push(Span::styled(symbols.bar_end, styles.guide_error));
				}
				if !self.prompt.error().is_empty() {
					end.push(Span::raw("  "));
					end.push(Span::styled(self.prompt.error(), styles.error));
				}
				frame.push(end);
				frame.push(Line::blank());
			}

			Status::Submit => {
				frame.push(title);
				let mut settled = Line::blank();
				if guide {
					settled.push(Span::styled(symbols.bar, styles.guide));
				}
				if !value.is_empty() {
					settled.push(Span::raw("  "));
					settled.push(Span::styled(value, styles.submitted));
				}
				frame.push(settled);
			}

			Status::Cancel => {
				frame.push(title);
				let mut settled = Line::blank();
				if guide {
					settled.push(Span::styled(symbols.bar, styles.guide));
				}
				if !value.is_empty() {
					settled.push(Span::raw("  "));
					settled.push(Span::styled(value, styles.cancelled));
				}
				frame.push(settled);

				// A value of nothing but whitespace is drawn — it is not empty — but does not earn
				// the closing Guide. Upstream tests `value` for the first and `value.trim()` for
				// the second, and the two disagree exactly there.
				if !value.trim().is_empty() {
					let mut closing = Line::blank();
					if guide {
						closing.push(Span::styled(symbols.bar, styles.guide));
					}
					frame.push(closing);
				}
			}

			Status::Initial | Status::Active => {
				frame.push(title);

				let mut input = Line::blank();
				if guide {
					input.push(Span::styled(symbols.bar, styles.guide_active));
					input.push(Span::raw("  "));
				}
				input.spans.extend(self.user_input());
				frame.push(input);

				let mut end = Line::blank();
				if guide {
					end.push(Span::styled(symbols.bar_end, styles.guide_active));
				}
				frame.push(end);
				frame.push(Line::blank());
			}
		}

		frame
	}

	/// `userInput`: what the user typed, or the placeholder standing in for it.
	fn user_input(&self) -> Vec<Span> {
		let styles = &self.theme.styles;

		if self.prompt.user_input().is_empty() {
			return placeholder_spans(self.placeholder, styles);
		}

		match self.prompt.input_with_cursor() {
			InputWithCursor::Plain(text) => vec![Span::raw(text)],
			InputWithCursor::AtEnd(text) => {
				vec![Span::raw(text), Span::raw(CURSOR_BLOCK)]
			}
			InputWithCursor::Over { before, at, after } => {
				let mut spans = Vec::with_capacity(3);
				if !before.is_empty() {
					spans.push(Span::raw(before));
				}
				spans.push(Span::styled(at, styles.cursor));
				if !after.is_empty() {
					spans.push(Span::raw(after));
				}
				spans
			}
		}
	}
}

/// The default Theme, as somewhere to borrow from.
static THEME: Theme = Theme::clack();

/// `String.prototype.trim`, as far as a Frame line can feel it: trailing whitespace, and any span
/// that is nothing but whitespace at the end of the line.
fn trim_end(mut line: Line) -> Line {
	while let Some(last) = line.spans.last_mut() {
		let trimmed = last.text.trim_end();
		if trimmed.is_empty() {
			line.spans.pop();
			continue;
		}
		if trimmed.len() != last.text.len() {
			last.text.truncate(trimmed.len());
		}
		break;
	}
	line
}

impl Widget for &TextWidget<'_> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		(&self.frame()).render(area, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::line_editor::{Key, KeyName};
	use crate::prompt::{Outcome, Prompt, Status};

	fn text() -> Prompt<TextState> {
		Prompt::new(TextState::new())
	}

	fn typed(prompt: &mut Prompt<TextState>, s: &str) {
		for c in s.chars() {
			prompt.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
		}
	}

	fn submit(prompt: &mut Prompt<TextState>) {
		prompt.key(None, &Key::named(KeyName::Return));
	}

	fn answer(prompt: &Prompt<TextState>) -> Option<&str> {
		match prompt.outcome() {
			Some(Outcome::Submitted(v)) => v.map(String::as_str),
			_ => None,
		}
	}

	#[test]
	fn the_answer_is_what_was_typed() {
		let mut prompt = text();
		typed(&mut prompt, "Jan");
		submit(&mut prompt);
		assert_eq!(answer(&prompt), Some("Jan"));
	}

	#[test]
	fn an_untouched_prompt_answers_with_the_empty_string() {
		let mut prompt = text();
		submit(&mut prompt);
		assert_eq!(prompt.status(), Status::Submit);
		assert_eq!(answer(&prompt), Some(""));
	}

	#[test]
	fn the_default_value_fills_in_for_an_empty_answer() {
		let mut prompt = Prompt::new(TextState::new().with_default_value("anonymous"));
		submit(&mut prompt);
		assert_eq!(answer(&prompt), Some("anonymous"));
	}

	#[test]
	fn the_default_value_also_fills_in_for_an_answer_that_was_erased() {
		let mut prompt = Prompt::new(TextState::new().with_default_value("anonymous"));
		typed(&mut prompt, "Jan");
		for _ in 0..3 {
			prompt.key(None, &Key::named(KeyName::Backspace));
		}
		assert_eq!(prompt.user_input(), "");
		submit(&mut prompt);
		assert_eq!(answer(&prompt), Some("anonymous"));
	}

	#[test]
	fn the_default_value_does_not_override_an_answer() {
		let mut prompt = Prompt::new(TextState::new().with_default_value("anonymous"));
		typed(&mut prompt, "Jan");
		submit(&mut prompt);
		assert_eq!(answer(&prompt), Some("Jan"));
	}

	#[test]
	fn an_initial_value_is_editable_text_rather_than_a_fallback() {
		let mut prompt = Prompt::new(TextState::new()).with_initial_user_input("Jan");
		typed(&mut prompt, "!");
		submit(&mut prompt);
		assert_eq!(answer(&prompt), Some("Jan!"));
	}

	#[test]
	fn a_cancelled_prompt_has_no_answer() {
		let mut prompt = text();
		typed(&mut prompt, "Jan");
		prompt.key(None, &Key::named(KeyName::Escape));
		assert_eq!(prompt.outcome(), Some(Outcome::Cancelled));
	}

	#[test]
	fn a_cancelled_prompt_still_finalizes_its_value() {
		// Upstream emits `finalize` for cancel as well as submit, so the default is applied either
		// way and a caller that ignores the cancel still finds a sensible value.
		let mut prompt = Prompt::new(TextState::new().with_default_value("anonymous"));
		prompt.key(None, &Key::named(KeyName::Escape));
		assert_eq!(prompt.state().value(), Some("anonymous"));
	}

	#[test]
	fn editing_keys_reach_the_line_editor() {
		let mut prompt = text();
		typed(&mut prompt, "hello world");
		prompt.key(None, &Key::ctrl('w'));
		assert_eq!(prompt.user_input(), "hello ");
		prompt.key(None, &Key::ctrl('a'));
		assert_eq!(prompt.cursor(), 0);
		typed(&mut prompt, "oh ");
		submit(&mut prompt);
		assert_eq!(answer(&prompt), Some("oh hello "));
	}

	// --- The widget -----------------------------------------------------------------------------
	//
	// The opening Frame of every harvested Scenario is compared against clack's own bytes in
	// `tests/scenario_replay.rs`, colours included. These cover the three branches a recording only
	// ever shows as a diff — submit, cancel and error — where there is no oracle until the Emitter
	// and an emulator exist. What they assert is the shape of the Frame, which is the part upstream
	// writes down as control flow rather than as styling.

	/// The text of each line, ignoring style.
	fn drawn(widget: &TextWidget<'_>) -> Vec<String> {
		widget
			.frame()
			.lines
			.iter()
			.map(|line| line.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn an_opening_frame_has_a_guide_above_the_question_and_a_foot_below_the_input() {
		let prompt = text();
		let widget = TextWidget::new(&prompt, "What is your name?");
		assert_eq!(
			drawn(&widget),
			["│", "◆  What is your name?", "│  _", "└", ""]
		);
	}

	/// The last line is the empty string a trailing newline leaves behind, and it is a row: clack's
	/// `restoreCursor` counts it when walking back up the Frame.
	#[test]
	fn the_trailing_newline_is_a_row_of_its_own() {
		let prompt = text();
		let widget = TextWidget::new(&prompt, "foo");
		assert_eq!(widget.frame().lines.len(), 5);
		assert_eq!(widget.frame().height(80), 5);
	}

	#[test]
	fn without_a_guide_only_the_question_and_the_input_remain() {
		let prompt = text();
		let widget = TextWidget::new(&prompt, "foo").with_guide(false);
		assert_eq!(drawn(&widget), ["◆  foo", "_", "", ""]);
	}

	#[test]
	fn the_placeholder_stands_in_until_something_is_typed() {
		let mut prompt = text();
		let widget = TextWidget::new(&prompt, "foo").with_placeholder("bar");
		assert_eq!(drawn(&widget)[2], "│  bar");

		typed(&mut prompt, "x");
		let widget = TextWidget::new(&prompt, "foo").with_placeholder("bar");
		assert_eq!(drawn(&widget)[2], "│  x█");
	}

	/// `defaultValue` is not `initialValue`: nothing is typed for the user, so the Frame shows the
	/// same empty field it would without one. Upstream has a Scenario named for this.
	#[test]
	fn a_default_value_does_not_render() {
		let prompt = Prompt::new(TextState::new().with_default_value("bar"));
		let widget = TextWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  _");
	}

	#[test]
	fn the_cursor_inverts_the_character_it_rests_on_rather_than_following_the_text() {
		let mut prompt = text();
		typed(&mut prompt, "ab");
		prompt.key(None, &Key::named(KeyName::Left));

		let widget = TextWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  ab");
		let input = &widget.frame().lines[2].spans;
		assert_eq!(input.last().map(|s| s.text.as_str()), Some("b"));
		assert!(
			input
				.last()
				.unwrap()
				.style
				.add_modifier
				.contains(ratatui_core::style::Modifier::REVERSED)
		);
	}

	#[test]
	fn a_submitted_frame_keeps_the_question_and_shows_the_value() {
		let mut prompt = text();
		typed(&mut prompt, "Jan");
		submit(&mut prompt);

		let widget = TextWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◇  foo", "│  Jan"]);
	}

	/// An empty answer earns no value line — but the Guide beside it stays.
	#[test]
	fn a_submitted_frame_with_no_value_still_draws_its_guide() {
		let mut prompt = text();
		submit(&mut prompt);
		let widget = TextWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◇  foo", "│"]);
	}

	#[test]
	fn a_cancelled_frame_closes_with_a_second_guide() {
		let mut prompt = text();
		typed(&mut prompt, "Jan");
		prompt.key(None, &Key::named(KeyName::Escape));

		let widget = TextWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "■  foo", "│  Jan", "│"]);
	}

	/// Upstream tests `value` to decide whether to draw the value and `value.trim()` to decide
	/// whether to close the Frame. A value of nothing but spaces is where those two disagree.
	#[test]
	fn a_cancelled_whitespace_value_is_drawn_but_does_not_close_the_frame() {
		let mut prompt = text();
		typed(&mut prompt, "  ");
		prompt.key(None, &Key::named(KeyName::Escape));

		let widget = TextWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "■  foo", "│    "]);
	}

	#[test]
	fn an_error_frame_puts_the_message_on_the_foot() {
		let mut prompt = Prompt::new(TextState::new())
			.with_validator(|_: Option<&String>| Some("too short".to_string()));
		typed(&mut prompt, "x");
		submit(&mut prompt);
		assert_eq!(prompt.status(), Status::Error);

		let widget = TextWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "▲  foo", "│  x█", "└  too short", ""]);
	}

	/// `title.trim()`, which upstream calls only in the error branch. It takes the trailing newline
	/// off the title — and, when the question ends in whitespace, that too.
	#[test]
	fn an_error_frame_trims_the_end_of_its_title() {
		let mut prompt = Prompt::new(TextState::new())
			.with_validator(|_: Option<&String>| Some("nope".to_string()));
		submit(&mut prompt);

		let widget = TextWidget::new(&prompt, "foo   ");
		assert_eq!(drawn(&widget)[1], "▲  foo");

		// And with nothing to say at all, the two spaces after the symbol go with it.
		let widget = TextWidget::new(&prompt, "");
		assert_eq!(drawn(&widget)[1], "▲");
	}
}
