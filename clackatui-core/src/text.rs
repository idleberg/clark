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
//! builder in `clackatui` will accept both names and resolve them the way upstream does.

use crate::prompt::PromptState;

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
}
