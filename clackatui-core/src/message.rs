//! Ported from `@clack/prompts`' `log.ts` and `messages.ts` — the renderers that are not Prompts.
//!
//! Nothing here has a state machine, reads a key, or renders twice. A caller says something, one
//! Frame is built, and it goes out once — which is the whole difference between this module and the
//! rest of the crate, and the reason [`crate::emitter::write_once`] exists beside [`Emitter`].
//!
//! [`Emitter`]: crate::emitter::Emitter
//!
//! # The trailing newline is the writer's
//!
//! Every one of these ends its write with a `\n` upstream, and two of them end with two. A [`Frame`]
//! here is a list of rows and carries no trailing anything, so the convention is
//! [`crate::emitter::write_once`]'s: rows joined with `\n`, and one `\n` after the last. A Frame
//! that ends with a blank [`Line`] is therefore how a blank line between this output and the next is
//! written down — which is what `outro` and `cancel` both want, and `intro` does not.
//!
//! # `spacing` is rows above, not rows around
//!
//! `log.message` writes `spacing` copies of the *secondary* symbol before anything else, so the
//! default output is a bar and then the message. With the Guide off the same loop pushes the empty
//! string, so the blank rows stay and only the bars go — a `log` with no Guide still opens with a
//! blank line.

use crate::frame::{Frame, Line, Span};
use crate::theme::Theme;

/// `log.message`: a message under a symbol, with the Guide's bar beside every row after the first.
///
/// `symbol` and `secondary_symbol` are Spans rather than colours because upstream's are strings and
/// its own tests pass `--` and `>>` — the option is the whole drawn cell, not a style applied to a
/// bar.
///
/// A row with nothing on it is drawn as the bare symbol, without the two spaces that would follow
/// it — upstream branches on `length > 0` for exactly that, and it is the only place in clack where
/// a prefix is dropped rather than padded.
pub fn log(
	message: &str,
	symbol: Span,
	secondary_symbol: Span,
	spacing: usize,
	with_guide: bool,
) -> Frame {
	let mut frame = Frame::new();

	for _ in 0..spacing {
		frame.push(row(None, &secondary_symbol, with_guide));
	}

	// `message.split('\n')` — never empty, so there is always a first line, and an empty message is
	// one empty row rather than none. Upstream reaches a message of *no* lines only through its
	// `string[]` overload, which has no counterpart here.
	for (index, line) in message.split('\n').enumerate() {
		let prefix = if index == 0 {
			&symbol
		} else {
			&secondary_symbol
		};
		frame.push(row(Some(line), prefix, with_guide));
	}

	frame
}

/// One row of a `log`: the symbol, two spaces, and the text — or as little of that as applies.
fn row(text: Option<&str>, symbol: &Span, with_guide: bool) -> Line {
	match (text.filter(|text| !text.is_empty()), with_guide) {
		(Some(text), true) => Line::from_iter([symbol.clone(), Span::raw("  "), Span::raw(text)]),
		(Some(text), false) => Line::from(Span::raw(text)),
		(None, true) => Line::from(symbol.clone()),
		(None, false) => Line::blank(),
	}
}

/// `intro`: the opening line, under the top of the Guide.
pub fn intro(title: &str, theme: &Theme, with_guide: bool) -> Frame {
	let mut frame = Frame::new();
	frame.push(guided(
		title,
		theme.symbols.bar_start,
		theme,
		with_guide,
		None,
	));
	frame
}

/// `outro`: the closing line, under the bottom of the Guide and a bar of its own.
///
/// The blank row at the end is upstream's second `\n`, which separates this from whatever the
/// program prints next.
pub fn outro(message: &str, theme: &Theme, with_guide: bool) -> Frame {
	let mut frame = Frame::new();
	if with_guide {
		frame.push(Line::from(Span::styled(
			theme.symbols.bar,
			theme.styles.guide,
		)));
	}
	frame.push(guided(
		message,
		theme.symbols.bar_end,
		theme,
		with_guide,
		None,
	));
	frame.push(Line::blank());
	frame
}

/// `cancel`: an interaction that ended early, in red under the bottom of the Guide.
///
/// No leading bar, where `outro` has one — the two are written a few lines apart and simply differ.
pub fn cancel(message: &str, theme: &Theme, with_guide: bool) -> Frame {
	let mut frame = Frame::new();
	frame.push(guided(
		message,
		theme.symbols.bar_end,
		theme,
		with_guide,
		Some(theme.styles.log_cancel),
	));
	frame.push(Line::blank());
	frame
}

/// A message behind a corner of the Guide, or on its own with the Guide off.
///
/// The two spaces are written whether or not there is a message, so an empty `outro` leaves them on
/// the row. Reproduced rather than trimmed, per ADR-0013: a terminal can see trailing space the
/// moment anything selects it.
fn guided(
	message: &str,
	symbol: &'static str,
	theme: &Theme,
	with_guide: bool,
	style: Option<ratatui_core::style::Style>,
) -> Line {
	let text = Span::styled(message, style.unwrap_or_default());
	if with_guide {
		Line::from_iter([
			Span::styled(symbol, theme.styles.guide),
			Span::raw("  "),
			text,
		])
	} else {
		Line::from(text)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::emitter::write_once;

	fn bar(theme: &Theme) -> Span {
		Span::styled(theme.symbols.bar, theme.styles.guide)
	}

	fn logged(message: &str, with_guide: bool) -> String {
		let theme = Theme::clack();
		write_once(&log(message, bar(&theme), bar(&theme), 1, with_guide))
	}

	#[test]
	fn a_message_opens_with_a_bar_and_carries_one_beside_every_row() {
		assert_eq!(
			strip(&logged("line 1\nline 2", true)),
			"│\n│  line 1\n│  line 2\n"
		);
	}

	#[test]
	fn an_empty_row_is_the_bar_alone_and_keeps_no_spaces() {
		assert_eq!(strip(&logged("foo\n\nbar", true)), "│\n│  foo\n│\n│  bar\n");
	}

	/// `log.message('')` is one empty row, not none — so the output is two bars.
	#[test]
	fn an_empty_message_is_still_a_row() {
		assert_eq!(strip(&logged("", true)), "│\n│\n");
	}

	#[test]
	fn turning_the_guide_off_leaves_the_blank_rows_behind() {
		assert_eq!(strip(&logged("foo\n\nbar", false)), "\nfoo\n\nbar\n");
	}

	#[test]
	fn spacing_counts_rows_above_the_message() {
		let theme = Theme::clack();
		let frame = log("spaced", bar(&theme), bar(&theme), 3, true);
		assert_eq!(strip(&write_once(&frame)), "│\n│\n│\n│  spaced\n");
	}

	#[test]
	fn the_three_messages_draw_the_corners_they_do() {
		let theme = Theme::clack();
		assert_eq!(
			strip(&write_once(&intro("hello", &theme, true))),
			"┌  hello\n"
		);
		assert_eq!(
			strip(&write_once(&outro("done", &theme, true))),
			"│\n└  done\n\n"
		);
		assert_eq!(
			strip(&write_once(&cancel("stopped", &theme, true))),
			"└  stopped\n\n"
		);
	}

	/// An empty message still costs the two spaces after the corner.
	#[test]
	fn the_two_spaces_are_written_whether_or_not_there_is_a_message() {
		let theme = Theme::clack();
		assert_eq!(strip(&write_once(&outro("", &theme, true))), "│\n└  \n\n");
	}

	#[test]
	fn with_no_guide_a_message_is_the_message() {
		let theme = Theme::clack();
		assert_eq!(
			strip(&write_once(&intro("hello", &theme, false))),
			"hello\n"
		);
		assert_eq!(
			strip(&write_once(&outro("done", &theme, false))),
			"done\n\n"
		);
	}

	/// The bytes carry SGR; these tests are about the characters.
	fn strip(bytes: &str) -> String {
		let mut out = String::new();
		let mut chars = bytes.chars();
		while let Some(c) = chars.next() {
			if c == '\u{1b}' {
				for c in chars.by_ref() {
					if c == 'm' {
						break;
					}
				}
			} else {
				out.push(c);
			}
		}
		out
	}
}
