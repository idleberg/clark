//! Ported from `@clack/prompts`' `box.ts` — [`note`](crate::note)'s configurable cousin.
//!
//! Same shape on the terminal, almost nothing shared underneath. A `note` measures its content and
//! draws a box that fits; a `box` decides its width first — from the terminal, or a fraction of it —
//! and then makes the content fit *that*. So this one truncates its title, aligns its rows, and can
//! be told which corners to draw.
//!
//! # Nothing here is styled
//!
//! `note` draws its border in the Guide's gray. `box.ts` calls `styleText` nowhere at all: every
//! character it writes is plain, including the bar in the left margin. The only colour a `box` can
//! have is the one `format_border` puts there, and upstream's default for that is the identity.
//!
//! # `width` is documented twice and does neither thing
//!
//! `BoxOptions.width` is documented as "`'auto'` to fit the content or a number for a fixed width",
//! with `'auto'` the default. Both halves are wrong in `box.ts`, in different ways, and [`Width`]
//! has a variant apiece for what actually happens (ADR-0013):
//!
//! - The number is `Math.min(1, opts.width)` and is then multiplied by the terminal's width, so it
//!   is a *fraction* — `width: 40` is not forty columns, it is all of them. [`Width::Fraction`].
//! - The shrink-to-content branch is guarded by `opts?.width === 'auto'`, which an omitted option
//!   does not satisfy. So the default is not `'auto'`: leave `width` out and the box fills the
//!   terminal. [`Width::Full`] is that default and [`Width::Auto`] is what passing `'auto'` does.
//!
//! `rounded` goes the same way for the same reason: documented `@default true`, read as
//! `opts?.rounded ? roundedSymbols : squareSymbols`, so an omitted `rounded` is falsy and the
//! corners come out square. [`Options::default`] says `false`, because that is what a `box()` with
//! no options draws.
//!
//! # The box is always an even number of columns wide
//!
//! Whatever the width works out to, an odd one is nudged: up if there is room to the right, down if
//! there is not. Nothing in upstream explains it and the corpus records it either way.
//!
use crate::frame::{Frame, Line, Span};
use crate::theme::Theme;
use crate::width::width;
use crate::wrap::wrap;

/// Where a row sits in the width it is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
	#[default]
	Left,
	Center,
	Right,
}

/// How wide the box is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Width {
	/// All of the terminal. What upstream does when `width` is left out — see the module docs, this
	/// is the default and `Auto` is only what the documentation says the default is.
	#[default]
	Full,
	/// As wide as the content needs, up to the terminal. `width: 'auto'`, passed explicitly.
	Auto,
	/// A fraction of the terminal's width, capped at all of it. See the module docs — upstream's
	/// `width` is documented as a column count and behaves as this.
	Fraction(f64),
}

/// A caller's `formatBorder`, applied to each border character before it is repeated.
///
/// Upstream's is `(text: string) => string` and is called once per distinct character — the four
/// corners, the horizontal and the vertical — *before* the repeats, so a formatter that returns an
/// escape sequence has it repeated per column. A Frame carries no escapes (ADR-0011), so this
/// returns a drawn [`Line`] instead, the same trade [`crate::note`]'s formatter makes.
pub type Format<'a> = &'a dyn Fn(&str) -> Line;

fn unformatted(text: &str) -> Line {
	Line::from(Span::raw(text))
}

/// Upstream's `defaultFormatBorder`: the character, unchanged and unstyled.
static UNFORMATTED: fn(&str) -> Line = unformatted;

/// Everything `BoxOptions` carries. [`Default`] is upstream's defaults.
#[derive(Clone, Copy)]
pub struct Options<'a> {
	pub content_align: Align,
	pub title_align: Align,
	pub width: Width,
	pub title_padding: usize,
	pub content_padding: usize,
	/// `╭╮╰╯` when true, `┌┐└┘` when false. False by default — see the module docs.
	pub rounded: bool,
	pub with_guide: bool,
	pub format_border: Format<'a>,
}

impl Default for Options<'_> {
	fn default() -> Self {
		Self {
			content_align: Align::Left,
			title_align: Align::Left,
			width: Width::Full,
			title_padding: 1,
			content_padding: 2,
			rounded: false,
			with_guide: true,
			format_border: &UNFORMATTED,
		}
	}
}

/// `box` at upstream's defaults.
pub fn r#box(message: &str, title: &str, columns: usize, theme: &Theme) -> Frame {
	box_with(message, title, columns, theme, &Options::default())
}

/// `box`, configured.
///
/// Everything is computed in [`isize`] because upstream computes in doubles and several of these
/// quantities go negative in a narrow terminal — a title padded wider than the box, a content
/// padding with no room left for content. Where a negative reaches a `repeat`, upstream throws and
/// this writes nothing; a thrown exception leaves nothing on a terminal to be faithful to.
pub fn box_with(
	message: &str,
	title: &str,
	columns: usize,
	theme: &Theme,
	options: &Options<'_>,
) -> Frame {
	let columns = columns as isize;
	let title_padding = options.title_padding as isize;
	let content_padding = options.content_padding as isize;
	// `${S_BAR} ` — one space, where a `note`'s rows get two.
	let prefix_width: isize = if options.with_guide { 2 } else { 0 };
	let title_width = width(title) as isize;

	let fraction = match options.width {
		Width::Full | Width::Auto => 1.0,
		Width::Fraction(width) => width.min(1.0),
	};
	let max_box_width = columns - prefix_width;
	let mut box_width = (columns as f64 * fraction).floor() as isize - prefix_width;

	// `auto` shrinks the box to its content, and only ever shrinks it. `opts?.width === 'auto'` is
	// upstream's guard, and an omitted `width` is not `'auto'` — which is why `Width::Full` is here
	// and is the default.
	if options.width == Width::Auto {
		let longest = message
			.split('\n')
			.fold(title_width + title_padding * 2, |longest, line| {
				longest.max(width(line) as isize + content_padding * 2)
			});
		if longest + 2 < box_width {
			box_width = longest + 2;
		}
	}

	// Odd widths are nudged into the room there is. See the module docs.
	if box_width % 2 != 0 {
		if box_width < max_box_width {
			box_width += 1;
		} else {
			box_width -= 1;
		}
	}

	let inner = box_width - 2;

	// `title.slice(0, maxTitleLength - 3)`, in UTF-16 code units and with JS's negative index — so a
	// box too narrow for an ellipsis cuts from the *end* of the title instead of the start of it, and
	// a title of wide characters is cut by count rather than by column. Both are reproduced; see
	// `slice_utf16`.
	let max_title = inner - title_padding * 2;
	let title = if title_width > max_title {
		format!("{}...", slice_utf16(title, max_title - 3))
	} else {
		title.to_owned()
	};
	let (title_left, title_right) = padding_for(
		width(&title) as isize,
		inner,
		title_padding,
		options.title_align,
	);

	let border = |text: &str| (options.format_border)(text);
	let symbols = if options.rounded {
		[
			theme.symbols.corner_top_left,
			theme.symbols.corner_top_right,
			theme.symbols.corner_bottom_left,
			theme.symbols.corner_bottom_right,
		]
	} else {
		[
			theme.symbols.bar_start,
			theme.symbols.bar_start_right,
			theme.symbols.bar_end,
			theme.symbols.bar_end_right,
		]
	}
	.map(border);
	let horizontal = border(theme.symbols.bar_h);
	let vertical = border(theme.symbols.bar);

	// The prefix is not formatted and not styled — the one border character a `box` draws plain.
	let prefix = || {
		let mut line = Line::blank();
		if options.with_guide {
			line.push(Span::raw(format!("{} ", theme.symbols.bar)));
		}
		line
	};

	let mut frame = Frame::new();

	let mut top = prefix();
	extend(&mut top, &symbols[0], 1);
	extend(&mut top, &horizontal, title_left);
	top.push(Span::raw(title));
	extend(&mut top, &horizontal, title_right);
	extend(&mut top, &symbols[1], 1);
	frame.push(top);

	let wrapped = wrap(message, (inner - content_padding * 2).max(0) as usize);
	for line in wrapped.split('\n') {
		let (left, right) = padding_for(
			width(line) as isize,
			inner,
			content_padding,
			options.content_align,
		);
		let mut row = prefix();
		extend(&mut row, &vertical, 1);
		row.push(Span::raw(spaces(left)));
		row.push(Span::raw(line));
		row.push(Span::raw(spaces(right)));
		extend(&mut row, &vertical, 1);
		frame.push(row);
	}

	let mut bottom = prefix();
	extend(&mut bottom, &symbols[2], 1);
	extend(&mut bottom, &horizontal, inner);
	extend(&mut bottom, &symbols[3], 1);
	frame.push(bottom);

	frame
}

/// Upstream's `getPaddingForLine`. The right side is whatever the left one left over, so a line
/// wider than the box gives a negative — which is where upstream throws.
fn padding_for(line: isize, inner: isize, padding: isize, align: Align) -> (isize, isize) {
	let left = match align {
		Align::Left => padding,
		// `Math.floor`, which is `div_euclid` and not `/` once the numerator goes negative.
		Align::Center => (inner - line).div_euclid(2),
		Align::Right => inner - line - padding,
	};
	(left, inner - left - line)
}

fn spaces(count: isize) -> String {
	" ".repeat(count.max(0) as usize)
}

/// `line`, repeated `count` times, appended to `row`.
fn extend(row: &mut Line, line: &Line, count: isize) {
	for _ in 0..count.max(0) {
		row.spans.extend(line.spans.iter().cloned());
	}
}

/// `text.slice(0, end)` as JavaScript means it: UTF-16 code units, and a negative `end` counted back
/// from the far end of the string rather than clamped to zero.
///
/// The code units are what makes this worth writing out. `slice` cuts a `가` after one unit and a
/// `😀` after two — the first is one column and the second is a surrogate pair — so a truncated
/// title is as long as upstream's only if it is cut by the same count.
fn slice_utf16(text: &str, end: isize) -> String {
	let units: Vec<u16> = text.encode_utf16().collect();
	let length = units.len() as isize;
	let end = if end < 0 {
		(length + end).max(0)
	} else {
		end.min(length)
	} as usize;
	String::from_utf16_lossy(&units[..end])
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::emitter::write_once;

	fn drawn(message: &str, title: &str, columns: usize, options: &Options<'_>) -> String {
		write_once(&box_with(message, title, columns, &Theme::clack(), options))
	}

	/// Upstream's documented default, which upstream only reaches when it is asked for.
	fn auto() -> Options<'static> {
		Options {
			width: Width::Auto,
			..Options::default()
		}
	}

	#[test]
	fn auto_fits_the_content_and_rounds_up_to_an_even_width() {
		// `message` is 7 columns, plus two of content padding either side is 11, plus two borders is
		// 13 — odd, and there is room to the right, so 14. The title's padding is measured in border
		// characters, not spaces: it is the rule the title sits in, so `titlePadding: 1` puts one `─`
		// to its left and the rest of the row to its right.
		assert_eq!(
			drawn("message", "t", 80, &auto()),
			"│ ┌─t──────────┐\n\
			 │ │  message   │\n\
			 │ └────────────┘\n"
		);
	}

	/// The documented default and the real one are not the same box.
	#[test]
	fn an_omitted_width_fills_the_terminal_where_auto_would_not() {
		let first = |options: &Options<'_>| {
			width(
				drawn("hi", "t", 40, options)
					.lines()
					.next()
					.expect("a top row"),
			)
		};
		assert_eq!(first(&Options::default()), 40);
		assert!(first(&auto()) < 40);
	}

	#[test]
	fn without_the_guide_nothing_sits_in_the_left_margin() {
		assert!(
			drawn(
				"hi",
				"",
				80,
				&Options {
					with_guide: false,
					..Options::default()
				}
			)
			.starts_with('┌')
		);
	}

	/// Documented `@default true` and false in the code, so a `box()` with no options is square.
	#[test]
	fn the_corners_are_square_until_they_are_asked_not_to_be() {
		let square = drawn("hi", "", 80, &Options::default());
		assert!(square.contains('┌') && square.contains('┘'), "{square}");
		let rounded = drawn(
			"hi",
			"",
			80,
			&Options {
				rounded: true,
				..Options::default()
			},
		);
		assert!(rounded.contains('╭') && rounded.contains('╯'), "{rounded}");
	}

	/// A title too wide for the box loses its end, not its middle.
	#[test]
	fn a_title_wider_than_the_box_is_truncated_with_an_ellipsis() {
		let drawn = drawn("hi", "a considerably longer title", 20, &Options::default());
		// Eleven code units and an ellipsis, in the fourteen the sixteen-wide inside leaves it.
		assert!(drawn.contains("┌─a considera...─┐"), "{drawn}");
	}

	/// `Fraction` is what upstream's `width` number is: a share of the terminal, capped at all of it.
	#[test]
	fn a_fraction_is_a_share_of_the_terminal() {
		let half = drawn(
			"hi",
			"",
			40,
			&Options {
				width: Width::Fraction(0.5),
				..Options::default()
			},
		);
		// 40 halved is 20, less the two the prefix costs.
		for row in half.lines() {
			assert_eq!(width(row), 20, "{row:?}");
		}
		// Anything at or above one is the whole terminal, which is the defect the docs deny.
		assert_eq!(
			drawn(
				"hi",
				"",
				40,
				&Options {
					width: Width::Fraction(40.0),
					..Options::default()
				}
			),
			drawn(
				"hi",
				"",
				40,
				&Options {
					width: Width::Fraction(1.0),
					..Options::default()
				}
			)
		);
	}

	#[test]
	fn centred_content_sits_in_the_middle_of_the_box() {
		let centred = drawn(
			"hi",
			"",
			40,
			&Options {
				width: Width::Fraction(1.0),
				content_align: Align::Center,
				..Options::default()
			},
		);
		// 38 wide, 36 inside the borders, so `hi` starts at column 17 of the inside.
		assert!(centred.contains(&format!("│{}hi{}│", " ".repeat(17), " ".repeat(17))));
	}

	#[test]
	fn a_slice_cuts_where_javascript_cuts() {
		assert_eq!(slice_utf16("hello", 3), "hel");
		assert_eq!(slice_utf16("hello", 30), "hello");
		// The negative index JS counts from the end, which is what a box too narrow for an ellipsis
		// hands this.
		assert_eq!(slice_utf16("hello", -1), "hell");
		assert_eq!(slice_utf16("hello", -30), "");
		// Two code units, one of them a wide character: `가` is one unit and `😀` is two.
		assert_eq!(slice_utf16("가😀", 2), "가\u{fffd}");
	}
}
