//! Ported from `@clack/core`'s `prompts/password.ts` and `@clack/prompts`' `password()`.
//!
//! A `password` is a `text` that draws a row of mask characters instead of what was typed. The
//! state machine is almost the same — the value is the user input on every keypress, and an unset
//! value finalizes to the empty string — and the masking is entirely the widget's.
//!
//! Two things here are not `text`:
//!
//! - **The cursor is drawn on the mask, not on the text.** `userInputWithCursor` slices `masked` at
//!   an offset taken from `userInput`, which works because one mask character stands for one UTF-16
//!   code unit. See [`PasswordWidget::masked`].
//! - **`clearOnError` mutates the Prompt from inside the render callback.** Upstream's `password()`
//!   calls `this.clear()` while composing the error Frame, after it has already captured the text
//!   that Frame shows. The Frame is therefore the last one with the old value in it and the state is
//!   empty by the time the next key arrives. A widget here cannot reach the Prompt, so the option
//!   lives on [`PasswordState`] and [`PromptState::clears_after_render`] is the seam — see
//!   `Session::render`, which is where upstream's ordering is preserved.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::widgets::Widget;

use crate::frame::{Frame, Line, Span};
use crate::prompt::{Prompt, PromptState, Status};
use crate::theme::Theme;

/// The state of a `password` Prompt: the text behind the mask.
#[derive(Clone, Debug, Default)]
pub struct PasswordState {
	value: Option<String>,
	clear_on_error: bool,
}

impl PasswordState {
	pub fn new() -> Self {
		Self::default()
	}

	/// `clearOnError`: throw the typed value away when validation rejects it.
	///
	/// An option of the `render` callback upstream, and of the state here, for the reason the module
	/// docs give — it is the only option in clack that changes the Prompt rather than the Frame.
	pub fn with_clear_on_error(mut self, clear: bool) -> Self {
		self.clear_on_error = clear;
		self
	}

	/// The answer. `None` until the Prompt settles, after which it is always `Some`.
	pub fn value(&self) -> Option<&str> {
		self.value.as_deref()
	}
}

impl PromptState for PasswordState {
	type Value = String;

	fn user_input(&mut self, input: &str) {
		self.value = Some(input.to_string());
	}

	/// Upstream's is `if (this.value === undefined) this.value = ''` — a strict check, unlike
	/// `text`'s, so an empty answer stays the empty string and there is no default to fall back to.
	fn finalize(&mut self) {
		if self.value.is_none() {
			self.value = Some(String::new());
		}
	}

	fn clears_after_render(&self, status: Status) -> bool {
		self.clear_on_error && status == Status::Error
	}

	fn value(&self) -> Option<&String> {
		self.value.as_ref()
	}
}

/// A `password` Prompt drawn as a Frame — the `render` callback of `@clack/prompts`' `password()`.
pub struct PasswordWidget<'a> {
	prompt: &'a Prompt<PasswordState>,
	message: &'a str,
	mask: &'a str,
	theme: &'a Theme,
	/// `opts.withGuide ?? settings.withGuide` — `None` defers to the Prompt's Settings.
	with_guide: Option<bool>,
}

impl<'a> PasswordWidget<'a> {
	pub fn new(prompt: &'a Prompt<PasswordState>, message: &'a str) -> Self {
		Self {
			prompt,
			message,
			// `@clack/prompts` passes `S_PASSWORD_MASK`, so the core Prompt's own `•` default is
			// never the one a user sees. The Theme's is the one that reaches a terminal.
			mask: THEME.symbols.password_mask,
			theme: &THEME,
			with_guide: None,
		}
	}

	/// `mask`: the character each unit of the answer is drawn as.
	pub fn with_mask(mut self, mask: &'a str) -> Self {
		self.mask = mask;
		self
	}

	/// The Theme. Its `password_mask` is the default mask, so setting a Theme after a mask undoes
	/// the mask — set the Theme first, as with every other builder here.
	pub fn with_theme(mut self, theme: &'a Theme) -> Self {
		self.mask = theme.symbols.password_mask;
		self.theme = theme;
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

		let mut frame = Frame::new();

		if guide {
			frame.push(Span::styled(symbols.bar, styles.guide));
		}
		let title = Line::from_iter([
			self.theme.step(status),
			Span::raw("  "),
			Span::styled(self.message, styles.message),
		]);

		let masked = self.masked();

		match status {
			Status::Error => {
				frame.push(trim_end(title));

				let mut input = Line::blank();
				if guide {
					input.push(Span::styled(symbols.bar, styles.guide_error));
					input.push(Span::raw("  "));
				}
				input.push(Span::raw(masked));
				frame.push(input);

				// Unlike `text`, the two spaces belong to the prefix and the message is written
				// whether or not there is one.
				let mut end = Line::blank();
				if guide {
					end.push(Span::styled(symbols.bar_end, styles.guide_error));
					end.push(Span::raw("  "));
				}
				end.push(Span::styled(self.prompt.error(), styles.error));
				frame.push(end);
				frame.push(Line::blank());
			}

			Status::Submit => {
				frame.push(title);
				let mut settled = Line::blank();
				if guide {
					settled.push(Span::styled(symbols.bar, styles.guide));
					settled.push(Span::raw("  "));
				}
				if !masked.is_empty() {
					settled.push(Span::styled(masked, styles.submitted));
				}
				frame.push(settled);
			}

			Status::Cancel => {
				frame.push(title);
				let empty = masked.is_empty();

				let mut settled = Line::blank();
				if guide {
					settled.push(Span::styled(symbols.bar, styles.guide));
					settled.push(Span::raw("  "));
				}
				if !empty {
					settled.push(Span::styled(masked, styles.cancelled));
				}
				frame.push(settled);

				// `masked && hasGuide` — a password of nothing but spaces is still masked, so unlike
				// `text` there is no `trim()` here and no disagreement to reproduce.
				if !empty && guide {
					frame.push(Line::from(Span::styled(symbols.bar, styles.guide)));
				}
			}

			Status::Initial | Status::Active => {
				frame.push(title);

				let mut input = Line::blank();
				if guide {
					input.push(Span::styled(symbols.bar, styles.guide_active));
					input.push(Span::raw("  "));
				}
				input.spans.extend(self.masked_with_cursor(&masked));
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

	/// `get masked()`: the user input with every character replaced by the mask.
	///
	/// Upstream is `userInput.replaceAll(/./g, mask)`, and both halves of that matter. Without the
	/// `u` flag `.` matches one UTF-16 code unit, so an astral character is drawn as *two* masks —
	/// and `.` does not match a line terminator, so a newline survives into the Frame unmasked.
	/// Neither is reachable from readline, and both are cheaper to port than to argue about.
	fn masked(&self) -> String {
		let mut out = String::new();
		for c in self.prompt.user_input().chars() {
			if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
				out.push(c);
				continue;
			}
			for _ in 0..c.len_utf16() {
				out.push_str(self.mask);
			}
		}
		out
	}

	/// `get userInputWithCursor()`: the mask, with the cursor drawn on it.
	///
	/// The offset is `_cursor`, which counts UTF-16 units of the *user input*, and it is applied to
	/// the *mask* — sound only because one unit of input becomes one mask character. A mask of more
	/// than one character puts the cursor in the wrong place, upstream and here alike.
	fn masked_with_cursor(&self, masked: &str) -> Vec<Span> {
		let styles = &self.theme.styles;
		let cursor = self.prompt.cursor_utf16();

		if cursor >= self.prompt.user_input().encode_utf16().count() {
			let mut spans = Vec::with_capacity(2);
			if !masked.is_empty() {
				spans.push(Span::raw(masked));
			}
			spans.push(Span::styled("_", styles.placeholder_empty));
			return spans;
		}

		// One mask character per unit, so the UTF-16 offset is a character offset here.
		let mut chars = masked.chars();
		let before: String = chars.by_ref().take(cursor).collect();
		let at = chars.next().map(String::from).unwrap_or_default();
		let after: String = chars.collect();

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

/// The default Theme, as somewhere to borrow from.
static THEME: Theme = Theme::clack();

/// `title.trim()`, as far as a Frame line can feel it. The same one `text` uses, and for the same
/// branch — upstream calls it in the error case and nowhere else.
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

impl Widget for &PasswordWidget<'_> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		(&self.frame()).render(area, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::line_editor::{Key, KeyName};
	use crate::prompt::Outcome;
	use crate::theme::Styles;

	fn password() -> Prompt<PasswordState> {
		Prompt::new(PasswordState::new())
	}

	fn typed(prompt: &mut Prompt<PasswordState>, s: &str) {
		for c in s.chars() {
			prompt.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
		}
	}

	fn submit(prompt: &mut Prompt<PasswordState>) {
		prompt.key(None, &Key::named(KeyName::Return));
	}

	fn answer(prompt: &Prompt<PasswordState>) -> Option<&str> {
		match prompt.outcome() {
			Some(Outcome::Submitted(v)) => v.map(String::as_str),
			_ => None,
		}
	}

	fn drawn(widget: &PasswordWidget<'_>) -> Vec<String> {
		widget
			.frame()
			.lines
			.iter()
			.map(|line| line.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn the_answer_is_what_was_typed_not_what_was_drawn() {
		let mut prompt = password();
		typed(&mut prompt, "hunter2");
		submit(&mut prompt);
		assert_eq!(answer(&prompt), Some("hunter2"));
	}

	#[test]
	fn an_untouched_prompt_answers_with_the_empty_string() {
		let mut prompt = password();
		submit(&mut prompt);
		assert_eq!(answer(&prompt), Some(""));
	}

	#[test]
	fn an_opening_frame_draws_a_cursor_shaped_hole() {
		let prompt = password();
		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◆  foo", "│  _", "└", ""]);

		// And that hole is inverse *and* hidden, which is what makes it a hole rather than a `_`.
		let input = &widget.frame().lines[2].spans;
		assert_eq!(input.last().unwrap().style, Styles::CLACK.placeholder_empty);
	}

	#[test]
	fn every_character_becomes_one_mask() {
		let mut prompt = password();
		typed(&mut prompt, "abc");
		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  ▪▪▪_");
	}

	#[test]
	fn a_custom_mask_replaces_the_themes() {
		let mut prompt = password();
		typed(&mut prompt, "ab");
		let widget = PasswordWidget::new(&prompt, "foo").with_mask("*");
		assert_eq!(drawn(&widget)[2], "│  **_");
	}

	/// `/./g` with no `u` flag counts UTF-16 units, so one emoji is two masks. Upstream's, and the
	/// reason the cursor arithmetic below works at all.
	#[test]
	fn an_astral_character_is_drawn_as_two_masks() {
		let mut prompt = password();
		typed(&mut prompt, "\u{1F600}");
		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  ▪▪_");
	}

	#[test]
	fn the_cursor_inverts_the_mask_it_rests_on() {
		let mut prompt = password();
		typed(&mut prompt, "abc");
		prompt.key(None, &Key::named(KeyName::Left));

		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  ▪▪▪");
		// The cursor is on the last mask, so it is the last span and nothing follows it.
		let spans = &widget.frame().lines[2].spans;
		assert_eq!(spans.last().unwrap().style, Styles::CLACK.cursor);
		assert_eq!(spans.last().unwrap().text, "▪");
	}

	#[test]
	fn a_submitted_frame_shows_the_mask_and_never_the_answer() {
		let mut prompt = password();
		typed(&mut prompt, "ab");
		submit(&mut prompt);
		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◇  foo", "│  ▪▪"]);
	}

	/// An empty answer leaves the prefix behind — the two spaces belong to it here, unlike `text`.
	#[test]
	fn a_submitted_empty_password_still_draws_its_prefix() {
		let mut prompt = password();
		submit(&mut prompt);
		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◇  foo", "│  "]);
	}

	#[test]
	fn a_cancelled_frame_closes_with_a_second_guide() {
		let mut prompt = password();
		typed(&mut prompt, "a");
		prompt.key(None, &Key::named(KeyName::Escape));
		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "■  foo", "│  ▪", "│"]);
	}

	#[test]
	fn a_cancelled_empty_password_does_not_close_the_frame() {
		let mut prompt = password();
		prompt.key(None, &Key::named(KeyName::Escape));
		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "■  foo", "│  "]);
	}

	#[test]
	fn an_error_frame_puts_the_message_on_the_foot() {
		let mut prompt = Prompt::new(PasswordState::new())
			.with_validator(|_: Option<&String>| Some("too short".to_string()));
		typed(&mut prompt, "a");
		submit(&mut prompt);
		assert_eq!(prompt.status(), Status::Error);

		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "▲  foo", "│  ▪", "└  too short", ""]);
	}

	/// The masked text on the error row is the *old* value: upstream reads it before it clears.
	/// Clearing itself is the Session's, after the Frame is composed.
	#[test]
	fn clear_on_error_leaves_the_frame_it_was_read_for_alone() {
		let mut prompt = Prompt::new(PasswordState::new().with_clear_on_error(true))
			.with_validator(|_: Option<&String>| Some("nope".to_string()));
		typed(&mut prompt, "ab");
		submit(&mut prompt);

		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  ▪▪");

		prompt.after_render();
		assert_eq!(prompt.user_input(), "");
		let widget = PasswordWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  ");
	}

	#[test]
	fn without_clear_on_error_the_value_survives_the_frame() {
		let mut prompt = Prompt::new(PasswordState::new())
			.with_validator(|_: Option<&String>| Some("nope".to_string()));
		typed(&mut prompt, "ab");
		submit(&mut prompt);
		prompt.after_render();
		assert_eq!(prompt.user_input(), "ab");
	}

	#[test]
	fn without_a_guide_only_the_question_and_the_mask_remain() {
		let mut prompt = password();
		typed(&mut prompt, "a");
		let widget = PasswordWidget::new(&prompt, "foo").with_guide(false);
		assert_eq!(drawn(&widget), ["◆  foo", "▪_", "", ""]);
	}
}
