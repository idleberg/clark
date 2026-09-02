//! Ported from `@clack/prompts`' `note.ts` — a message in a box, drawn once.
//!
//! The third static renderer, and the first one that reads the terminal's width. `log`, `intro`,
//! `outro` and `cancel` hand a line to the terminal and let *it* break (ADR-0029); a `note` cannot,
//! because it draws a right-hand border and a border only lands in the right column if the text
//! inside it was measured first. So this wraps — with [`crate::wrap`], the same word wrap a Prompt's
//! Frame goes through — and then pads every row out to the width of the widest one.
//!
//! # `format` returns a [`Line`], not a string
//!
//! Upstream's `format` is `(line: string) => string`, and its own tests pass one that adds
//! characters (`* … *`) and one that adds colour (`styleText('red', …)`). A Frame carries no escapes
//! (ADR-0011), so the colour half of that has nowhere to go as a string. Here a formatter returns a
//! drawn [`Line`] instead, which covers both: characters change its width, styles do not — exactly
//! the distinction `wrapWithFormat` draws with `stringWidth`, which ignores SGR.
//!
//! # Wrapping twice
//!
//! `wrapWithFormat` wraps the message, measures the widest row before and after formatting, and
//! wraps *again* at the width less that difference. It is not an optimisation and cannot be folded
//! into one pass: the formatter is only given whole rows, so how much room it needs is unknown until
//! there are rows to give it.
//!
//! # The Guide is only half applied
//!
//! `withGuide` decides the leading bar and whether the bottom-left corner is a `├` or a `╰` — and
//! nothing else. Every row of the box keeps the bar in its left margin either way, so a `note`
//! without the Guide still draws one down the side of itself. Reproduced, not tidied (ADR-0013).

use crate::frame::{Frame, Line, Span};
use crate::theme::Theme;
use crate::width::width;
use crate::wrap::wrap;

/// A caller's `format`, applied to each row of the wrapped message.
pub type Format<'a> = &'a dyn Fn(&str) -> Line;

/// `note` with upstream's default formatter: the row, unchanged and unstyled.
pub fn note(message: &str, title: &str, columns: usize, theme: &Theme, with_guide: bool) -> Frame {
	note_with(message, title, columns, theme, with_guide, &|line| {
		Line::from(Span::raw(line))
	})
}

/// `note`, with a formatter for each row of the message.
pub fn note_with(
	message: &str,
	title: &str,
	columns: usize,
	theme: &Theme,
	with_guide: bool,
	format: Format<'_>,
) -> Frame {
	// Six columns is exactly what the box costs its content: the left bar and two spaces, the two of
	// padding `len` adds, and the right bar.
	let wrapped = wrap_with_format(message, columns.saturating_sub(6), format);

	// `['', ...wrapMsg.split('\n').map(format), '']` — the blank rows above and below the message are
	// not formatted, only the message's own.
	let mut lines = vec![Line::blank()];
	lines.extend(wrapped.split('\n').map(format));
	lines.push(Line::blank());

	let title_width = width(title);
	let len = lines
		.iter()
		.map(Line::width)
		.max()
		.unwrap_or(0)
		.max(title_width)
		+ 2;

	let bar = || Span::styled(theme.symbols.bar, theme.styles.guide);
	let mut frame = Frame::new();

	if with_guide {
		frame.push(Line::from(bar()));
	}

	// `◇  title ───────╮`. Upstream's `Math.max(…, 1)` is kept and is dead: `len` is at least
	// `title_width + 2`, so the subtraction is at least 1 before the floor is applied. Written down
	// rather than dropped, because it is what `note.ts` says — and because `box`, which truncates its
	// title where this does not, is the shape the guard would have been for.
	let rule = theme
		.symbols
		.bar_h
		.repeat(len.saturating_sub(title_width + 1).max(1));
	frame.push(Line::from_iter([
		Span::styled(theme.symbols.step_submit, theme.styles.step_submit),
		Span::raw("  "),
		Span::raw(title),
		Span::raw(" "),
		Span::styled(
			format!("{rule}{}", theme.symbols.corner_top_right),
			theme.styles.guide,
		),
	]));

	for line in lines {
		let padding = len - line.width();
		let mut row = Line::from(bar());
		row.push(Span::raw("  "));
		row.spans.extend(line.spans);
		row.push(Span::raw(" ".repeat(padding)));
		row.push(bar());
		frame.push(row);
	}

	// `len + 2`, because the bottom border spans the two spaces after the left bar as well.
	let corner = if with_guide {
		theme.symbols.connect_left
	} else {
		theme.symbols.corner_bottom_left
	};
	frame.push(Line::from(Span::styled(
		format!(
			"{corner}{}{}",
			theme.symbols.bar_h.repeat(len + 2),
			theme.symbols.corner_bottom_right
		),
		theme.styles.guide,
	)));

	frame
}

/// Upstream's `wrapWithFormat`: wrap, ask the formatter how much room it wants, wrap again.
///
/// The second width can be zero — a formatter that adds as many columns as there are — and
/// [`crate::wrap::breaks`] lays a zero-width line out one code point per row, which is what upstream
/// does with it too. It cannot go below zero here where upstream's can, and the branch that would
/// tell them apart compares two quantities that stay equal either way.
fn wrap_with_format(message: &str, columns: usize, format: Format<'_>) -> String {
	let wrapped = wrap(message, columns);
	let (normal, formatted) = wrapped
		.split('\n')
		.fold((0, 0), |(normal, formatted), line| {
			(normal.max(width(line)), formatted.max(format(line).width()))
		});
	wrap(
		message,
		columns.saturating_sub(formatted.saturating_sub(normal)),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::emitter::write_once;

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

	fn drawn(message: &str, title: &str, columns: usize, with_guide: bool) -> String {
		strip(&write_once(&note(
			message,
			title,
			columns,
			&Theme::clack(),
			with_guide,
		)))
	}

	#[test]
	fn the_box_is_as_wide_as_its_widest_row() {
		assert_eq!(
			drawn("short\na longer line", "title", 80, true),
			"│\n\
			 ◇  title ─────────╮\n\
			 │                 │\n\
			 │  short          │\n\
			 │  a longer line  │\n\
			 │                 │\n\
			 ├─────────────────╯\n"
		);
	}

	/// The title widens the box when the message does not.
	#[test]
	fn a_title_wider_than_the_message_sets_the_width() {
		assert_eq!(
			drawn("hi", "a considerably longer title", 80, true),
			"│\n\
			 ◇  a considerably longer title ─╮\n\
			 │                               │\n\
			 │  hi                           │\n\
			 │                               │\n\
			 ├───────────────────────────────╯\n"
		);
	}

	/// The bar stays in the left margin; only the leading row and the bottom-left corner go.
	#[test]
	fn without_the_guide_the_box_still_has_a_bar_beside_it() {
		assert_eq!(
			drawn("message", "title", 80, false),
			"◇  title ───╮\n\
			 │           │\n\
			 │  message  │\n\
			 │           │\n\
			 ╰───────────╯\n"
		);
	}

	#[test]
	fn a_message_wider_than_the_terminal_is_wrapped_to_it() {
		let drawn = drawn("aaaa bbbb cccc dddd eeee", "t", 20, true);
		for row in drawn.lines().skip(1) {
			assert_eq!(width(row), 20, "{row:?}");
		}
	}

	/// A formatter that adds characters costs the message the room it takes.
	#[test]
	fn a_formatter_that_widens_a_row_narrows_the_wrap() {
		let theme = Theme::clack();
		let starred = |line: &str| Line::from(Span::raw(format!("* {line} *")));
		let frame = note_with("aaaa bbbb cccc", "t", 20, &theme, true, &starred);
		let rendered = strip(&write_once(&frame));
		// Without the formatter all fourteen columns are the message's and it fits on one row; the
		// four the stars cost push `cccc` onto a second.
		assert!(rendered.contains("* aaaa bbbb"), "{rendered}");
		assert!(rendered.contains("cccc *"), "{rendered}");
	}
}
