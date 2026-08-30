//! The blocking driver: a [`Session`] on a real terminal.
//!
//! Ported from what is left of `@clack/core`'s `Prompt.prompt` once the sequencing has been taken
//! out of it (that is [`Session`]'s, and is where the ordering guarantees live). What remains is
//! genuinely I/O: put the terminal into raw mode, read keys until the Prompt settles, write what
//! the Session produces, and re-render when the window changes size.
//!
//! # Raw mode is given back whatever happens
//!
//! Upstream calls `setRawMode(input, false)` in `close()` and again in each of the `submit` and
//! `cancel` listeners, which between them cover the paths it has. A Rust driver has one it does
//! not: a panic. Raw mode is therefore released by a guard rather than at the end of the loop, so
//! that a panic in a `draw` callback leaves the user with a working shell rather than a terminal
//! that no longer echoes.

use std::io::{self, Write};

use clackatui_core::prompt::{Prompt, PromptState};
use clackatui_core::session::Session;
use crossterm::event::{Event, KeyEventKind};
use crossterm::terminal;

use crate::error::ClackError;
use crate::keys::decode;

/// Run a Session to its outcome and hand back the Prompt that produced it.
///
/// The Prompt rather than the value, because [`Outcome`](clackatui_core::prompt::Outcome) borrows
/// what it reports and a caller usually wants to own it. Each Prompt's builder is what turns this
/// into an answer.
pub fn run<S: PromptState>(mut session: Session<S>) -> Result<Prompt<S>, ClackError> {
	let mut out = io::stdout();

	if let Ok((columns, rows)) = terminal::size() {
		session = session.with_size(columns, rows);
	}

	let _raw = RawMode::enter()?;

	let result = pump(&mut out, &mut session);

	// A Session hides the cursor when it opens and shows it again when it closes, which covers
	// every path upstream has. It does not cover the one a Rust driver adds: an I/O failure part
	// way through. Leaving a terminal with no cursor is worse than one redundant escape, so the
	// cursor is restored here when the Session never got to do it itself.
	if result.is_err() && !session.is_finished() {
		let _ = write(&mut out, SHOW_CURSOR);
	}
	result?;

	Ok(session.into_prompt())
}

/// Upstream's `cursor.show`. Written only on the failure path — see [`run`].
const SHOW_CURSOR: &str = "\u{1b}[?25h";

fn pump<S: PromptState>(out: &mut impl Write, session: &mut Session<S>) -> Result<(), ClackError> {
	write(out, &session.open())?;

	while !session.is_finished() {
		match crossterm::event::read()? {
			// Release and repeat events reach a driver only where the terminal speaks a protocol
			// that reports them. readline never sees either, so neither does the Prompt.
			Event::Key(event) if event.kind == KeyEventKind::Press => {
				let (character, key) = decode(event);
				write(out, &session.key(character.as_deref(), &key))?;
			}
			Event::Resize(columns, rows) => {
				write(out, &session.resize(columns, rows))?;
			}
			_ => {}
		}
	}

	Ok(())
}

fn write(out: &mut impl Write, bytes: &str) -> io::Result<()> {
	if bytes.is_empty() {
		return Ok(());
	}
	out.write_all(bytes.as_bytes())?;
	out.flush()
}

/// Raw mode, released on the way out of scope.
struct RawMode;

impl RawMode {
	fn enter() -> io::Result<Self> {
		terminal::enable_raw_mode()?;
		Ok(Self)
	}
}

impl Drop for RawMode {
	fn drop(&mut self) {
		// Nothing useful can be done about a failure here, and panicking during a drop that may
		// itself be unwinding would abort the process.
		let _ = terminal::disable_raw_mode();
	}
}
