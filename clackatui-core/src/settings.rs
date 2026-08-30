//! Ported from `@clack/core`'s `utils/settings.ts`.
//!
//! Upstream this is one mutable module-level object plus an `updateSettings()` that merges into it.
//! Here it is a value a [`Prompt`](crate::prompt::Prompt) owns, because a process-global that every
//! Prompt reads is a poor fit for a library that also wants to be testable in parallel. The
//! defaults are clack's, so the observable behaviour of a Prompt nobody has configured is the same.
//!
//! `settings.actions` is not ported as a set. Upstream builds it once from a `const` array and
//! `updateSettings` never touches it, so its only role is to ask whether a key name is one of the
//! seven — which [`Action::from_key_name`] answers by construction.
//!
//! The date-prompt messages are not here yet; they arrive with the Prompt that reads them (M4).

use std::collections::HashMap;

use crate::line_editor::KeyName;

/// A semantic navigation event, decoupled from the key that produced it.
///
/// Upstream's `Action`. Prompts that do not track text (select and friends) subscribe to these
/// rather than to raw keys, which is how the vim aliases work without every Prompt knowing about
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
	Up,
	Down,
	Left,
	Right,
	Space,
	Enter,
	Cancel,
}

impl Action {
	/// The action a key name names directly — `settings.actions.has(key.name)` upstream.
	///
	/// [`KeyName::Return`] is deliberately absent. readline names `\r` `return` and `\n` `enter`,
	/// and only the latter is in upstream's action list, so `Enter` fires on `\n` alone. Submission
	/// on `\r` is handled by the Prompt itself and never travels as an action.
	///
	/// `Cancel` is likewise unreachable here: no readline key is named `cancel`, so it can only
	/// arrive through an alias.
	pub fn from_key_name(name: &KeyName) -> Option<Self> {
		match name {
			KeyName::Up => Some(Self::Up),
			KeyName::Down => Some(Self::Down),
			KeyName::Left => Some(Self::Left),
			KeyName::Right => Some(Self::Right),
			KeyName::Char(' ') => Some(Self::Space),
			KeyName::Enter => Some(Self::Enter),
			_ => None,
		}
	}
}

/// The messages clack prints for outcomes that are not a value.
#[derive(Clone, Debug)]
pub struct Messages {
	pub cancel: String,
	pub error: String,
}

impl Default for Messages {
	fn default() -> Self {
		Self {
			cancel: "Canceled".into(),
			error: "Something went wrong".into(),
		}
	}
}

/// Configuration shared by every Prompt in a flow.
#[derive(Clone, Debug)]
pub struct Settings {
	/// Strings that stand in for an action. Matched against a keypress's character, its key name,
	/// and its escape sequence, in that order.
	pub aliases: HashMap<String, Action>,
	pub messages: Messages,
	/// Whether the Guide — the bar down the left margin — is drawn.
	pub with_guide: bool,
}

impl Default for Settings {
	/// clack's defaults: the vim movement keys, `ctrl+c`, and escape.
	fn default() -> Self {
		let aliases = [
			("k", Action::Up),
			("j", Action::Down),
			("h", Action::Left),
			("l", Action::Right),
			("\u{3}", Action::Cancel),
			("escape", Action::Cancel),
		];

		Self {
			aliases: aliases
				.into_iter()
				.map(|(k, v)| (k.to_string(), v))
				.collect(),
			messages: Messages::default(),
			with_guide: true,
		}
	}
}

impl Settings {
	/// `updateSettings({ aliases })`: adds an alias, and keeps the existing one if there is a clash.
	///
	/// Upstream is explicit that this "will not overwrite existing aliases", so a caller cannot
	/// take `escape` away from `cancel` by rebinding it.
	pub fn add_alias(&mut self, alias: impl Into<String>, action: Action) {
		self.aliases.entry(alias.into()).or_insert(action);
	}

	/// `isActionKey(candidates, action)`: whether any candidate is an alias for `action`.
	pub fn is_action_key(&self, candidates: &[Option<&str>], action: Action) -> bool {
		candidates
			.iter()
			.flatten()
			.any(|candidate| self.aliases.get(*candidate) == Some(&action))
	}

	/// The action a single string is an alias for.
	pub fn alias(&self, candidate: &str) -> Option<Action> {
		self.aliases.get(candidate).copied()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn return_is_not_the_enter_action() {
		// readline calls `\r` "return" and `\n` "enter"; only the latter is one of the seven.
		assert_eq!(Action::from_key_name(&KeyName::Return), None);
		assert_eq!(Action::from_key_name(&KeyName::Enter), Some(Action::Enter));
	}

	#[test]
	fn space_is_an_action_but_other_characters_are_not() {
		assert_eq!(
			Action::from_key_name(&KeyName::Char(' ')),
			Some(Action::Space)
		);
		assert_eq!(Action::from_key_name(&KeyName::Char('k')), None);
	}

	#[test]
	fn an_alias_cannot_be_rebound() {
		let mut settings = Settings::default();
		settings.add_alias("escape", Action::Up);
		assert_eq!(settings.alias("escape"), Some(Action::Cancel));
	}

	#[test]
	fn ctrl_c_is_a_cancel_alias_by_its_sequence() {
		let settings = Settings::default();
		assert!(settings.is_action_key(&[None, Some("c"), Some("\u{3}")], Action::Cancel));
		assert!(!settings.is_action_key(&[None, Some("c"), None], Action::Cancel));
	}
}
