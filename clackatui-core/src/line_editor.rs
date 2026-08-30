//! Node's `readline` line editor, reimplemented.
//!
//! `text`, `password` and `autocomplete` do no text editing of their own: clack builds a
//! `readline.Interface` and reads `rl.line` and `rl.cursor` after every keypress. Node's readline
//! *is* their line editor, so its default keymap is a compatibility requirement even though none of
//! it appears in clack's source. See ADR-0004.
//!
//! This is a port of `Interface[kTtyWrite]` and the editing primitives it dispatches to, from
//! `internal/readline/interface.js`. The entry point has the same shape as `rl.write(d, key)`:
//! an optional string and a key descriptor, dispatched on modifiers first and name second.
//!
//! # What is ported
//!
//! Every branch of `kTtyWrite` that can change `(line, cursor)` for a clack Prompt:
//!
//! | keys | effect |
//! |---|---|
//! | printable input, `tab` | insert; embedded line terminators split into submitted lines |
//! | `backspace`, `ctrl+h` | delete one code point left |
//! | `delete`, `ctrl+d` | delete one code point right (`ctrl+d` aborts on an empty line) |
//! | `ctrl+w`, `ctrl+backspace`, `alt+backspace` | delete word left |
//! | `ctrl+delete`, `alt+d`, `alt+delete` | delete word right |
//! | `ctrl+u`, `ctrl+shift+backspace` | kill to start of line |
//! | `ctrl+k`, `ctrl+shift+delete` | kill to end of line |
//! | `left`/`right`, `ctrl+b`/`ctrl+f` | move one code point |
//! | `home`/`end`, `ctrl+a`/`ctrl+e` | move to either end |
//! | `ctrl+left`/`ctrl+right`, `alt+b`/`alt+f` | move one word |
//! | `ctrl+y`, `alt+y` | yank, yank-pop |
//! | `ctrl+_`, `ctrl+^` | undo, redo |
//! | `return`, `enter` | submit the line and clear it |
//! | `ctrl+c` | abort |
//!
//! # What is not
//!
//! History navigation (`up`/`down`, `ctrl+p`/`ctrl+n`) and tab completion are out of scope, and
//! deliberately so rather than for lack of time: clack constructs its interface with no completer
//! and closes it when the Prompt resolves, so the history is empty for every keypress a Prompt ever
//! observes. `up` and `down` are inert here, which is what readline does with an empty history.
//!
//! Also absent is everything that is display rather than state — `kRefreshLine`, `getCursorPos`,
//! `ctrl+l`, `ctrl+z` — because the core performs no I/O. The Emitter owns the screen.
//!
//! # Indices
//!
//! `rl.cursor` is a UTF-16 code unit offset, because that is what a JavaScript string index is.
//! [`LineEditor::cursor`] is a byte offset into a UTF-8 `str`, which is what a Rust caller wants;
//! [`LineEditor::cursor_utf16`] converts, and exists so the Conformance suite can compare against
//! the number clack actually reads. The two agree on where the cursor *is* — readline steps by one
//! code point, never by one grapheme, so combining marks are separate stops on both sides.

use std::borrow::Cow;
use std::collections::VecDeque;

/// Node's cap on the kill ring, from `kMaxLengthOfKillRing`.
const MAX_KILL_RING: usize = 32;

/// Node's cap on the undo stack, from `kMaxUndoRedoStackSize`. The redo stack is uncapped upstream;
/// it is uncapped here too.
const MAX_UNDO: usize = 2048;

/// A key as readline reports it: a name, three modifier flags, and the bytes that produced it.
///
/// Mirrors Node's `Key`. `name` is always lowercase for letters — readline reports shift+A as
/// `Char('a')` with `shift` set — and dispatch here matches on it verbatim, so an uppercase
/// [`KeyName::Char`] simply falls through to insertion, exactly as it would upstream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Key {
	pub name: Option<KeyName>,
	pub ctrl: bool,
	pub meta: bool,
	pub shift: bool,
	/// The raw sequence. Only read to detect undo (`0x1F`) and redo (`0x1E`), which readline keys
	/// off the sequence rather than the name.
	pub sequence: Option<String>,
}

impl Key {
	/// A bare named key, no modifiers.
	pub fn named(name: KeyName) -> Self {
		Self {
			name: Some(name),
			..Self::default()
		}
	}

	/// `ctrl` plus a letter, as readline names it.
	pub fn ctrl(c: char) -> Self {
		Self {
			name: Some(KeyName::Char(c)),
			ctrl: true,
			..Self::default()
		}
	}

	/// `alt`/`meta` plus a letter.
	pub fn meta(c: char) -> Self {
		Self {
			name: Some(KeyName::Char(c)),
			meta: true,
			..Self::default()
		}
	}
}

/// The key names readline dispatches on. Letters carry their lowercase name in [`KeyName::Char`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyName {
	Char(char),
	Backspace,
	Delete,
	Left,
	Right,
	Home,
	End,
	Up,
	Down,
	Tab,
	/// `\r`.
	Return,
	/// `\n`.
	Enter,
	Escape,
}

impl KeyName {
	/// The string readline puts in `key.name`, which is what clack's aliases are keyed by.
	///
	/// Note `\r` is `return` and `\n` is `enter` — readline's naming, not a slip.
	pub fn readline_name(&self) -> Cow<'static, str> {
		match self {
			// readline names the space bar rather than passing the character through.
			Self::Char(' ') => Cow::Borrowed("space"),
			Self::Char(c) => Cow::Owned(c.to_string()),
			Self::Backspace => Cow::Borrowed("backspace"),
			Self::Delete => Cow::Borrowed("delete"),
			Self::Left => Cow::Borrowed("left"),
			Self::Right => Cow::Borrowed("right"),
			Self::Home => Cow::Borrowed("home"),
			Self::End => Cow::Borrowed("end"),
			Self::Up => Cow::Borrowed("up"),
			Self::Down => Cow::Borrowed("down"),
			Self::Tab => Cow::Borrowed("tab"),
			Self::Return => Cow::Borrowed("return"),
			Self::Enter => Cow::Borrowed("enter"),
			Self::Escape => Cow::Borrowed("escape"),
		}
	}
}

/// What a keypress produced beyond an edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineEvent {
	/// The line was submitted and the editor cleared. Carries the text as it was.
	Line(String),
	/// `ctrl+c`, or `ctrl+d` on an empty line — readline closes the interface here.
	Abort,
}

#[derive(Clone, Debug)]
struct Edit {
	text: String,
	cursor: usize,
}

/// A single-line text editor with Node's readline keymap.
///
/// No I/O and no history: feed it keys, read [`line`](Self::line) and [`cursor`](Self::cursor).
#[derive(Clone, Debug, Default)]
pub struct LineEditor {
	line: String,
	cursor: usize,
	kill_ring: VecDeque<String>,
	kill_ring_cursor: usize,
	yanking: bool,
	undo_stack: Vec<Edit>,
	redo_stack: Vec<Edit>,
}

impl LineEditor {
	pub fn new() -> Self {
		Self::default()
	}

	/// The current text.
	pub fn line(&self) -> &str {
		&self.line
	}

	/// The cursor as a byte offset into [`line`](Self::line). Always on a character boundary.
	pub fn cursor(&self) -> usize {
		self.cursor
	}

	/// The cursor as a UTF-16 code unit offset — the number `rl.cursor` reports, and the one clack
	/// copies into `Prompt._cursor`.
	pub fn cursor_utf16(&self) -> usize {
		self.line[..self.cursor].chars().map(char::len_utf16).sum()
	}

	/// Replace the text and put the cursor at the end, as `rl.write(text)` does from empty.
	///
	/// clack calls this for `initialUserInput` and when a failed validation re-writes the value.
	pub fn set_line(&mut self, text: impl Into<String>) {
		self.line = text.into();
		self.cursor = self.line.len();
	}

	/// Feed one keypress. Mirrors `rl.write(d, key)`, which for a terminal is `kTtyWrite`.
	///
	/// Returns the events the keypress produced. A single key produces at most one; a pasted
	/// string containing line terminators submits one line per terminator, which is why this is a
	/// vector rather than an `Option`. The empty case does not allocate.
	pub fn write(&mut self, s: Option<&str>, key: &Key) -> Vec<LineEvent> {
		// Reset yanking state unless we are doing yank pop.
		if !(key.meta && key.name == Some(KeyName::Char('y'))) {
			self.yanking = false;
		}

		// Undo and redo are keyed off the sequence, not the name, and return early.
		if let Some(seq) = &key.sequence {
			match seq.chars().next() {
				Some('\u{1F}') => {
					self.undo();
					return Vec::new();
				}
				Some('\u{1E}') => {
					self.redo();
					return Vec::new();
				}
				_ => {}
			}
		}

		// Ignore the escape key, as readline does.
		if key.name == Some(KeyName::Escape) {
			return Vec::new();
		}

		if key.ctrl && key.shift {
			match key.name {
				Some(KeyName::Backspace) => self.delete_line_left(),
				Some(KeyName::Delete) => self.delete_line_right(),
				_ => {}
			}
		} else if key.ctrl {
			match key.name {
				Some(KeyName::Char('c')) => return vec![LineEvent::Abort],
				Some(KeyName::Char('h')) => self.delete_left(),
				Some(KeyName::Char('d')) => {
					if self.cursor == 0 && self.line.is_empty() {
						return vec![LineEvent::Abort];
					} else if self.cursor < self.line.len() {
						self.delete_right();
					}
				}
				Some(KeyName::Char('u')) => self.delete_line_left(),
				Some(KeyName::Char('k')) => self.delete_line_right(),
				Some(KeyName::Char('a')) => self.cursor = 0,
				Some(KeyName::Char('e')) => self.cursor = self.line.len(),
				Some(KeyName::Char('b')) => self.cursor = self.prev_boundary(),
				Some(KeyName::Char('f')) => self.cursor = self.next_boundary(),
				Some(KeyName::Char('y')) => self.yank(),
				// `ctrl+l` clears the screen, `ctrl+z` suspends, `ctrl+n`/`ctrl+p` walk an empty
				// history: none of them touch (line, cursor).
				Some(KeyName::Char('w') | KeyName::Backspace) => self.delete_word_left(),
				Some(KeyName::Delete) => self.delete_word_right(),
				Some(KeyName::Left) => self.word_left(),
				Some(KeyName::Right) => self.word_right(),
				_ => {}
			}
		} else if key.meta {
			match key.name {
				Some(KeyName::Char('b')) => self.word_left(),
				Some(KeyName::Char('f')) => self.word_right(),
				Some(KeyName::Char('d') | KeyName::Delete) => self.delete_word_right(),
				Some(KeyName::Backspace) => self.delete_word_left(),
				Some(KeyName::Char('y')) => self.yank_pop(),
				_ => {}
			}
		} else {
			match key.name {
				// Upstream distinguishes `\r` from `\n` only to collapse a CRLF pair arriving
				// within `crlfDelay`. That is a property of the byte stream, so it belongs to the
				// key parser; by the time a key reaches here the pair is already one event.
				Some(KeyName::Return | KeyName::Enter) => return vec![self.submit()],
				Some(KeyName::Backspace) => self.delete_left(),
				Some(KeyName::Delete) => self.delete_right(),
				Some(KeyName::Left) => self.cursor = self.prev_boundary(),
				Some(KeyName::Right) => self.cursor = self.next_boundary(),
				Some(KeyName::Home) => self.cursor = 0,
				Some(KeyName::End) => self.cursor = self.line.len(),
				// `up` and `down` navigate history, which is always empty here. See the module docs.
				Some(KeyName::Up | KeyName::Down) => {}
				// With no completer, `tab` falls through to insertion — it types a literal tab.
				_ => return self.insert_text(s.unwrap_or("")),
			}
		}

		Vec::new()
	}

	/// `kTtyWrite`'s default branch: insert, splitting on line terminators and submitting each.
	fn insert_text(&mut self, s: &str) -> Vec<LineEvent> {
		if s.is_empty() {
			return Vec::new();
		}

		let mut events = Vec::new();
		let mut rest = s;
		while let Some((before, after)) = split_at_line_ending(rest) {
			self.insert_string(before);
			events.push(self.submit());
			rest = after;
		}
		self.insert_string(rest);
		events
	}

	/// `kLine`: the line is handed off, the undo history is discarded, the editor is cleared.
	fn submit(&mut self) -> LineEvent {
		let line = std::mem::take(&mut self.line);
		self.cursor = 0;
		self.undo_stack.clear();
		self.redo_stack.clear();
		LineEvent::Line(line)
	}

	// -- editing primitives ------------------------------------------------------------------
	//
	// `kMoveCursor(dx)` is not ported as such. Its only effect on state is `cursor` clamped into
	// `0..=line.len()`, and every caller knows the offset it wants, so the callers above assign it.

	/// `kInsertString`.
	///
	/// An empty insert still records an undo entry: upstream runs `kBeforeEdit` unconditionally,
	/// before the emptiness is ever considered.
	fn insert_string(&mut self, c: &str) {
		self.before_edit();
		self.line.insert_str(self.cursor, c);
		self.cursor += c.len();
	}

	/// `kDeleteLeft`.
	fn delete_left(&mut self) {
		if self.cursor > 0 && !self.line.is_empty() {
			self.before_edit();
			let start = self.prev_boundary();
			self.line.replace_range(start..self.cursor, "");
			self.cursor = start;
		}
	}

	/// `kDeleteRight`.
	fn delete_right(&mut self) {
		if self.cursor < self.line.len() {
			self.before_edit();
			let end = self.next_boundary();
			self.line.replace_range(self.cursor..end, "");
		}
	}

	/// `kWordLeft`.
	fn word_left(&mut self) {
		if self.cursor > 0 {
			self.cursor = self.word_start();
		}
	}

	/// `kWordRight`.
	fn word_right(&mut self) {
		if self.cursor < self.line.len() {
			self.cursor = self.word_end();
		}
	}

	/// `kDeleteWordLeft`.
	fn delete_word_left(&mut self) {
		if self.cursor > 0 {
			self.before_edit();
			let start = self.word_start();
			self.line.replace_range(start..self.cursor, "");
			self.cursor = start;
		}
	}

	/// `kDeleteWordRight`. Note that the cursor does not move, and that upstream uses a *different*
	/// pattern here than `kWordRight` does — see [`word_end_for_delete`].
	fn delete_word_right(&mut self) {
		if self.cursor < self.line.len() {
			self.before_edit();
			let end = self.cursor + word_end_for_delete(&self.line[self.cursor..]);
			self.line.replace_range(self.cursor..end, "");
		}
	}

	/// `kDeleteLineLeft`. Pushes an undo entry unconditionally, even with nothing to delete.
	fn delete_line_left(&mut self) {
		self.before_edit();
		let del = self.line[..self.cursor].to_owned();
		self.line.replace_range(..self.cursor, "");
		self.cursor = 0;
		self.push_to_kill_ring(del);
	}

	/// `kDeleteLineRight`.
	fn delete_line_right(&mut self) {
		self.before_edit();
		let del = self.line[self.cursor..].to_owned();
		self.line.truncate(self.cursor);
		self.push_to_kill_ring(del);
	}

	// -- kill ring ---------------------------------------------------------------------------

	/// `kPushToKillRing`. An empty kill, or one identical to the most recent, is dropped.
	fn push_to_kill_ring(&mut self, del: String) {
		if del.is_empty() || self.kill_ring.front().is_some_and(|top| *top == del) {
			return;
		}
		self.kill_ring.push_front(del);
		self.kill_ring_cursor = 0;
		while self.kill_ring.len() > MAX_KILL_RING {
			self.kill_ring.pop_back();
		}
	}

	/// `kYank`.
	fn yank(&mut self) {
		if let Some(text) = self.kill_ring.get(self.kill_ring_cursor).cloned() {
			self.yanking = true;
			self.insert_string(&text);
		}
	}

	/// `kYankPop`. Only valid immediately after a yank, and only with more than one kill recorded.
	fn yank_pop(&mut self) {
		if !self.yanking || self.kill_ring.len() <= 1 {
			return;
		}
		let last = self.kill_ring[self.kill_ring_cursor].clone();
		self.kill_ring_cursor = (self.kill_ring_cursor + 1) % self.kill_ring.len();
		let current = self.kill_ring[self.kill_ring_cursor].clone();

		let head_end = self.cursor.saturating_sub(last.len());
		let mut next = self.line[..head_end].to_owned();
		next.push_str(&current);
		let cursor = next.len();
		next.push_str(&self.line[self.cursor..]);

		self.line = next;
		self.cursor = cursor;
	}

	// -- undo and redo -----------------------------------------------------------------------

	/// `kBeforeEdit`. Every primitive that can change the text records the state before it.
	fn before_edit(&mut self) {
		self.undo_stack.push(Edit {
			text: self.line.clone(),
			cursor: self.cursor,
		});
		if self.undo_stack.len() > MAX_UNDO {
			self.undo_stack.remove(0);
		}
	}

	/// `kUndo`.
	fn undo(&mut self) {
		let Some(entry) = self.undo_stack.pop() else {
			return;
		};
		self.redo_stack.push(Edit {
			text: std::mem::replace(&mut self.line, entry.text),
			cursor: self.cursor,
		});
		self.cursor = entry.cursor;
	}

	/// `kRedo`.
	fn redo(&mut self) {
		let Some(entry) = self.redo_stack.pop() else {
			return;
		};
		self.undo_stack.push(Edit {
			text: std::mem::replace(&mut self.line, entry.text),
			cursor: self.cursor,
		});
		self.cursor = entry.cursor;
	}

	// -- offsets -----------------------------------------------------------------------------

	/// `charLengthLeft`: one code point back, clamped at the start.
	fn prev_boundary(&self) -> usize {
		match self.line[..self.cursor].chars().next_back() {
			Some(c) => self.cursor - c.len_utf8(),
			None => 0,
		}
	}

	/// `charLengthAt`: one code point forward, clamped at the end. Upstream returns 1 past the end
	/// and lets `kMoveCursor` clamp; the clamp is folded in here.
	fn next_boundary(&self) -> usize {
		match self.line[self.cursor..].chars().next() {
			Some(c) => self.cursor + c.len_utf8(),
			None => self.line.len(),
		}
	}

	fn word_start(&self) -> usize {
		self.cursor - word_start_offset(&self.line[..self.cursor])
	}

	fn word_end(&self) -> usize {
		self.cursor + word_end_offset(&self.line[self.cursor..])
	}
}

/// The three classes readline's word patterns partition characters into.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
	Space,
	Word,
	Other,
}

fn class_of(c: char) -> Class {
	if is_js_space(c) {
		Class::Space
	} else if is_js_word(c) {
		Class::Word
	} else {
		Class::Other
	}
}

/// JavaScript's `\w` without the `u` flag: ASCII only, and `_` counts.
fn is_js_word(c: char) -> bool {
	c.is_ascii_alphanumeric() || c == '_'
}

/// JavaScript's `\s`: ASCII whitespace plus the Unicode space separators, the line separators, and
/// the byte order mark. Deliberately not `char::is_whitespace`, which excludes `U+FEFF` and
/// includes `U+0085`.
fn is_js_space(c: char) -> bool {
	matches!(
		c,
		'\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
			..='\u{200A}'
				| '\u{2028}' | '\u{2029}'
				| '\u{202F}' | '\u{205F}'
				| '\u{3000}' | '\u{FEFF}'
	)
}

/// `/^\s*(?:[^\w\s]+|\w+)?/` applied to the reversed text before the cursor, as a backwards scan.
///
/// Reversing is upstream's trick for avoiding a quadratic anchored match; it cannot change the
/// length of the match, because every element of the pattern is a character class. Returns how many
/// bytes before the cursor the match covers.
fn word_start_offset(leading: &str) -> usize {
	let mut chars = leading.chars().rev().peekable();
	let mut len = 0;

	while chars.peek().is_some_and(|c| is_js_space(*c)) {
		len += chars.next().expect("peeked").len_utf8();
	}

	if let Some(class) = chars
		.peek()
		.map(|c| class_of(*c))
		.filter(|c| *c != Class::Space)
	{
		while chars.peek().is_some_and(|c| class_of(*c) == class) {
			len += chars.next().expect("peeked").len_utf8();
		}
	}

	len
}

/// `/^(?:\s+|[^\w\s]+|\w+)\s*/` — a run of one class, then any whitespace after it.
fn word_end_offset(trailing: &str) -> usize {
	let mut chars = trailing.chars().peekable();
	let Some(class) = chars.peek().map(|c| class_of(*c)) else {
		return 0;
	};

	let mut len = 0;
	while chars.peek().is_some_and(|c| class_of(*c) == class) {
		len += chars.next().expect("peeked").len_utf8();
	}
	while chars.peek().is_some_and(|c| is_js_space(*c)) {
		len += chars.next().expect("peeked").len_utf8();
	}
	len
}

/// `/^(?:\s+|\W+|\w+)\s*/` — what `kDeleteWordRight` uses, and *not* what `kWordRight` uses.
///
/// The middle alternative is `\W+` rather than `[^\w\s]+`, and `\W` includes whitespace. So a run
/// of punctuation swallows any whitespace inside it and keeps going: `alt+f` on `"! !x"` stops
/// after `"! "`, while `alt+d` deletes `"! !"`. The divergence is upstream's, not a transcription
/// slip, and it is the sort of thing ADR-0004 exists to pin down.
fn word_end_for_delete(trailing: &str) -> usize {
	let mut chars = trailing.chars().peekable();
	let Some(first) = chars.peek().copied() else {
		return 0;
	};

	let mut len = 0;
	if is_js_space(first) {
		while chars.peek().is_some_and(|c| is_js_space(*c)) {
			len += chars.next().expect("peeked").len_utf8();
		}
		return len;
	}

	let word = is_js_word(first);
	while chars.peek().is_some_and(|c| {
		if word {
			is_js_word(*c)
		} else {
			!is_js_word(*c)
		}
	}) {
		len += chars.next().expect("peeked").len_utf8();
	}
	while chars.peek().is_some_and(|c| is_js_space(*c)) {
		len += chars.next().expect("peeked").len_utf8();
	}
	len
}

/// Upstream's `lineEnding` pattern — CRLF, LF, a lone CR, U+2028 or U+2029 — matched
/// once, returning the text before the terminator and the text after it.
fn split_at_line_ending(s: &str) -> Option<(&str, &str)> {
	for (i, c) in s.char_indices() {
		let end = match c {
			'\n' | '\u{2028}' | '\u{2029}' => i + c.len_utf8(),
			'\r' => {
				if s[i + 1..].starts_with('\n') {
					i + 2
				} else {
					i + 1
				}
			}
			_ => continue,
		};
		return Some((&s[..i], &s[end..]));
	}
	None
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Type a string the way a terminal would: one keypress per character.
	fn typed(text: &str) -> LineEditor {
		let mut editor = LineEditor::new();
		for c in text.chars() {
			let mut buf = [0u8; 4];
			editor.write(Some(c.encode_utf8(&mut buf)), &Key::named(KeyName::Char(c)));
		}
		editor
	}

	fn state(editor: &LineEditor) -> (&str, usize) {
		(editor.line(), editor.cursor())
	}

	#[test]
	fn typing_appends_and_advances() {
		let editor = typed("hello");
		assert_eq!(state(&editor), ("hello", 5));
	}

	#[test]
	fn insertion_happens_at_the_cursor() {
		let mut editor = typed("ac");
		editor.write(None, &Key::named(KeyName::Left));
		editor.write(Some("b"), &Key::named(KeyName::Char('b')));
		assert_eq!(state(&editor), ("abc", 2));
	}

	#[test]
	fn the_cursor_steps_by_code_point_not_byte() {
		// U+00E9 is two bytes in UTF-8 but one code unit in UTF-16; the astral emoji is four bytes
		// and two code units. One `left` crosses either in a single step.
		let mut editor = typed("é\u{1F600}");
		assert_eq!(editor.cursor(), 6);
		assert_eq!(editor.cursor_utf16(), 3);

		editor.write(None, &Key::named(KeyName::Left));
		assert_eq!(editor.cursor(), 2);
		assert_eq!(editor.cursor_utf16(), 1);

		editor.write(None, &Key::named(KeyName::Left));
		assert_eq!(editor.cursor(), 0);
		assert_eq!(editor.cursor_utf16(), 0);
	}

	#[test]
	fn the_cursor_steps_by_code_point_not_grapheme() {
		// e + U+0301 is one grapheme and two code points. readline stops between them, so we do.
		let mut editor = typed("e\u{301}");
		editor.write(None, &Key::named(KeyName::Left));
		assert_eq!(state(&editor), ("e\u{301}", 1));
	}

	#[test]
	fn movement_clamps_at_both_ends() {
		let mut editor = typed("ab");
		for _ in 0..5 {
			editor.write(None, &Key::named(KeyName::Left));
		}
		assert_eq!(editor.cursor(), 0);
		for _ in 0..5 {
			editor.write(None, &Key::named(KeyName::Right));
		}
		assert_eq!(editor.cursor(), 2);
	}

	#[test]
	fn home_and_end_match_ctrl_a_and_ctrl_e() {
		let mut a = typed("hello");
		let mut b = typed("hello");
		a.write(None, &Key::named(KeyName::Home));
		b.write(None, &Key::ctrl('a'));
		assert_eq!(state(&a), state(&b));

		a.write(None, &Key::named(KeyName::End));
		b.write(None, &Key::ctrl('e'));
		assert_eq!(state(&a), state(&b));
		assert_eq!(a.cursor(), 5);
	}

	#[test]
	fn backspace_and_ctrl_h_delete_left() {
		let mut editor = typed("abc");
		editor.write(None, &Key::named(KeyName::Backspace));
		assert_eq!(state(&editor), ("ab", 2));
		editor.write(None, &Key::ctrl('h'));
		assert_eq!(state(&editor), ("a", 1));
	}

	#[test]
	fn delete_removes_forward_without_moving() {
		let mut editor = typed("abc");
		editor.write(None, &Key::ctrl('a'));
		editor.write(None, &Key::named(KeyName::Delete));
		assert_eq!(state(&editor), ("bc", 0));
	}

	#[test]
	fn ctrl_d_deletes_forward_but_aborts_on_an_empty_line() {
		let mut editor = typed("ab");
		editor.write(None, &Key::ctrl('a'));
		assert_eq!(editor.write(None, &Key::ctrl('d')), vec![]);
		assert_eq!(state(&editor), ("b", 0));

		editor.write(None, &Key::ctrl('d'));
		assert_eq!(state(&editor), ("", 0));
		assert_eq!(editor.write(None, &Key::ctrl('d')), vec![LineEvent::Abort]);
	}

	#[test]
	fn ctrl_d_at_the_end_of_a_non_empty_line_does_nothing() {
		let mut editor = typed("ab");
		assert_eq!(editor.write(None, &Key::ctrl('d')), vec![]);
		assert_eq!(state(&editor), ("ab", 2));
	}

	#[test]
	fn word_motion_crosses_whitespace_then_one_class() {
		let mut editor = typed("foo bar");
		editor.write(None, &Key::meta('b'));
		assert_eq!(editor.cursor(), 4);
		editor.write(None, &Key::meta('b'));
		assert_eq!(editor.cursor(), 0);

		editor.write(None, &Key::meta('f'));
		// `\w+` then `\s*`: the trailing space comes along.
		assert_eq!(editor.cursor(), 4);
		editor.write(None, &Key::meta('f'));
		assert_eq!(editor.cursor(), 7);
	}

	#[test]
	fn word_motion_treats_punctuation_as_its_own_class() {
		let mut editor = typed("a->b");
		editor.write(None, &Key::ctrl('a'));
		editor.write(None, &Key::meta('f'));
		assert_eq!(editor.cursor(), 1);
		editor.write(None, &Key::meta('f'));
		assert_eq!(editor.cursor(), 3);
	}

	#[test]
	fn ctrl_w_deletes_the_word_left() {
		let mut editor = typed("foo bar");
		editor.write(None, &Key::ctrl('w'));
		assert_eq!(state(&editor), ("foo ", 4));
		editor.write(None, &Key::ctrl('w'));
		assert_eq!(state(&editor), ("", 0));
	}

	/// The one place `kDeleteWordRight` and `kWordRight` disagree, because upstream wrote `\W+` in
	/// one and `[^\w\s]+` in the other.
	#[test]
	fn delete_word_right_swallows_whitespace_that_word_right_stops_at() {
		let mut moving = typed("! !x");
		moving.write(None, &Key::ctrl('a'));
		moving.write(None, &Key::meta('f'));
		assert_eq!(moving.cursor(), 2);

		let mut deleting = typed("! !x");
		deleting.write(None, &Key::ctrl('a'));
		deleting.write(None, &Key::meta('d'));
		assert_eq!(state(&deleting), ("x", 0));
	}

	#[test]
	fn ctrl_u_and_ctrl_k_split_the_line_at_the_cursor() {
		let mut editor = typed("foobar");
		for _ in 0..3 {
			editor.write(None, &Key::named(KeyName::Left));
		}
		editor.write(None, &Key::ctrl('k'));
		assert_eq!(state(&editor), ("foo", 3));
		editor.write(None, &Key::ctrl('u'));
		assert_eq!(state(&editor), ("", 0));
	}

	#[test]
	fn a_kill_can_be_yanked_back() {
		let mut editor = typed("foo bar");
		editor.write(None, &Key::ctrl('u'));
		assert_eq!(state(&editor), ("", 0));
		editor.write(None, &Key::ctrl('y'));
		assert_eq!(state(&editor), ("foo bar", 7));
	}

	#[test]
	fn yank_pop_walks_back_through_the_kill_ring() {
		let mut editor = typed("first");
		editor.write(None, &Key::ctrl('u'));
		editor.set_line("second");
		editor.write(None, &Key::ctrl('u'));

		editor.write(None, &Key::ctrl('y'));
		assert_eq!(state(&editor), ("second", 6));
		editor.write(None, &Key::meta('y'));
		assert_eq!(state(&editor), ("first", 5));
	}

	#[test]
	fn yank_pop_needs_a_yank_immediately_before_it() {
		let mut editor = typed("first");
		editor.write(None, &Key::ctrl('u'));
		editor.set_line("second");
		editor.write(None, &Key::ctrl('u'));

		editor.write(None, &Key::ctrl('y'));
		editor.write(None, &Key::named(KeyName::Left));
		editor.write(None, &Key::meta('y'));
		assert_eq!(state(&editor), ("second", 5));
	}

	#[test]
	fn an_identical_kill_is_not_pushed_twice() {
		let mut editor = LineEditor::new();
		editor.set_line("same");
		editor.write(None, &Key::ctrl('u'));
		editor.set_line("same");
		editor.write(None, &Key::ctrl('u'));
		editor.set_line("other");
		editor.write(None, &Key::ctrl('u'));

		editor.write(None, &Key::ctrl('y'));
		assert_eq!(editor.line(), "other");
		editor.write(None, &Key::meta('y'));
		assert_eq!(editor.line(), "same");
		// Two entries, not three: the duplicate never made it in.
		editor.write(None, &Key::meta('y'));
		assert_eq!(editor.line(), "other");
	}

	fn undo_key() -> Key {
		Key {
			sequence: Some("\u{1F}".to_owned()),
			..Key::default()
		}
	}

	fn redo_key() -> Key {
		Key {
			sequence: Some("\u{1E}".to_owned()),
			..Key::default()
		}
	}

	#[test]
	fn undo_steps_back_through_edits_and_redo_forward() {
		let mut editor = typed("ab");
		editor.write(None, &undo_key());
		assert_eq!(state(&editor), ("a", 1));
		editor.write(None, &undo_key());
		assert_eq!(state(&editor), ("", 0));
		editor.write(None, &undo_key());
		assert_eq!(state(&editor), ("", 0));

		editor.write(None, &redo_key());
		assert_eq!(state(&editor), ("a", 1));
		editor.write(None, &redo_key());
		assert_eq!(state(&editor), ("ab", 2));
	}

	#[test]
	fn movement_is_not_an_edit_and_so_is_not_undone() {
		let mut editor = typed("ab");
		editor.write(None, &Key::named(KeyName::Left));
		editor.write(None, &undo_key());
		// Back to before the `b`, cursor restored to where the edit happened.
		assert_eq!(state(&editor), ("a", 1));
	}

	#[test]
	fn escape_is_ignored_entirely() {
		let mut editor = typed("ab");
		editor.write(Some("\u{1B}"), &Key::named(KeyName::Escape));
		assert_eq!(state(&editor), ("ab", 2));
	}

	#[test]
	fn tab_types_a_tab_because_there_is_no_completer() {
		let mut editor = LineEditor::new();
		editor.write(Some("\t"), &Key::named(KeyName::Tab));
		assert_eq!(state(&editor), ("\t", 1));
	}

	#[test]
	fn history_keys_are_inert() {
		let mut editor = typed("ab");
		for key in [
			Key::named(KeyName::Up),
			Key::named(KeyName::Down),
			Key::ctrl('p'),
			Key::ctrl('n'),
		] {
			editor.write(None, &key);
		}
		assert_eq!(state(&editor), ("ab", 2));
	}

	#[test]
	fn return_submits_and_clears() {
		let mut editor = typed("hello");
		let events = editor.write(Some("\r"), &Key::named(KeyName::Return));
		assert_eq!(events, vec![LineEvent::Line("hello".to_owned())]);
		assert_eq!(state(&editor), ("", 0));
	}

	#[test]
	fn a_submitted_line_cannot_be_undone_back() {
		let mut editor = typed("hello");
		editor.write(Some("\r"), &Key::named(KeyName::Return));
		editor.write(None, &undo_key());
		assert_eq!(state(&editor), ("", 0));
	}

	#[test]
	fn a_pasted_string_submits_one_line_per_terminator() {
		let mut editor = LineEditor::new();
		let events = editor.write(Some("one\r\ntwo\rthree\nfour"), &Key::default());
		assert_eq!(
			events,
			vec![
				LineEvent::Line("one".to_owned()),
				LineEvent::Line("two".to_owned()),
				LineEvent::Line("three".to_owned()),
			]
		);
		assert_eq!(state(&editor), ("four", 4));
	}

	#[test]
	fn ctrl_c_aborts_without_touching_the_line() {
		let mut editor = typed("ab");
		assert_eq!(editor.write(None, &Key::ctrl('c')), vec![LineEvent::Abort]);
		assert_eq!(state(&editor), ("ab", 2));
	}

	#[test]
	fn a_bom_counts_as_whitespace_for_word_boundaries() {
		// JavaScript's `\s` includes U+FEFF, which `char::is_whitespace` does not.
		let mut editor = typed("foo\u{FEFF}bar");
		editor.write(None, &Key::ctrl('w'));
		assert_eq!(editor.line(), "foo\u{FEFF}");
	}
}
