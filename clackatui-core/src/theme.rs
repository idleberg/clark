//! Ported from `@clack/prompts`' `common.ts` — the symbol set and the colours a Frame is drawn
//! with.
//!
//! Upstream this is a page of `export const`s and two `switch`es. Here it is a value, so that a
//! Prompt can be drawn in a different Theme without a global being reassigned underneath it.
//! [`Theme::clack()`] is the one parity is measured against; every other Theme is unverified by
//! construction, which is what CONTEXT.md means by the word.
//!
//! ## Colours, not escapes
//!
//! Upstream reaches for `styleText`, which writes SGR sequences into the Frame string. A Frame here
//! carries a [`Style`] per [`Span`](crate::frame::Span) and no escapes at all (ADR-0011), so the
//! colour names are translated once, here:
//!
//! | `styleText` | SGR | here |
//! |---|---|---|
//! | `gray` | 90 | [`Color::DarkGray`] |
//! | `cyan` | 36 | [`Color::Cyan`] |
//! | `yellow` | 33 | [`Color::Yellow`] |
//! | `red` | 31 | [`Color::Red`] |
//! | `green` | 32 | [`Color::Green`] |
//! | `dim` | 2 | [`Modifier::DIM`] |
//! | `inverse` | 7 | [`Modifier::REVERSED`] |
//! | `hidden` | 8 | [`Modifier::HIDDEN`] |
//! | `strikethrough` | 9 | [`Modifier::CROSSED_OUT`] |
//!
//! Node's `gray` is bright black, SGR 90 — not SGR 30 — which is why the Guide is `DarkGray` and
//! not `Black`. Whether any of it is written at all is the Emitter's decision, not the Theme's:
//! upstream suppresses colour by having `styleText` return the string unchanged, and the equivalent
//! seam here is at the point Styles become bytes.

use ratatui_core::style::{Color, Modifier, Style};

use crate::frame::Span;
use crate::prompt::Status;

/// The symbol set. Upstream's `S_*` constants, in their original order.
///
/// Every field is a `&'static str` rather than a `char` because the ASCII fallbacks are not all one
/// character — `S_CHECKBOX_ACTIVE` falls back to `[•]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Symbols {
	pub step_active: &'static str,
	pub step_cancel: &'static str,
	pub step_error: &'static str,
	pub step_submit: &'static str,

	pub bar_start: &'static str,
	/// The Guide: the bar down the left margin.
	pub bar: &'static str,
	pub bar_end: &'static str,
	pub bar_start_right: &'static str,
	pub bar_end_right: &'static str,

	pub radio_active: &'static str,
	pub radio_inactive: &'static str,
	pub checkbox_active: &'static str,
	pub checkbox_selected: &'static str,
	pub checkbox_inactive: &'static str,
	pub password_mask: &'static str,

	pub bar_h: &'static str,
	pub corner_top_right: &'static str,
	pub connect_left: &'static str,
	pub corner_bottom_right: &'static str,
	pub corner_bottom_left: &'static str,
	pub corner_top_left: &'static str,

	pub info: &'static str,
	pub success: &'static str,
	pub warn: &'static str,
	pub error: &'static str,
}

impl Symbols {
	/// The first argument of every `unicodeOr` call.
	pub const UNICODE: Self = Self {
		step_active: "◆",
		step_cancel: "■",
		step_error: "▲",
		step_submit: "◇",

		bar_start: "┌",
		bar: "│",
		bar_end: "└",
		bar_start_right: "┐",
		bar_end_right: "┘",

		radio_active: "●",
		radio_inactive: "○",
		checkbox_active: "◻",
		checkbox_selected: "◼",
		checkbox_inactive: "◻",
		password_mask: "▪",

		bar_h: "─",
		corner_top_right: "╮",
		connect_left: "├",
		corner_bottom_right: "╯",
		corner_bottom_left: "╰",
		corner_top_left: "╭",

		info: "●",
		success: "◆",
		warn: "▲",
		error: "■",
	};

	/// The second argument of every `unicodeOr` call.
	///
	/// Not a design of ours, and it shows: three step symbols collapse onto `x`, and the two bar
	/// ends are em dashes. Reproduced as written, because a terminal that takes this branch is one
	/// where parity still has to hold.
	pub const ASCII: Self = Self {
		step_active: "*",
		step_cancel: "x",
		step_error: "x",
		step_submit: "o",

		bar_start: "T",
		bar: "|",
		bar_end: "—",
		bar_start_right: "T",
		bar_end_right: "—",

		radio_active: ">",
		radio_inactive: " ",
		checkbox_active: "[•]",
		checkbox_selected: "[+]",
		checkbox_inactive: "[ ]",
		password_mask: "•",

		bar_h: "-",
		corner_top_right: "+",
		connect_left: "+",
		corner_bottom_right: "+",
		corner_bottom_left: "+",
		corner_top_left: "+",

		info: "•",
		success: "*",
		warn: "!",
		error: "x",
	};
}

/// The styles a Frame draws with, named for the role rather than the colour.
///
/// The names are ours; upstream has none, because it spells each `styleText` call out where it is
/// used. Naming them is what makes a second Theme possible at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Styles {
	/// The step symbol, one per [`Status`].
	pub step_active: Style,
	pub step_cancel: Style,
	pub step_error: Style,
	pub step_submit: Style,

	/// The Guide above a Prompt's title, and beside a settled value. Upstream's `gray`.
	pub guide: Style,
	/// The Guide beside a Prompt still being answered.
	pub guide_active: Style,
	/// The Guide beside a Prompt whose validation failed.
	pub guide_error: Style,

	/// The question itself. Unstyled upstream.
	pub message: Style,
	/// The first character of a placeholder, which upstream inverts to stand in for the cursor.
	pub placeholder_cursor: Style,
	/// The rest of the placeholder.
	pub placeholder: Style,
	/// The stand-in drawn when there is no placeholder either: inverse *and* hidden, which is a
	/// cursor-shaped hole with no character in it.
	pub placeholder_empty: Style,
	/// The character the cursor rests on.
	pub cursor: Style,
	/// The radio of the choice a `confirm` is currently on.
	pub radio_selected: Style,
	/// The radio of the choice it is not on.
	pub radio_unselected: Style,
	/// The label beside an unselected radio. A selected one's label is unstyled.
	pub option_unselected: Style,
	/// The `/` between a `confirm`'s two choices.
	pub separator: Style,
	/// The `...` a list draws in place of the options it left out.
	pub overflow: Style,
	/// The note in brackets beside an option. Upstream's `dim`.
	pub hint: Style,
	/// An option that cannot be chosen — both its radio and its label. Upstream's `gray`, which is
	/// the Guide's colour and not the dim the other unselected options are drawn in.
	pub option_disabled: Style,
	/// The key named in an instruction footer, as opposed to what pressing it does.
	pub instruction_key: Style,
	/// The value of a submitted Prompt.
	pub submitted: Style,
	/// The value of a cancelled Prompt.
	pub cancelled: Style,
	/// A validation message.
	pub error: Style,
}

impl Styles {
	/// clack's, translated from `styleText` once.
	pub const CLACK: Self = Self {
		step_active: Style::new().fg(Color::Cyan),
		step_cancel: Style::new().fg(Color::Red),
		step_error: Style::new().fg(Color::Yellow),
		step_submit: Style::new().fg(Color::Green),

		guide: Style::new().fg(Color::DarkGray),
		guide_active: Style::new().fg(Color::Cyan),
		guide_error: Style::new().fg(Color::Yellow),

		message: Style::new(),
		placeholder_cursor: Style::new().add_modifier(Modifier::REVERSED),
		placeholder: Style::new().add_modifier(Modifier::DIM),
		placeholder_empty: Style::new().add_modifier(Modifier::REVERSED.union(Modifier::HIDDEN)),
		cursor: Style::new().add_modifier(Modifier::REVERSED),
		radio_selected: Style::new().fg(Color::Green),
		radio_unselected: Style::new().add_modifier(Modifier::DIM),
		option_unselected: Style::new().add_modifier(Modifier::DIM),
		separator: Style::new().add_modifier(Modifier::DIM),
		overflow: Style::new().add_modifier(Modifier::DIM),
		hint: Style::new().add_modifier(Modifier::DIM),
		option_disabled: Style::new().fg(Color::DarkGray),
		instruction_key: Style::new().add_modifier(Modifier::DIM),
		submitted: Style::new().add_modifier(Modifier::DIM),
		cancelled: Style::new().add_modifier(Modifier::CROSSED_OUT.union(Modifier::DIM)),
		error: Style::new().fg(Color::Yellow),
	};
}

/// The symbol set and style palette a Frame is drawn with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
	pub symbols: Symbols,
	pub styles: Styles,
}

impl Default for Theme {
	fn default() -> Self {
		Self::clack()
	}
}

impl Theme {
	/// clack's own appearance, with the Unicode symbols. The Theme parity is measured against.
	pub const fn clack() -> Self {
		Self {
			symbols: Symbols::UNICODE,
			styles: Styles::CLACK,
		}
	}

	/// clack's appearance where `is-unicode-supported` says no.
	pub const fn ascii() -> Self {
		Self {
			symbols: Symbols::ASCII,
			styles: Styles::CLACK,
		}
	}

	/// The Theme clack would pick in this process. See [`unicode_supported`].
	pub fn detect() -> Self {
		if unicode_supported() {
			Self::clack()
		} else {
			Self::ascii()
		}
	}

	/// `symbol(state)`: the step symbol, in the colour that names the state.
	///
	/// This is the character the Recorder reads a Scenario's outcome off — the first thing on the
	/// title line of every Frame, and so the last one written names the state a Prompt settled in.
	pub fn step(&self, status: Status) -> Span {
		let (symbol, style) = match status {
			Status::Initial | Status::Active => (self.symbols.step_active, self.styles.step_active),
			Status::Cancel => (self.symbols.step_cancel, self.styles.step_cancel),
			Status::Error => (self.symbols.step_error, self.styles.step_error),
			Status::Submit => (self.symbols.step_submit, self.styles.step_submit),
		};
		Span::styled(symbol, style)
	}

	/// `symbolBar(state)`: the Guide, in the same colour as the step symbol.
	///
	/// Not every Prompt uses it. `text` draws its Guide gray above the title and cyan beside the
	/// input, which is neither of the colours this returns — upstream spells those out rather than
	/// calling `symbolBar`, and so does the widget.
	pub fn bar(&self, status: Status) -> Span {
		let style = match status {
			Status::Initial | Status::Active => self.styles.step_active,
			Status::Cancel => self.styles.step_cancel,
			Status::Error => self.styles.step_error,
			Status::Submit => self.styles.step_submit,
		};
		Span::styled(self.symbols.bar, style)
	}
}

/// `is-unicode-supported@1.3.0`, ported.
///
/// Reads the environment, which is why it is a function rather than a constant, and why the Theme a
/// Prompt is drawn in is a value passed to it rather than something it decides for itself. Off
/// Windows the test is one variable: the Linux kernel console cannot draw the symbols, and nothing
/// else is assumed to have that problem.
pub fn unicode_supported() -> bool {
	let var = |name: &str| std::env::var(name).ok();

	if !cfg!(windows) {
		return var("TERM").as_deref() != Some("linux");
	}

	let set = |name: &str| var(name).is_some_and(|v| !v.is_empty());

	set("CI")
		|| set("WT_SESSION")
		|| set("TERMINUS_SUBLIME")
		|| var("ConEmuTask").as_deref() == Some("{cmd::Cmder}")
		|| var("TERM_PROGRAM").as_deref() == Some("Terminus-Sublime")
		|| var("TERM_PROGRAM").as_deref() == Some("vscode")
		|| var("TERM").as_deref() == Some("xterm-256color")
		|| var("TERM").as_deref() == Some("alacritty")
		|| var("TERMINAL_EMULATOR").as_deref() == Some("JetBrains-JediTerm")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_default_theme_is_clacks() {
		assert_eq!(Theme::default(), Theme::clack());
	}

	/// The four characters the Recorder reads a Scenario's outcome off. If these move, every
	/// harvested Fixture stops being interpretable.
	#[test]
	fn the_step_symbols_are_the_ones_the_recorder_looks_for() {
		let theme = Theme::clack();
		assert_eq!(theme.step(Status::Initial).text, "◆");
		assert_eq!(theme.step(Status::Active).text, "◆");
		assert_eq!(theme.step(Status::Submit).text, "◇");
		assert_eq!(theme.step(Status::Cancel).text, "■");
		assert_eq!(theme.step(Status::Error).text, "▲");
	}

	#[test]
	fn a_step_symbol_carries_the_colour_that_names_its_state() {
		let theme = Theme::clack();
		assert_eq!(theme.step(Status::Active).style.fg, Some(Color::Cyan));
		assert_eq!(theme.step(Status::Submit).style.fg, Some(Color::Green));
		assert_eq!(theme.step(Status::Cancel).style.fg, Some(Color::Red));
		assert_eq!(theme.step(Status::Error).style.fg, Some(Color::Yellow));
	}

	/// `initial` and `active` share an arm upstream, in both switches.
	#[test]
	fn an_untouched_prompt_looks_like_an_active_one() {
		let theme = Theme::clack();
		assert_eq!(theme.step(Status::Initial), theme.step(Status::Active));
		assert_eq!(theme.bar(Status::Initial), theme.bar(Status::Active));
	}

	#[test]
	fn the_guide_bar_takes_the_state_colour_but_keeps_its_symbol() {
		let theme = Theme::clack();
		for status in [
			Status::Active,
			Status::Submit,
			Status::Cancel,
			Status::Error,
		] {
			assert_eq!(theme.bar(status).text, "│");
			assert_eq!(theme.bar(status).style.fg, theme.step(status).style.fg);
		}
	}

	/// Node's `gray` is SGR 90, not 30. Getting this wrong would show up as a Grid mismatch on
	/// every Frame with a Guide in it, which is all of them.
	#[test]
	fn the_guide_is_bright_black() {
		assert_eq!(Styles::CLACK.guide.fg, Some(Color::DarkGray));
	}

	#[test]
	fn a_cancelled_value_is_struck_through_and_dim() {
		let cancelled = Styles::CLACK.cancelled.add_modifier;
		assert!(cancelled.contains(Modifier::CROSSED_OUT));
		assert!(cancelled.contains(Modifier::DIM));
	}

	/// The ASCII fallbacks are upstream's, including the collisions. Three states share `x`, which
	/// means a Fixture recorded without Unicode cannot be read back the way `scenario_replay` reads
	/// one — worth knowing before it is tried.
	#[test]
	fn the_ascii_step_symbols_collide_the_way_upstreams_do() {
		let theme = Theme::ascii();
		assert_eq!(theme.step(Status::Cancel).text, "x");
		assert_eq!(theme.step(Status::Error).text, "x");
		assert_eq!(theme.step(Status::Active).text, "*");
		assert_eq!(theme.step(Status::Submit).text, "o");
	}

	#[test]
	fn a_fallback_symbol_may_be_more_than_one_character() {
		assert_eq!(Symbols::ASCII.checkbox_active, "[•]");
	}

	/// A Theme swap changes only the symbols, never the palette. Upstream has no other Theme, so
	/// there is no second palette to be faithful to.
	#[test]
	fn the_two_themes_differ_only_in_their_symbols() {
		assert_eq!(Theme::clack().styles, Theme::ascii().styles);
		assert_ne!(Theme::clack().symbols, Theme::ascii().symbols);
	}
}
