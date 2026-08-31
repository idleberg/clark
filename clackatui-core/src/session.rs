//! One Prompt's lifetime, as bytes: ported from `@clack/core`'s `Prompt.prompt` and `close`.
//!
//! [`Prompt`] decides what a Prompt *is* after each keypress and the [`Emitter`] decides what a
//! Frame *costs* in terminal writes. Between them sits a third thing, small enough to be easy to
//! overlook and observable enough to be worth porting: the order in which those two are asked.
//! Upstream it is spread across three methods — `prompt()` renders once before any key arrives,
//! `onKeypress` renders again after every key it has finished processing, and `close()` writes a
//! newline and then, through the `submit`/`cancel` listener, shows the cursor again.
//!
//! A recording of clack shows that order directly. Every `text` Fixture in
//! `tests/fixtures/scenarios/` opens with `ESC[?25l` and the whole opening Frame, carries one diff
//! per keypress, and ends with `\n` followed by `ESC[?25h` — in that order, the newline first. So
//! the sequence is a compatibility surface like any other, and is reproduced here rather than
//! arranged to taste by whichever driver happens to be running the Prompt.
//!
//! # Still no I/O
//!
//! A Session produces bytes and never writes them, for the same reason the Emitter does not: the
//! blocking driver lives in the `clackatui` crate, and this one has no business opening a terminal.
//! That also means a Session is the whole of a Prompt that a test can drive — feed it the keys a
//! Scenario recorded and it produces the byte stream clack would have produced, with no terminal
//! and no threads involved.
//!
//! # One divergence, recorded rather than smoothed over
//!
//! **The status after the opening Frame.** Upstream's `state` is one field shared by the state
//! machine and the writer, and `render()` moves it from `initial` to `active` once it has written
//! something. Here the two are separate: [`Emitter`] tracks "have I written a Frame yet" itself, and
//! the Prompt's [`Status`] stays [`Status::Initial`] until a keypress moves it. Nothing observable
//! turns on the difference — clack's `symbol()` draws `initial` and `active` identically, and
//! [`crate::text::TextWidget`] matches them in one arm — but a Prompt added later that told the two
//! apart would need this revisited.
//!
//! The second divergence this file used to record — re-wrapping on resize — has been settled and is
//! gone. Upstream keeps `_prevFrame` as the *wrapped* string and re-wraps it at the terminal's
//! current width to count the rows it walks back over, while the Emitter kept the rows it last laid
//! out; the two agree unless the terminal narrowed. There was no oracle for it, so it was left as
//! the straightforward thing and named here. The hand-authored resize Scenarios are that oracle,
//! and they found it: two of them differed on the Grid. `Emitter::restored` is upstream's count
//! now, and ADR-0017 has the reasoning.

use crate::emitter::Emitter;
use crate::frame::Frame;
use crate::line_editor::Key;
use crate::prompt::{Outcome, Prompt, PromptState, Status};

/// Upstream's `getColumns` fallback, for an output that is not a terminal.
const DEFAULT_COLUMNS: u16 = 80;
/// Upstream's `getRows` fallback, likewise.
const DEFAULT_ROWS: u16 = 20;

/// Upstream's `render` option: a Prompt's state, drawn.
pub type Draw<S> = dyn Fn(&Prompt<S>) -> Frame;

/// A Prompt, an Emitter, and the order upstream asks them in.
///
/// Every method that can write returns the bytes it produced; a driver's whole job is to put them
/// somewhere and to supply the keys. Once [`is_finished`](Self::is_finished) holds, every further
/// call returns nothing — upstream removes its keypress listener at that point, so a late key is
/// not merely ignored, it is never delivered.
pub struct Session<S: PromptState> {
	prompt: Prompt<S>,
	emitter: Emitter,
	draw: Box<Draw<S>>,
	columns: u16,
	rows: u16,
	closed: bool,
}

impl<S: PromptState> Session<S> {
	/// `draw` is upstream's `render` option: the callback that turns the Prompt's state into a
	/// Frame. It is a closure rather than a trait method because that is what it is upstream — the
	/// widget belongs to `@clack/prompts` and the state machine to `@clack/core`, and a `text()`
	/// builder closes over the message and placeholder that never reach the state at all.
	pub fn new(prompt: Prompt<S>, draw: impl Fn(&Prompt<S>) -> Frame + 'static) -> Self {
		Self {
			prompt,
			emitter: Emitter::new(),
			draw: Box::new(draw),
			columns: DEFAULT_COLUMNS,
			rows: DEFAULT_ROWS,
			closed: false,
		}
	}

	/// The terminal to lay Frames out for. See [`Emitter::frame`] for why it is two numbers.
	pub fn with_size(mut self, columns: u16, rows: u16) -> Self {
		self.columns = columns;
		self.rows = rows;
		self
	}

	/// The opening Frame: `prompt()`'s single `render()` before any key is read.
	///
	/// Calling it twice is harmless — the second Frame is identical to the first, and an identical
	/// Frame writes nothing — but it is meant to be called once, at the top of the loop.
	pub fn open(&mut self) -> String {
		if self.closed {
			return String::new();
		}
		self.render()
	}

	/// One keypress, and everything that follows from it.
	///
	/// This is the tail of `onKeypress`: the Prompt processes the key, the new Frame is written,
	/// and *then* — if the Prompt has settled — the Prompt is closed. The render happens before the
	/// close on both sides, which is what leaves the settled Frame on the screen with the newline
	/// after it rather than before.
	pub fn key(&mut self, s: Option<&str>, key: &Key) -> String {
		if self.closed {
			return String::new();
		}

		self.prompt.key(s, key);
		let mut out = self.render();
		if self.prompt.status().is_finished() {
			out.push_str(&self.close());
		}
		out
	}

	/// The terminal changed size, so the Frame is laid out again.
	///
	/// Upstream subscribes `render` to the output's `resize` event, so this is one more render and
	/// nothing else — in particular the Prompt's state is untouched. See the module docs for where
	/// this and upstream part company.
	pub fn resize(&mut self, columns: u16, rows: u16) -> String {
		self.columns = columns;
		self.rows = rows;
		if self.closed {
			return String::new();
		}
		self.render()
	}

	/// `close()`: the newline that leaves the settled Frame in the scrollback, and then the cursor.
	///
	/// The order is upstream's and is the other way round from what one might guess: `close` writes
	/// the newline itself, and the `submit`/`cancel` listener that shows the cursor runs afterwards,
	/// because it is triggered by the `emit` at the end of `close`.
	fn close(&mut self) -> String {
		self.closed = true;
		let mut out = self.emitter.finish();
		out.push_str(&self.emitter.show_cursor());
		out
	}

	fn render(&mut self) -> String {
		let frame = (self.draw)(&self.prompt);
		self.emitter.frame(&frame, self.columns, self.rows)
	}

	/// Whether the Prompt has settled and been closed.
	pub fn is_finished(&self) -> bool {
		self.closed
	}

	pub fn status(&self) -> Status {
		self.prompt.status()
	}

	/// The outcome, once [`is_finished`](Self::is_finished) holds.
	pub fn outcome(&self) -> Option<Outcome<'_, S::Value>> {
		self.prompt.outcome()
	}

	pub fn prompt(&self) -> &Prompt<S> {
		&self.prompt
	}

	/// The Prompt back, for a driver that wants the answer by ownership.
	pub fn into_prompt(self) -> Prompt<S> {
		self.prompt
	}

	pub fn columns(&self) -> u16 {
		self.columns
	}

	pub fn rows(&self) -> u16 {
		self.rows
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::frame::{Line, Span};
	use crate::line_editor::KeyName;
	use crate::text::{TextState, TextWidget};

	const HIDE: &str = "\u{1b}[?25l";
	const SHOW: &str = "\u{1b}[?25h";

	fn text(message: &'static str) -> Session<TextState> {
		Session::new(Prompt::new(TextState::new()), move |prompt| {
			TextWidget::new(prompt, message).frame()
		})
	}

	fn typed(session: &mut Session<TextState>, s: &str) -> String {
		s.chars()
			.map(|c| session.key(Some(&c.to_string()), &Key::named(KeyName::Char(c))))
			.collect()
	}

	fn press(session: &mut Session<TextState>, name: KeyName) -> String {
		session.key(None, &Key::named(name))
	}

	/// A Frame with no styling in it, so that a test about sequencing is not also a test about SGR.
	fn plain(lines: &'static [&'static str]) -> impl Fn(&Prompt<TextState>) -> Frame {
		move |_| {
			let mut frame = Frame::new();
			for line in lines {
				frame.push(Line::from(Span::raw(*line)));
			}
			frame
		}
	}

	#[test]
	fn a_session_opens_by_hiding_the_cursor_and_writing_the_whole_frame() {
		let mut session = Session::new(Prompt::new(TextState::new()), plain(&["a", "b"]));
		assert_eq!(session.open(), format!("{HIDE}a\nb"));
	}

	#[test]
	fn the_opening_frame_is_written_once_however_often_it_is_asked_for() {
		let mut session = Session::new(Prompt::new(TextState::new()), plain(&["a"]));
		assert_eq!(session.open(), format!("{HIDE}a"));
		assert_eq!(session.open(), "");
	}

	/// The shape every harvested Fixture has: hide, Frame, diffs, newline, show.
	#[test]
	fn a_settled_session_ends_with_a_newline_and_then_the_cursor() {
		let mut session = text("foo");
		let mut stream = session.open();
		stream.push_str(&typed(&mut session, "Jan"));
		stream.push_str(&press(&mut session, KeyName::Return));

		assert!(
			stream.starts_with(HIDE),
			"the stream does not open by hiding"
		);
		assert!(
			stream.ends_with(&format!("\n{SHOW}")),
			"the stream does not end with a newline and then the cursor"
		);
		assert_eq!(session.status(), Status::Submit);
		assert!(session.is_finished());
	}

	/// Upstream removes its keypress listener in `close()`, so there is no such thing as a key
	/// after the last one. A Session that kept accepting them would write past the newline.
	#[test]
	fn nothing_reaches_the_prompt_once_it_has_closed() {
		let mut session = text("foo");
		session.open();
		press(&mut session, KeyName::Return);
		assert!(session.is_finished());

		assert_eq!(typed(&mut session, "late"), "");
		assert_eq!(session.resize(20, 5), "");
		assert_eq!(session.prompt().user_input(), "");
	}

	#[test]
	fn a_cancelled_session_closes_the_same_way_a_submitted_one_does() {
		let mut session = text("foo");
		let mut stream = session.open();
		stream.push_str(&press(&mut session, KeyName::Escape));

		assert!(stream.ends_with(&format!("\n{SHOW}")));
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
	}

	/// The render happens before the close, so the settled Frame is on screen when the newline
	/// lands. If the order were the other way round the answer would be written below the Prompt.
	#[test]
	fn the_settled_frame_is_written_before_the_newline() {
		let mut session = text("foo");
		session.open();
		typed(&mut session, "Jan");
		let last = press(&mut session, KeyName::Return);

		// A Frame carries newlines of its own, so the close is identified by what it is — the
		// newline immediately before the cursor is shown — rather than by the first `\n` in sight.
		let body = last
			.strip_suffix(&format!("\n{SHOW}"))
			.expect("the write ends with the close");
		assert!(
			body.contains("Jan"),
			"the settled value was not written before the newline: {last:?}"
		);
	}

	#[test]
	fn a_resize_writes_a_frame_without_touching_the_prompt() {
		let mut session = text("What is your name?");
		session.open();
		typed(&mut session, "Jan");

		let before = session.prompt().status();
		let written = session.resize(12, 20);

		assert_eq!(session.columns(), 12);
		assert_eq!(session.status(), before);
		assert_eq!(session.prompt().user_input(), "Jan");
		assert!(
			!written.is_empty(),
			"narrowing the terminal wrapped nothing"
		);
	}

	/// A resize that changes nothing about the layout is a render like any other, and an identical
	/// Frame writes nothing at all.
	#[test]
	fn a_resize_that_changes_no_row_writes_nothing() {
		let mut session = text("foo");
		session.open();
		assert_eq!(session.resize(80, 20), "");
	}

	#[test]
	fn the_answer_survives_into_the_prompt_the_session_gives_back() {
		let mut session = text("foo");
		session.open();
		typed(&mut session, "Jan");
		press(&mut session, KeyName::Return);

		let prompt = session.into_prompt();
		assert!(matches!(prompt.outcome(), Some(Outcome::Submitted(Some(v))) if v == "Jan"));
	}
}
