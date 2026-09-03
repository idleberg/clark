//! The interval a [`spinner`](crate::spinner) and a [`progress`](crate::progress) bar share.
//!
//! `clark-core` takes the elapsed time as an argument and hands back bytes; the thread that
//! decides when to ask is here. Both renderers want exactly the same one — a loop that sleeps for
//! `delay`, a lock the caller and that loop share, and an ending that stops the loop before it takes
//! the lock — so there is one of it, generic over what it is ticking.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use clark_core::prompt::Status;

/// `CommonOptions.output`: the stream a renderer draws on.
///
/// Upstream takes a `Writable` and defaults it to `process.stdout`. A terminal program has two
/// streams and a caller has never passed anything but one of them, so this is an enum rather than a
/// generic writer — a generic would reach into [`Ticker`]'s `Arc<Mutex<T>>` and through every
/// builder that holds one, for a knob with two settings.
///
/// A Prompt does not take one: its drawing goes to stderr and its answer to stdout, which is
/// [`crate::driver`]'s decision to make rather than the caller's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Output {
	/// Upstream's default.
	#[default]
	Stdout,
	/// Where a renderer draws when its stdout belongs to something else — a wrapped command's
	/// output, or an answer a shell is capturing.
	Stderr,
}

impl Output {
	/// Write bytes, dropping a failure — the same trade [`crate::message`] makes.
	fn write(self, bytes: &str) {
		match self {
			Self::Stdout => Self::put(&mut std::io::stdout(), bytes),
			Self::Stderr => Self::put(&mut std::io::stderr(), bytes),
		}
	}

	fn put(out: &mut impl Write, bytes: &str) {
		let _ = out.write_all(bytes.as_bytes());
		let _ = out.flush();
	}

	/// `isTTY(output)`, asked of the stream actually being drawn on.
	pub(crate) fn is_terminal(self) -> bool {
		match self {
			Self::Stdout => std::io::stdout().is_terminal(),
			Self::Stderr => std::io::stderr().is_terminal(),
		}
	}
}

/// What a [`Ticker`] can drive: the three calls that write bytes on a clock.
pub(crate) trait Tick: Send + 'static {
	fn tick(&mut self, elapsed: Duration) -> String;
	fn stop(&mut self, message: &str, status: Status, elapsed: Duration) -> String;
	fn clear(&mut self) -> String;
}

pub(crate) struct Ticker<T: Tick> {
	inner: Arc<Mutex<T>>,
	running: Arc<AtomicBool>,
	handle: Option<JoinHandle<()>>,
	origin: Instant,
	output: Output,
}

impl<T: Tick> Ticker<T> {
	/// Starts the interval. Whatever `start` wrote is the caller's to print first — the clock begins
	/// here, and the row it draws is the first one after it.
	pub(crate) fn start(inner: T, delay: Duration, output: Output) -> Self {
		let inner = Arc::new(Mutex::new(inner));
		let running = Arc::new(AtomicBool::new(true));
		let origin = Instant::now();

		let handle = {
			let inner = Arc::clone(&inner);
			let running = Arc::clone(&running);
			std::thread::spawn(move || {
				while running.load(Ordering::Relaxed) {
					std::thread::sleep(delay);
					if !running.load(Ordering::Relaxed) {
						break;
					}
					// A poisoned lock means an ending panicked while holding it; there is nothing
					// useful left to draw, so the interval stops rather than panicking in a thread
					// nobody is joining yet.
					let Ok(mut inner) = inner.lock() else { break };
					print(output, &inner.tick(origin.elapsed()));
				}
			})
		};

		Self {
			inner,
			running,
			handle: Some(handle),
			origin,
			output,
		}
	}

	/// Runs `change` against the state the interval draws from. Writes nothing by itself.
	pub(crate) fn with(&self, change: impl FnOnce(&mut T)) {
		if let Ok(mut inner) = self.inner.lock() {
			change(&mut inner);
		}
	}

	/// The interval is stopped before the lock is taken, so the closing row cannot be drawn over by
	/// a tick that was already on its way. `None` is a `clear`.
	pub(crate) fn end(mut self, message: &str, status: Option<Status>) {
		self.running.store(false, Ordering::Relaxed);
		if let Some(handle) = self.handle.take() {
			let _ = handle.join();
		}
		let origin = self.origin;
		let output = self.output;
		self.with(|inner| {
			print(
				output,
				&match status {
					Some(status) => inner.stop(message, status, origin.elapsed()),
					None => inner.clear(),
				},
			);
		});
	}
}

impl<T: Tick> Drop for Ticker<T> {
	/// Dropped without an ending, the row is cleared rather than stopped.
	///
	/// Nothing here can know what the ending should have said, and leaving the thread running and
	/// the cursor hidden is the one outcome that is certainly wrong. [`end`](Self::end) has already
	/// taken the handle by the time this runs on the ordinary paths, so this is only the forgotten
	/// one.
	fn drop(&mut self) {
		self.running.store(false, Ordering::Relaxed);
		if let Some(handle) = self.handle.take() {
			let _ = handle.join();
			let output = self.output;
			self.with(|inner| print(output, &inner.clear()));
		}
	}
}

/// Print bytes to a renderer's [`Output`], dropping a failure.
pub(crate) fn print(output: Output, bytes: &str) {
	if bytes.is_empty() {
		return;
	}
	output.write(bytes);
}
