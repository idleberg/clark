//! `log`, `intro`, `outro` and `cancel` — the output that is not a Prompt.
//!
//! ```no_run
//! clackatui::intro("create-app");
//! clackatui::log::success("dependencies installed");
//! clackatui::outro("You're all set!");
//! ```
//!
//! Everything here writes to stdout once and returns nothing, as upstream's do. A failed write is
//! dropped rather than reported: these are called for their side effect in the middle of a program
//! that has nothing useful to do about a broken pipe, and a `Result` on every log line is a `Result`
//! nobody reads. A Prompt is the other way round — [`driver`](crate::driver) reports every failure,
//! because a Prompt that cannot draw cannot be answered either.
//!
//! No raw mode and no `\r\n`: unlike the driver, these run with the terminal's output discipline
//! intact, which is the one that puts the carriage return back by itself.

use std::io::Write;

use clackatui_core::emitter::write_once;
use clackatui_core::frame::Span;
use clackatui_core::message;
use clackatui_core::note as core_note;
use clackatui_core::theme::Theme;

/// Print bytes to stdout, dropping a failure. See the module docs.
fn print(bytes: &str) {
	let mut out = std::io::stdout();
	let _ = out.write_all(bytes.as_bytes());
	let _ = out.flush();
}

/// The opening line of an interaction.
pub fn intro(title: impl AsRef<str>) {
	print(&write_once(&message::intro(
		title.as_ref(),
		&Theme::clack(),
		true,
	)));
}

/// [`intro`] in another Theme, or without the Guide.
pub fn intro_with(title: impl AsRef<str>, theme: &Theme, with_guide: bool) {
	print(&write_once(&message::intro(
		title.as_ref(),
		theme,
		with_guide,
	)));
}

/// The closing line of an interaction, followed by a blank one.
pub fn outro(message: impl AsRef<str>) {
	print(&write_once(&message::outro(
		message.as_ref(),
		&Theme::clack(),
		true,
	)));
}

/// [`outro`] in another Theme, or without the Guide.
pub fn outro_with(message: impl AsRef<str>, theme: &Theme, with_guide: bool) {
	print(&write_once(&message::outro(
		message.as_ref(),
		theme,
		with_guide,
	)));
}

/// An interaction that ended early — usually what follows [`ClackError::Cancelled`].
///
/// [`ClackError::Cancelled`]: crate::ClackError::Cancelled
pub fn cancel(message: impl AsRef<str>) {
	print(&write_once(&message::cancel(
		message.as_ref(),
		&Theme::clack(),
		true,
	)));
}

/// [`cancel`] in another Theme, or without the Guide.
pub fn cancel_with(message: impl AsRef<str>, theme: &Theme, with_guide: bool) {
	print(&write_once(&message::cancel(
		message.as_ref(),
		theme,
		with_guide,
	)));
}

/// A message in a box, under a title.
///
/// The only renderer here that reads the terminal's width, because it draws a right-hand border.
/// Eighty columns when there is no terminal to ask, which is upstream's fallback too.
///
/// ```no_run
/// clackatui::note("npm install\nnpm run dev", "Next steps");
/// ```
pub fn note(message: impl AsRef<str>, title: impl AsRef<str>) {
	print(&write_once(&core_note::note(
		message.as_ref(),
		title.as_ref(),
		columns(),
		&Theme::clack(),
		true,
	)));
}

/// [`note`], with a formatter for each row of the message, another Theme, or no Guide.
///
/// The formatter returns a drawn [`Line`](clackatui_core::frame::Line) where upstream's returns a
/// string, so it can add colour as well as characters — see [`clackatui_core::note`].
pub fn note_with(
	message: impl AsRef<str>,
	title: impl AsRef<str>,
	theme: &Theme,
	with_guide: bool,
	format: core_note::Format<'_>,
) {
	print(&write_once(&core_note::note_with(
		message.as_ref(),
		title.as_ref(),
		columns(),
		theme,
		with_guide,
		format,
	)));
}

/// The terminal's width, or upstream's eighty when there is no terminal.
fn columns() -> usize {
	crossterm::terminal::size().map_or(80, |(columns, _)| columns as usize)
}

/// Messages between Prompts, each under a symbol of its own.
pub mod log {
	use super::*;

	/// A `log` line, configured. [`message`] and its five named symbols are this with the defaults.
	///
	/// ```no_run
	/// use clackatui::log::Log;
	/// # let theme = clackatui::Theme::clack();
	/// Log::new(&theme).spacing(0).with_guide(false).print("no bar, no gap");
	/// ```
	pub struct Log {
		symbol: Span,
		secondary_symbol: Span,
		spacing: usize,
		with_guide: bool,
	}

	impl Log {
		/// A `log.message`: the Guide's own bar as both symbols, one blank row above.
		pub fn new(theme: &Theme) -> Self {
			let bar = Span::styled(theme.symbols.bar, theme.styles.guide);
			Self {
				symbol: bar.clone(),
				secondary_symbol: bar,
				spacing: 1,
				with_guide: true,
			}
		}

		/// The symbol beside the first row. Upstream's is a string, so this is a whole drawn cell
		/// rather than a colour — `log.message('…', { symbol: '>>' })` is a supported call there.
		pub fn symbol(mut self, symbol: Span) -> Self {
			self.symbol = symbol;
			self
		}

		/// The symbol beside every row after the first, and above the message. Defaults to the bar.
		pub fn secondary_symbol(mut self, symbol: Span) -> Self {
			self.secondary_symbol = symbol;
			self
		}

		/// How many rows of the secondary symbol to write above the message. One by default.
		pub fn spacing(mut self, spacing: usize) -> Self {
			self.spacing = spacing;
			self
		}

		pub fn with_guide(mut self, with_guide: bool) -> Self {
			self.with_guide = with_guide;
			self
		}

		/// The bytes this would write.
		pub fn render(&self, message: impl AsRef<str>) -> String {
			write_once(&message::log(
				message.as_ref(),
				self.symbol.clone(),
				self.secondary_symbol.clone(),
				self.spacing,
				self.with_guide,
			))
		}

		/// Write it.
		pub fn print(&self, message: impl AsRef<str>) {
			super::print(&self.render(message));
		}
	}

	/// A line beside the Guide's bar, with no symbol of its own.
	pub fn message(text: impl AsRef<str>) {
		Log::new(&Theme::clack()).print(text);
	}

	/// A line under a blue `●`.
	pub fn info(text: impl AsRef<str>) {
		symbolled(text, |theme| {
			Span::styled(theme.symbols.info, theme.styles.log_info)
		});
	}

	/// A line under a green `◆`.
	pub fn success(text: impl AsRef<str>) {
		symbolled(text, |theme| {
			Span::styled(theme.symbols.success, theme.styles.log_success)
		});
	}

	/// A line under a green `◇` — the symbol a settled Prompt leaves behind, for a step that was
	/// not one.
	pub fn step(text: impl AsRef<str>) {
		symbolled(text, |theme| {
			Span::styled(theme.symbols.step_submit, theme.styles.log_step)
		});
	}

	/// A line under a yellow `▲`.
	pub fn warn(text: impl AsRef<str>) {
		symbolled(text, |theme| {
			Span::styled(theme.symbols.warn, theme.styles.log_warn)
		});
	}

	/// A line under a red `■`.
	pub fn error(text: impl AsRef<str>) {
		symbolled(text, |theme| {
			Span::styled(theme.symbols.error, theme.styles.log_error)
		});
	}

	fn symbolled(text: impl AsRef<str>, symbol: impl Fn(&Theme) -> Span) {
		let theme = Theme::clack();
		Log::new(&theme).symbol(symbol(&theme)).print(text);
	}
}

#[cfg(test)]
mod tests {
	use super::log::Log;
	use super::*;

	#[test]
	fn the_defaults_are_a_bar_a_gap_and_two_spaces() {
		let theme = Theme::clack();
		assert!(
			Log::new(&theme)
				.render("hello")
				.ends_with("│\u{1b}[0m  hello\n")
		);
	}

	#[test]
	fn a_symbol_replaces_only_the_first_rows_marker() {
		let theme = Theme::clack();
		let rendered = Log::new(&theme).symbol(Span::raw(">>")).render("one\ntwo");
		let rows: Vec<&str> = rendered.lines().collect();
		assert!(rows[1].contains(">>  one"), "{rows:?}");
		assert!(rows[2].contains("│\u{1b}[0m  two"), "{rows:?}");
	}
}
