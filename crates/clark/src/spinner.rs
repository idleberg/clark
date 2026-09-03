//! The spinner on a real terminal: a thread, a clock, and the bytes [`clark_core::spinner`]
//! produces.
//!
//! ```no_run
//! let spinner = clark::spinner().start("Installing dependencies");
//! // ...
//! spinner.stop("Installed");
//! ```
//!
//! Everything about *what* is drawn is in the core crate and is measured against a recording of
//! clack. What is here is the part that cannot be: an interval that fires every `delay`, and a lock
//! around the state the caller and that interval share — both of which live in [`crate::ticker`],
//! because [`crate::progress`] wants exactly the same ones.
//!
//! # `block()` is not ported
//!
//! Upstream's `start` puts the terminal into raw mode and swallows keypresses, so that typing while
//! a spinner runs leaves no echo behind to be overwritten. Doing that here would mean a reader
//! thread competing with the caller's own stdin for the rest of the program, which is a much larger
//! promise than a spinner should make. So keys typed during a spinner echo, exactly as they would
//! during any other program's output. The cursor still goes away and comes back — that half of
//! `block` is a write, and it is in the core crate with the rest of the writes.

use std::time::Duration;

use clark_core::prompt::Status;
use clark_core::spinner::{self as core_spinner, Indicator, StyleFrame};
use clark_core::theme::{Theme, unicode_supported};

use crate::ticker::{Output, Tick, Ticker, print};

impl Tick for core_spinner::Spinner<'static> {
	fn tick(&mut self, elapsed: Duration) -> String {
		core_spinner::Spinner::tick(self, elapsed)
	}

	fn stop(&mut self, message: &str, status: Status, elapsed: Duration) -> String {
		core_spinner::Spinner::stop(self, message, status, elapsed)
	}

	fn clear(&mut self) -> String {
		core_spinner::Spinner::clear(self)
	}
}

/// `spinner()`: a spinner that has not started yet.
pub fn spinner() -> Builder {
	Builder {
		theme: Theme::detect(),
		delay: None,
		output: Output::default(),
		options: core_spinner::Options {
			ci: is_ci(),
			..core_spinner::Options::default()
		},
	}
}

/// `isCI()`, which upstream reads once when the spinner is made.
pub(crate) fn is_ci() -> bool {
	std::env::var("CI").is_ok_and(|ci| ci == "true")
}

pub struct Builder {
	theme: Theme,
	delay: Option<Duration>,
	output: Output,
	options: core_spinner::Options<'static>,
}

impl Builder {
	/// What follows the message: dots, or the time since [`start`](Self::start).
	pub fn indicator(mut self, indicator: Indicator) -> Self {
		self.options.indicator = indicator;
		self
	}

	/// The symbols cycled through. Defaults to the Theme's.
	pub fn frames(mut self, frames: Vec<String>) -> Self {
		self.options.frames = Some(frames);
		self
	}

	/// How long a frame is on screen. Upstream's default is 80ms, or 120 without Unicode.
	pub fn delay(mut self, delay: Duration) -> Self {
		self.delay = Some(delay);
		self
	}

	pub fn with_guide(mut self, with_guide: bool) -> Self {
		self.options.with_guide = with_guide;
		self
	}

	/// The frame symbol's appearance. Magenta by default; returns a drawn
	/// [`Line`](clark_core::frame::Line), so it can add characters as well as colour.
	pub fn style_frame(mut self, style_frame: StyleFrame<'static>) -> Self {
		self.options.style_frame = style_frame;
		self
	}

	pub fn theme(mut self, theme: Theme) -> Self {
		self.theme = theme;
		self
	}

	/// Draw nothing and write one line per message, the way a CI log wants it. Set from `$CI`.
	pub fn ci(mut self, ci: bool) -> Self {
		self.options.ci = ci;
		self
	}

	/// The stream drawn on. Upstream's `output`, whose default is stdout.
	pub fn output(mut self, output: Output) -> Self {
		self.output = output;
		self
	}

	/// Start spinning. The interval runs until one of [`Spinner`]'s endings, or until it is dropped.
	pub fn start(self, message: impl AsRef<str>) -> Spinner {
		let delay = self.delay.unwrap_or_else(default_delay);
		let output = self.output;
		let mut inner = core_spinner::Spinner::new(self.theme, columns(), self.options);
		print(output, &inner.start(message.as_ref()));
		Spinner(Ticker::start(inner, delay, output))
	}
}

/// Upstream's `delay`: 80ms, or 120 without Unicode.
pub(crate) fn default_delay() -> Duration {
	if unicode_supported() {
		Duration::from_millis(80)
	} else {
		Duration::from_millis(120)
	}
}

/// `getColumns(output)`, with upstream's fallback.
pub(crate) fn columns() -> usize {
	crossterm::terminal::size().map_or(80, |(columns, _)| columns as usize)
}

/// A running spinner. Ends at the first of `stop`, `cancel`, `error`, `clear` or being dropped.
pub struct Spinner(Ticker<core_spinner::Spinner<'static>>);

impl Spinner {
	/// What the next frame will say. Draws nothing by itself, as upstream's does not.
	pub fn message(&self, message: impl AsRef<str>) {
		self.0.with(|spinner| spinner.set_message(message.as_ref()));
	}

	/// Stop, leaving a green `◇` and the message.
	pub fn stop(self, message: impl AsRef<str>) {
		self.0.end(message.as_ref(), Some(Status::Submit));
	}

	/// Stop, leaving a red `■` — the ending for an interaction the user gave up on.
	pub fn cancel(self, message: impl AsRef<str>) {
		self.0.end(message.as_ref(), Some(Status::Cancel));
	}

	/// Stop, leaving a red `▲`.
	pub fn error(self, message: impl AsRef<str>) {
		self.0.end(message.as_ref(), Some(Status::Error));
	}

	/// Stop and erase the row, leaving only the bar `start` wrote. Upstream leaves that bar too, and
	/// has a `TODO` about it.
	pub fn clear(self) {
		self.0.end("", None);
	}
}
