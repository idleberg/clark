//! A progress bar on a real terminal — a [`spinner`](crate::spinner) whose message is a bar.
//!
//! ```no_run
//! let bar = clark::progress().max(3).start("Copying");
//! bar.advance(1, None);
//! bar.stop("Copied");
//! ```
//!
//! The interval, the thread and the lock are [`crate::ticker`]'s, shared with the spinner. What is
//! here is the builder and the one call a spinner does not have.

use std::time::Duration;

use clark_core::progress::{self as core_progress, BarStyle};
use clark_core::prompt::Status;
use clark_core::settings::Settings;
use clark_core::spinner::{self as core_spinner, Indicator, StyleFrame};
use clark_core::theme::Theme;

use crate::spinner::{columns, is_ci, resolve};
use crate::ticker::{Output, Tick, Ticker, print};

impl Tick for core_progress::Progress<'static> {
	fn tick(&mut self, elapsed: Duration) -> String {
		core_progress::Progress::tick(self, elapsed)
	}

	fn stop(&mut self, message: &str, status: Status, elapsed: Duration) -> String {
		core_progress::Progress::stop(self, message, status, elapsed)
	}

	fn clear(&mut self) -> String {
		core_progress::Progress::clear(self)
	}
}

/// `progress()`: a bar that has not started yet.
pub fn progress() -> Builder {
	Builder {
		theme: Theme::detect(),
		delay: None,
		output: Output::default(),
		accessible: None,
		settings: None,
		options: core_progress::Options {
			spinner: core_spinner::Options {
				ci: is_ci(),
				..core_spinner::Options::default()
			},
			..core_progress::Options::default()
		},
	}
}

pub struct Builder {
	theme: Theme,
	delay: Option<Duration>,
	output: Output,
	accessible: Option<bool>,
	settings: Option<Settings>,
	options: core_progress::Options<'static>,
}

impl Builder {
	/// Which character the bar is drawn out of. Heavy by default.
	pub fn style(mut self, style: BarStyle) -> Self {
		self.options.style = style;
		self
	}

	/// The value that fills the bar. 100 by default, and never less than 1.
	pub fn max(mut self, max: usize) -> Self {
		self.options.max = max;
		self
	}

	/// The bar's width in columns. 40 by default, and never less than 1.
	pub fn size(mut self, size: usize) -> Self {
		self.options.size = size;
		self
	}

	// --- the spinner underneath ------------------------------------------------------------------

	pub fn indicator(mut self, indicator: Indicator) -> Self {
		self.options.spinner.indicator = indicator;
		self
	}

	pub fn frames(mut self, frames: Vec<String>) -> Self {
		self.options.spinner.frames = Some(frames);
		self
	}

	pub fn delay(mut self, delay: Duration) -> Self {
		self.delay = Some(delay);
		self
	}

	pub fn with_guide(mut self, with_guide: bool) -> Self {
		self.options.spinner.with_guide = with_guide;
		self
	}

	pub fn style_frame(mut self, style_frame: StyleFrame<'static>) -> Self {
		self.options.spinner.style_frame = style_frame;
		self
	}

	pub fn theme(mut self, theme: Theme) -> Self {
		self.theme = theme;
		self
	}

	pub fn ci(mut self, ci: bool) -> Self {
		self.options.spinner.ci = ci;
		self
	}

	/// The stream drawn on. Upstream's `output`, whose default is stdout.
	pub fn output(mut self, output: Output) -> Self {
		self.output = output;
		self
	}

	/// Plain lines and a 30-second heartbeat instead of a redrawn bar. Left unset,
	/// [`settings`](Self::settings) decides, then `$ACCESSIBLE`.
	pub fn accessible(mut self, accessible: bool) -> Self {
		self.accessible = Some(accessible);
		self
	}

	pub fn settings(mut self, settings: Settings) -> Self {
		self.settings = Some(settings);
		self
	}

	/// Start at nothing. The interval runs until one of [`Progress`]'s endings, or until it is
	/// dropped.
	pub fn start(mut self, message: impl AsRef<str>) -> Progress {
		let delay = resolve(
			&mut self.options.spinner,
			self.accessible,
			self.settings,
			self.delay,
		);
		let output = self.output;
		let mut inner = core_progress::Progress::new(self.theme, columns(), self.options);
		print(output, &inner.start(message.as_ref()));
		Progress(Ticker::start(inner, delay, output))
	}
}

/// A running progress bar.
pub struct Progress(Ticker<core_progress::Progress<'static>>);

impl Progress {
	/// Move the bar on by `step`, and set the message if one is given. Draws nothing by itself: the
	/// bar reaches the terminal on the next tick, as the spinner's message does.
	pub fn advance(&self, step: usize, message: Option<&str>) {
		self.0.with(|progress| progress.advance(step, message));
	}

	/// A new message, and no movement. Upstream's `message` is `advance(0, msg)`.
	pub fn message(&self, message: impl AsRef<str>) {
		self.0
			.with(|progress| progress.set_message(message.as_ref()));
	}

	/// Stop, leaving a green `◇` and the message — and no bar.
	pub fn stop(self, message: impl AsRef<str>) {
		self.0.end(message.as_ref(), Some(Status::Submit));
	}

	pub fn cancel(self, message: impl AsRef<str>) {
		self.0.end(message.as_ref(), Some(Status::Cancel));
	}

	pub fn error(self, message: impl AsRef<str>) {
		self.0.end(message.as_ref(), Some(Status::Error));
	}

	/// Stop and erase the bar, leaving only the Guide `start` wrote.
	pub fn clear(self) {
		self.0.end("", None);
	}
}
