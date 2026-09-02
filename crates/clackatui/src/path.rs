//! `path()`, ported from `@clack/prompts`' `path.ts`.
//!
//! An [`autocomplete`](crate::autocomplete) whose list is the filesystem: type a path, and the
//! directory the text names is read again after every keystroke. Nothing is filtered in the
//! terminal — what narrows the list is `readdirSync` and a prefix test, which is
//! [`clackatui_core::path::options`].
//!
//! # This is the crate that touches a disk
//!
//! `clackatui-core` performs no I/O, so the reading is here: [`StdFs`] answers
//! [`Fs`](clackatui_core::path::Fs) with `std::fs`, and is the only thing in the port that opens a
//! directory. A caller with a filesystem of their own — a virtual one, a remote one, a fake in a
//! test — passes it to [`Path::fs`] and gets the same Prompt over it.

use std::path::Path as StdPath;

use clackatui_core::autocomplete::{AutocompleteState, AutocompleteWidget};
use clackatui_core::path::{Fs, options, required};
use clackatui_core::prompt::{Outcome, Prompt, Validator};
use clackatui_core::session::Session;
use clackatui_core::settings::Settings;
use clackatui_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// The real filesystem, as `path.ts`'s `node:fs` import.
///
/// `lstat` rather than `stat`, as upstream's is: a symlink to a directory is not a directory here,
/// so it is offered as a leaf and typing a `/` after it is what walks through it.
pub struct StdFs;

impl Fs for StdFs {
	fn exists(&self, path: &str) -> bool {
		// `existsSync` follows nothing and reports on the link itself, which `symlink_metadata` is.
		StdPath::new(path).symlink_metadata().is_ok()
	}

	fn is_dir(&self, path: &str) -> bool {
		StdPath::new(path)
			.symlink_metadata()
			.is_ok_and(|meta| meta.is_dir())
	}

	fn read_dir(&self, path: &str) -> Option<Vec<String>> {
		let entries = std::fs::read_dir(path).ok()?;
		Some(
			entries
				.filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
				.collect(),
		)
	}
}

/// A `path` Prompt, waiting to be configured and run.
///
/// ```no_run
/// let file = clackatui::path("Select a file").root("/tmp").interact()?;
/// # Ok::<_, clackatui::ClackError>(())
/// ```
pub struct Path {
	message: String,
	root: Option<String>,
	initial_value: Option<String>,
	directory: bool,
	fs: Box<dyn Fs>,
	validator: Option<Box<dyn Validator<String>>>,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
}

/// Choose a file or a directory, by typing a path with the list keeping up.
pub fn path(message: impl Into<String>) -> Path {
	Path {
		message: message.into(),
		root: None,
		initial_value: None,
		directory: false,
		fs: Box::new(StdFs),
		validator: None,
		theme: None,
		settings: None,
		with_guide: None,
	}
}

impl Path {
	/// `root`: where the field starts. Not a boundary — a suggestion list is whatever the text names,
	/// and the text can be edited to anywhere. The working directory without one.
	pub fn root(mut self, root: impl Into<String>) -> Self {
		self.root = Some(root.into());
		self
	}

	/// `initialValue`: the path the field opens with, which wins over [`root`](Self::root).
	pub fn initial_value(mut self, value: impl Into<String>) -> Self {
		self.initial_value = Some(value.into());
		self
	}

	/// `directory`: suggest only directories.
	///
	/// It changes what a bare `return` answers with, too. A directory typed *without* a trailing
	/// slash lists its siblings rather than its children, so `return` answers with the directory
	/// itself; type the slash and the children appear.
	pub fn directory(mut self, directory: bool) -> Self {
		self.directory = directory;
		self
	}

	/// The filesystem to read. [`StdFs`] unless something else is given.
	pub fn fs(mut self, fs: impl Fs + 'static) -> Self {
		self.fs = Box::new(fs);
		self
	}

	/// `validate`: refuse an answer, with a message. It runs after `path`'s own, which refuses an
	/// empty one.
	pub fn validate(mut self, validator: impl Validator<String> + 'static) -> Self {
		self.validator = Some(Box::new(validator));
		self
	}

	pub fn theme(mut self, theme: Theme) -> Self {
		self.theme = Some(theme);
		self
	}

	pub fn settings(mut self, settings: Settings) -> Self {
		self.settings = Some(settings);
		self
	}

	/// Whether the Guide — the bar down the left margin — is drawn beside this Prompt.
	pub fn with_guide(mut self, with_guide: bool) -> Self {
		self.with_guide = Some(with_guide);
		self
	}

	/// Ask, and bubble a cancel as [`ClackError::Cancelled`].
	pub fn interact(self) -> Result<String, ClackError> {
		self.interact_opt()?.ok_or(ClackError::Cancelled)
	}

	/// Ask, and report a cancel as [`None`].
	pub fn interact_opt(self) -> Result<Option<String>, ClackError> {
		let prompt = driver::run(self.session())?;
		Ok(match prompt.outcome() {
			Some(Outcome::Submitted(Some(values))) => values.first().cloned(),
			_ => None,
		})
	}

	/// The Session this Prompt would be run as, without running it.
	pub fn session(self) -> Session<AutocompleteState<String>> {
		let (fs, directory) = (self.fs, self.directory);
		let state =
			AutocompleteState::with_options_fn(move |input| options(fs.as_ref(), input, directory));

		let mut prompt = Prompt::new(state);
		if let Some(settings) = self.settings {
			prompt = prompt.with_settings(settings);
		}
		// `path`'s own validator runs whether or not the caller passes one, and the caller's runs
		// inside it — upstream's `validate(value)` is one function with the empty check in front.
		match self.validator {
			Some(mut validator) => {
				prompt = prompt.with_validator(move |values: Option<&Vec<String>>| {
					required(values).or_else(|| validator.validate(values?.first()))
				});
			}
			None => prompt = prompt.with_validator(required),
		}
		// `initialUserInput: opts.initialValue ?? opts.root ?? process.cwd()`.
		let initial =
			self.initial_value
				.or(self.root)
				.unwrap_or_else(|| match std::env::current_dir() {
					Ok(cwd) => cwd.to_string_lossy().into_owned(),
					Err(_) => String::new(),
				});
		prompt = prompt.with_initial_user_input(initial);

		let message = self.message;
		let theme = self.theme.unwrap_or_else(Theme::clack);
		let with_guide = self.with_guide;

		Session::new(prompt, move |prompt, columns, rows| {
			// `maxItems: 5`, which `path` sets and does not offer.
			let mut widget = AutocompleteWidget::new(prompt, &message)
				.with_theme(&theme)
				.with_columns(columns as usize)
				.with_rows(rows as usize)
				.with_max_items(5);
			if let Some(with_guide) = with_guide {
				widget = widget.with_guide(with_guide);
			}
			widget.frame()
		})
	}
}
