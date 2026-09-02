//! crossterm's key events, as Node's `readline` would have reported them.
//!
//! Everything below the driver is a port of code that reads `readline`'s `Key` — the Line editor
//! dispatches on `key.name` and `key.ctrl`, and the Prompt matches aliases against the *character*,
//! the name and the escape sequence in that order (ADR-0004). So the driver's job is not to invent
//! a key model but to arrive at readline's, from a terminal library that has its own.
//!
//! This is a port of `emitKeypressEvents` in `internal/readline/utils.js`, specifically the naming
//! rules it applies once a sequence has been recognised. Three of them are easy to get wrong and
//! all three are load-bearing:
//!
//!   - **A control key still carries its character.** readline hands `ctrl+c` the string `"\u{3}"`
//!     as well as the name `c`, and clack's cancel alias is keyed by that string — so a decoder
//!     that dropped it would leave `ctrl+c` inert.
//!   - **Punctuation has no name at all.** readline names letters, digits and a fixed list of
//!     special keys; `!` and `.` arrive with `key.name` undefined. The Line editor's insertion
//!     branch is the fallthrough for exactly that case.
//!   - **An escaped key carries no character.** Anything readline decoded from an escape sequence —
//!     the arrows, `home`, `alt+b` — is emitted with `char` undefined, because the bytes that
//!     produced it are not text.
//!
//! # No oracle yet
//!
//! README lists "key parsing (Node `readline` vs crossterm)" among the Conformance suites, and it
//! is the one that has not been harvested. Until it is, this module is a close reading rather than
//! a verified port, and it is the least-guarded thing in the project — the tests below pin what it
//! is meant to do, not that readline agrees.

use clackatui_core::line_editor::{Key, KeyName};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// One crossterm key event as `(char, key)` — the two arguments readline emits.
///
/// `char` is `None` for a key that came out of an escape sequence, and for a key readline has no
/// bytes for at all. A key that clack could never act on still decodes: it arrives with no name and
/// no character, which is inert everywhere rather than a case anyone has to handle.
pub fn decode(event: KeyEvent) -> (Option<String>, Key) {
	let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
	let meta = event.modifiers.contains(KeyModifiers::ALT);
	let shift = event.modifiers.contains(KeyModifiers::SHIFT);

	let mut key = Key {
		ctrl,
		meta,
		shift,
		..Key::default()
	};

	// `char` and `sequence` are the same string for a key that is text and differ for one that is
	// not, so they are tracked apart: `sequence` is always what the terminal sent, `char` is what
	// readline passes on.
	let (name, sequence, escaped) = match event.code {
		KeyCode::Char(c) if ctrl => {
			// readline reads the control byte itself and derives the letter back from it, which is
			// why the character survives — see the cancel alias above.
			let byte = control_byte(c);
			(
				Some(KeyName::Char(c.to_ascii_lowercase())),
				byte.map(String::from),
				false,
			)
		}
		KeyCode::Char(' ') => (Some(KeyName::Char(' ')), Some(" ".to_owned()), meta),
		KeyCode::Char(c) if c.is_ascii_alphanumeric() => (
			Some(KeyName::Char(c.to_ascii_lowercase())),
			Some(c.to_string()),
			meta,
		),
		// `alt` plus a letter is `ESC` plus that letter, and readline names it. Anything else with
		// `alt` held is a sequence readline would not have named.
		KeyCode::Char(c) if meta => (Some(KeyName::Char(c)), Some(c.to_string()), true),
		// Punctuation: readline recognises the bytes as text and leaves the key unnamed.
		KeyCode::Char(c) => (None, Some(c.to_string()), false),

		KeyCode::Enter => (Some(KeyName::Return), Some("\r".to_owned()), false),
		KeyCode::Tab => (Some(KeyName::Tab), Some("\t".to_owned()), false),
		// readline names shift+tab `tab` as well; the flag is what tells them apart.
		KeyCode::BackTab => {
			key.shift = true;
			(Some(KeyName::Tab), Some("\u{1b}[Z".to_owned()), true)
		}
		KeyCode::Backspace => (Some(KeyName::Backspace), Some("\u{7f}".to_owned()), false),
		KeyCode::Esc => (Some(KeyName::Escape), Some("\u{1b}".to_owned()), false),

		// Everything readline decodes out of an escape sequence, and so emits with no character.
		KeyCode::Delete => (Some(KeyName::Delete), Some("\u{1b}[3~".to_owned()), true),
		KeyCode::Left => (Some(KeyName::Left), Some("\u{1b}[D".to_owned()), true),
		KeyCode::Right => (Some(KeyName::Right), Some("\u{1b}[C".to_owned()), true),
		KeyCode::Up => (Some(KeyName::Up), Some("\u{1b}[A".to_owned()), true),
		KeyCode::Down => (Some(KeyName::Down), Some("\u{1b}[B".to_owned()), true),
		KeyCode::Home => (Some(KeyName::Home), Some("\u{1b}[H".to_owned()), true),
		KeyCode::End => (Some(KeyName::End), Some("\u{1b}[F".to_owned()), true),

		// Function keys, media keys, `insert`, `page up` — readline names some of them and clack
		// acts on none. Left unnamed rather than given a name nothing reads.
		_ => (None, None, true),
	};

	key.name = name;
	key.sequence = sequence.clone();

	let character = if escaped { None } else { sequence };
	(character, key)
}

/// The byte a terminal sends for `ctrl` plus a character, or `None` when it sends nothing special.
///
/// `ctrl+a` through `ctrl+z` are `0x01`–`0x1a`; the four that follow are readline's undo and redo
/// keys among others, and are named here because [`clackatui_core::line_editor`] dispatches undo
/// and redo off the sequence rather than off the name.
fn control_byte(c: char) -> Option<char> {
	match c {
		'a'..='z' => char::from_u32(c as u32 - 'a' as u32 + 1),
		'A'..='Z' => char::from_u32(c as u32 - 'A' as u32 + 1),
		'[' => Some('\u{1b}'),
		'\\' => Some('\u{1c}'),
		']' => Some('\u{1d}'),
		'^' => Some('\u{1e}'),
		'_' => Some('\u{1f}'),
		' ' | '@' => Some('\u{0}'),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn key(code: KeyCode, modifiers: KeyModifiers) -> (Option<String>, Key) {
		decode(KeyEvent::new(code, modifiers))
	}

	fn plain(code: KeyCode) -> (Option<String>, Key) {
		key(code, KeyModifiers::NONE)
	}

	#[test]
	fn a_letter_arrives_as_its_own_character_and_its_lowercase_name() {
		let (c, k) = plain(KeyCode::Char('a'));
		assert_eq!(c.as_deref(), Some("a"));
		assert_eq!(k.name, Some(KeyName::Char('a')));
		assert!(!k.shift);
	}

	/// readline reports a capital as the lowercase name with `shift` set, and the character itself
	/// unchanged — which is what leaves the editor inserting an `A` rather than an `a`.
	#[test]
	fn a_capital_keeps_its_case_in_the_character_but_not_in_the_name() {
		let (c, k) = key(KeyCode::Char('A'), KeyModifiers::SHIFT);
		assert_eq!(c.as_deref(), Some("A"));
		assert_eq!(k.name, Some(KeyName::Char('a')));
		assert!(k.shift);
	}

	#[test]
	fn punctuation_has_no_name() {
		let (c, k) = plain(KeyCode::Char('!'));
		assert_eq!(c.as_deref(), Some("!"));
		assert_eq!(k.name, None);
	}

	#[test]
	fn the_space_bar_is_named_rather_than_passed_through() {
		let (c, k) = plain(KeyCode::Char(' '));
		assert_eq!(c.as_deref(), Some(" "));
		assert_eq!(k.name.unwrap().readline_name(), "space");
	}

	/// The one that matters most: clack's cancel alias is the string `"\u{3}"`, so a `ctrl+c` that
	/// arrived without its character would not cancel anything.
	#[test]
	fn ctrl_c_carries_the_control_byte_that_the_cancel_alias_is_keyed_by() {
		let (c, k) = key(KeyCode::Char('c'), KeyModifiers::CONTROL);
		assert_eq!(c.as_deref(), Some("\u{3}"));
		assert_eq!(k.sequence.as_deref(), Some("\u{3}"));
		assert_eq!(k.name, Some(KeyName::Char('c')));
		assert!(k.ctrl);
	}

	/// Undo and redo are dispatched off the sequence, not the name, so the byte has to be right.
	#[test]
	fn ctrl_underscore_carries_the_byte_the_line_editor_reads_undo_from() {
		let (_, k) = key(KeyCode::Char('_'), KeyModifiers::CONTROL);
		assert_eq!(k.sequence.as_deref(), Some("\u{1f}"));
	}

	#[test]
	fn an_arrow_is_named_but_carries_no_character() {
		let (c, k) = plain(KeyCode::Left);
		assert_eq!(c, None);
		assert_eq!(k.name, Some(KeyName::Left));
	}

	#[test]
	fn alt_plus_a_letter_is_escaped_and_so_has_no_character_either() {
		let (c, k) = key(KeyCode::Char('b'), KeyModifiers::ALT);
		assert_eq!(c, None);
		assert_eq!(k.name, Some(KeyName::Char('b')));
		assert!(k.meta);
	}

	#[test]
	fn return_and_tab_and_backspace_carry_the_bytes_they_are_made_of() {
		assert_eq!(plain(KeyCode::Enter).0.as_deref(), Some("\r"));
		assert_eq!(plain(KeyCode::Tab).0.as_deref(), Some("\t"));
		assert_eq!(plain(KeyCode::Backspace).0.as_deref(), Some("\u{7f}"));
		assert_eq!(plain(KeyCode::Esc).0.as_deref(), Some("\u{1b}"));
	}

	/// A key clack has no use for decodes to something inert rather than to a panic or a guess.
	#[test]
	fn an_unhandled_key_is_inert_rather_than_guessed_at() {
		let (c, k) = plain(KeyCode::F(5));
		assert_eq!(c, None);
		assert_eq!(k.name, None);
		assert_eq!(k.sequence, None);
	}
}
