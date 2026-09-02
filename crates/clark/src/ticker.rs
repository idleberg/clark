//! The interval a [`spinner`](crate::spinner) and a [`progress`](crate::progress) bar share.
//!
//! `clark-core` takes the elapsed time as an argument and hands back bytes; the thread that
//! decides when to ask is here. Both renderers want exactly the same one — a loop that sleeps for
//! `delay`, a lock the caller and that loop share, and an ending that stops the loop before it takes
//! the lock — so there is one of it, generic over what it is ticking.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use clark_core::prompt::Status;

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
}

impl<T: Tick> Ticker<T> {
	/// Starts the interval. Whatever `start` wrote is the caller's to print first — the clock begins
	/// here, and the row it draws is the first one after it.
	pub(crate) fn start(inner: T, delay: Duration) -> Self {
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
					print(&inner.tick(origin.elapsed()));
				}
			})
		};

		Self {
			inner,
			running,
			handle: Some(handle),
			origin,
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
		self.with(|inner| {
			print(&match status {
				Some(status) => inner.stop(message, status, origin.elapsed()),
				None => inner.clear(),
			});
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
			self.with(|inner| print(&inner.clear()));
		}
	}
}

/// Print bytes to stdout, dropping a failure — the same trade [`crate::message`] makes.
pub(crate) fn print(bytes: &str) {
	if bytes.is_empty() {
		return;
	}
	let mut out = std::io::stdout();
	let _ = out.write_all(bytes.as_bytes());
	let _ = out.flush();
}
