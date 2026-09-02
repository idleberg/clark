//! A task log on a real terminal: rows that clear when the task succeeds and stay when it fails.
//!
//! ```no_run
//! let mut log = clackatui::task_log("Installing").limit(5).start();
//! log.message("resolving…");
//! log.message("fetching…");
//! log.success("Installed");
//! ```
//!
//! No thread and no interval — a task log draws only when it is called, so [`crate::ticker`] has
//! nothing to do here. What this adds to [`clackatui_core::task_log`] is the terminal: the width,
//! and whether there is one at all.

use std::io::IsTerminal;

use clackatui_core::task_log::{self as core_task_log, GroupId, Outcome};
use clackatui_core::theme::Theme;

use crate::spinner::{columns, is_ci};
use crate::ticker::print;

/// `taskLog()`: a log that has not written its title yet.
pub fn task_log(title: impl AsRef<str>) -> Builder {
	Builder {
		theme: Theme::detect(),
		options: core_task_log::Options {
			title: title.as_ref().to_owned(),
			// `!isCI() && isTTY(output)`, read once, exactly where upstream reads it.
			is_tty: !is_ci() && std::io::stdout().is_terminal(),
			..core_task_log::Options::default()
		},
	}
}

pub struct Builder {
	theme: Theme,
	options: core_task_log::Options,
}

impl Builder {
	/// The most rows kept. Older ones are dropped, or retained by [`retain_log`](Self::retain_log).
	pub fn limit(mut self, limit: usize) -> Self {
		self.options.limit = Some(limit);
		self
	}

	/// Rows above the title and above each printed message. One by default.
	pub fn spacing(mut self, spacing: usize) -> Self {
		self.options.spacing = spacing;
		self
	}

	/// Keep the rows `limit` drops, and print them with the rest when the log is shown.
	pub fn retain_log(mut self, retain_log: bool) -> Self {
		self.options.retain_log = retain_log;
		self
	}

	/// The Guide's bar beside the messages. Upstream takes a `withGuide` here and never reads it —
	/// its messages follow the global setting — so this is the knob that works. The title keeps its
	/// bar either way, as upstream's does.
	pub fn with_guide(mut self, with_guide: bool) -> Self {
		self.options.with_guide = with_guide;
		self
	}

	pub fn theme(mut self, theme: Theme) -> Self {
		self.theme = theme;
		self
	}

	/// Draw nothing between the title and the ending, as a log with no terminal does.
	pub fn quiet(mut self, quiet: bool) -> Self {
		self.options.is_tty = !quiet;
		self
	}

	/// Write the title. Everything after it is a call away.
	pub fn start(self) -> TaskLog {
		let (log, bytes) = core_task_log::TaskLog::new(self.theme, columns(), self.options);
		print(&bytes);
		TaskLog(log)
	}
}

/// A group of rows under a name of its own.
pub struct Group(GroupId);

/// A running task log.
pub struct TaskLog(core_task_log::TaskLog);

impl TaskLog {
	/// A row.
	pub fn message(&mut self, message: impl AsRef<str>) {
		print(&self.0.message(message.as_ref(), false));
	}

	/// A row that continues the one before it, if that one was raw too — upstream's
	/// `{ raw: true }`, for output that arrives in pieces.
	pub fn raw(&mut self, message: impl AsRef<str>) {
		print(&self.0.message(message.as_ref(), true));
	}

	/// A named group. Its rows are printed under its name until it ends.
	pub fn group(&mut self, name: impl AsRef<str>) -> Group {
		Group(self.0.group(name.as_ref()))
	}

	pub fn group_message(&mut self, group: &Group, message: impl AsRef<str>) {
		print(&self.0.group_message(group.0, message.as_ref(), false));
	}

	pub fn group_raw(&mut self, group: &Group, message: impl AsRef<str>) {
		print(&self.0.group_message(group.0, message.as_ref(), true));
	}

	/// End a group: its rows become one green line.
	pub fn group_success(&mut self, group: &Group, message: impl AsRef<str>) {
		print(
			&self
				.0
				.complete_group(group.0, Outcome::Success, message.as_ref()),
		);
	}

	/// End a group: its rows become one red line.
	pub fn group_error(&mut self, group: &Group, message: impl AsRef<str>) {
		print(
			&self
				.0
				.complete_group(group.0, Outcome::Error, message.as_ref()),
		);
	}

	/// The task worked. The log is erased — this is the whole point of the module.
	pub fn success(&mut self, message: impl AsRef<str>) {
		self.success_with(message, false);
	}

	/// The task failed. The log is kept, under the message.
	pub fn error(&mut self, message: impl AsRef<str>) {
		self.error_with(message, true);
	}

	/// [`success`](Self::success), saying whether to keep the log. Upstream's default is `false`.
	pub fn success_with(&mut self, message: impl AsRef<str>, show_log: bool) {
		print(&self.0.success(message.as_ref(), show_log));
	}

	/// [`error`](Self::error), saying whether to keep the log. Upstream's default is `true`.
	pub fn error_with(&mut self, message: impl AsRef<str>, show_log: bool) {
		print(&self.0.error(message.as_ref(), show_log));
	}
}
