//! Ported from `@clack/core`'s `prompts/confirm.ts` and `@clack/prompts`' `confirm.ts`.
//!
//! `ConfirmPrompt` is the first Prompt here that does not type. It holds a boolean, it flips on any
//! navigation key, and it is the only Prompt in clack that settles from inside a listener rather
//! than at the end of `onKeypress` — see [`ConfirmState::CONFIRMS_ON_KEY`] and ADR-0018.
//!
//! Two of its behaviours look like mistakes and are ported as they are, because a terminal can see
//! both:
//!
//! - **Cancelling flips the answer.** `escape` is an alias for `cancel`, and a non-tracking Prompt
//!   turns every alias into a `cursor` event; `ConfirmPrompt`'s `cursor` listener inverts the value
//!   without looking at which action arrived. So the cancelled Frame of an untouched `confirm` reads
//!   `No`. The harvested Scenario `confirm › can cancel` records exactly that.
//! - **The message is wrapped 10 columns narrower than it looks.** See [`GUIDE_PREFIX_LENGTH`].

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::widgets::Widget;

use crate::frame::{Frame, Line, Span};
use crate::prompt::{Prompt, PromptState, Status};
use crate::settings::Action;
use crate::theme::Theme;
use crate::wrap::wrap;

/// The state of a `confirm` Prompt: one boolean.
#[derive(Clone, Copy, Debug)]
pub struct ConfirmState {
	value: bool,
}

impl Default for ConfirmState {
	/// `initialValue` defaults to `true` in `@clack/prompts`' `confirm()`, which is where the
	/// default lives — the core Prompt's own is `!!undefined`, which is `false`.
	fn default() -> Self {
		Self { value: true }
	}
}

impl ConfirmState {
	pub fn new() -> Self {
		Self::default()
	}

	/// `initialValue`: which of the two choices is highlighted when the Prompt opens.
	pub fn with_initial_value(mut self, value: bool) -> Self {
		self.value = value;
		self
	}

	/// The answer. Always readable — a `confirm` has a value from the moment it opens.
	pub fn value(&self) -> bool {
		self.value
	}
}

impl PromptState for ConfirmState {
	type Value = bool;

	/// `super(opts, false)`. A `confirm` navigates rather than types, so the vim aliases reach it
	/// and `h`/`l` move between the two choices.
	const TRACKS_INPUT: bool = false;

	/// The one Prompt in clack that settles from inside a listener.
	const CONFIRMS_ON_KEY: bool = true;

	/// Upstream's listener is `this.value = !this.value`, with no look at which action arrived. An
	/// `escape` therefore flips the answer on its way to cancelling it.
	fn cursor(&mut self, action: Action) {
		let _ = action;
		self.value = !self.value;
	}

	/// A `y` or an `n`, in either case. [`Self::CONFIRMS_ON_KEY`] is what makes it settle.
	fn confirm(&mut self, yes: bool) {
		self.value = yes;
	}

	fn value(&self) -> Option<&bool> {
		Some(&self.value)
	}
}

/// The width upstream takes off the terminal before wrapping a `confirm`'s message.
///
/// `wrapTextWithPrefix` is called with the *styled* Guide prefix and wraps to
/// `columns - prefix.length`. The prefix draws three columns — a bar and two spaces — but as a
/// JavaScript string it is thirteen characters: `ESC [ 9 0 m`, the bar, `ESC [ 3 9 m`, and the two
/// spaces. So a guided `confirm` breaks its message ten columns early, and the rows it writes stop
/// well short of the margin.
///
/// Reproduced rather than corrected, on ADR-0013's rule: a terminal can see where the message
/// breaks. It is a constant rather than a measurement because the port's Frames carry no escapes at
/// all (ADR-0011) — there is no styled string here to take the length of — and it is thirteen for
/// either Theme, since both draw the bar with one character.
pub const GUIDE_PREFIX_LENGTH: usize = 13;

/// A `confirm` Prompt drawn as a Frame — the `render` callback of `@clack/prompts`' `confirm()`.
pub struct ConfirmWidget<'a> {
	prompt: &'a Prompt<ConfirmState>,
	message: &'a str,
	active: &'a str,
	inactive: &'a str,
	vertical: bool,
	theme: &'a Theme,
	/// The terminal the message is wrapped against. Upstream reads this from the Prompt's own
	/// output stream, which is a different number from the one whole Frames are wrapped to — see
	/// [`crate::emitter::Emitter::frame`]. Outside a test harness the two are the same terminal.
	columns: u16,
	/// `opts.withGuide ?? settings.withGuide` — `None` defers to the Prompt's Settings.
	with_guide: Option<bool>,
}

impl<'a> ConfirmWidget<'a> {
	pub fn new(prompt: &'a Prompt<ConfirmState>, message: &'a str) -> Self {
		Self {
			prompt,
			message,
			active: "Yes",
			inactive: "No",
			vertical: false,
			theme: &THEME,
			columns: 80,
			with_guide: None,
		}
	}

	/// `active`: the label for the `true` choice.
	pub fn with_active(mut self, active: &'a str) -> Self {
		self.active = active;
		self
	}

	/// `inactive`: the label for the `false` choice.
	pub fn with_inactive(mut self, inactive: &'a str) -> Self {
		self.inactive = inactive;
		self
	}

	/// `vertical`: the two choices one above the other rather than side by side.
	pub fn with_vertical(mut self, vertical: bool) -> Self {
		self.vertical = vertical;
		self
	}

	pub fn with_theme(mut self, theme: &'a Theme) -> Self {
		self.theme = theme;
		self
	}

	/// The terminal width the message is wrapped against.
	pub fn with_columns(mut self, columns: u16) -> Self {
		self.columns = columns;
		self
	}

	pub fn with_guide(mut self, with_guide: bool) -> Self {
		self.with_guide = Some(with_guide);
		self
	}

	fn guided(&self) -> bool {
		self.with_guide.unwrap_or(self.prompt.settings().with_guide)
	}

	/// The Frame, branch for branch as upstream's `render` writes it.
	pub fn frame(&self) -> Frame {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let guide = self.guided();
		let status = self.prompt.status();
		let value = self.prompt.state().value();

		let mut frame = Frame::new();

		if guide {
			frame.push(Span::styled(symbols.bar, styles.guide));
		}
		for line in self.title(status) {
			frame.push(line);
		}

		let label = if value { self.active } else { self.inactive };

		match status {
			Status::Submit => {
				let mut settled = Line::blank();
				if guide {
					settled.push(Span::styled(symbols.bar, styles.guide));
					settled.push(Span::raw("  "));
				}
				settled.push(Span::styled(label, styles.submitted));
				frame.push(settled);
			}

			Status::Cancel => {
				let mut settled = Line::blank();
				if guide {
					settled.push(Span::styled(symbols.bar, styles.guide));
					settled.push(Span::raw("  "));
				}
				settled.push(Span::styled(label, styles.cancelled));
				frame.push(settled);

				// Unconditional, unlike `text`, which asks whether there is a value to close under.
				// A `confirm` always has one.
				if guide {
					frame.push(Line::from(Span::styled(symbols.bar, styles.guide)));
				}
			}

			// A `confirm` has no `error` branch: it takes no validator, so the status is unreachable.
			// Upstream's `switch` would fall into `default` here, and so does this.
			_ => {
				let mut choices = Line::blank();
				if guide {
					choices.push(Span::styled(symbols.bar, styles.guide_active));
					choices.push(Span::raw("  "));
				}
				self.choice(&mut choices, self.active, value);

				if self.vertical {
					frame.push(choices);
					let mut second = Line::blank();
					if guide {
						second.push(Span::styled(symbols.bar, styles.guide_active));
						second.push(Span::raw("  "));
					}
					self.choice(&mut second, self.inactive, !value);
					frame.push(second);
				} else {
					choices.push(Span::raw(" "));
					choices.push(Span::styled("/", styles.separator));
					choices.push(Span::raw(" "));
					self.choice(&mut choices, self.inactive, !value);
					frame.push(choices);
				}

				let mut end = Line::blank();
				if guide {
					end.push(Span::styled(symbols.bar_end, styles.guide_active));
				}
				frame.push(end);
				frame.push(Line::blank());
			}
		}

		frame
	}

	/// `wrapTextWithPrefix(output, message, titlePrefixBar, titlePrefix)`.
	///
	/// The first row carries the step symbol, every later one carries the Guide. Upstream passes no
	/// separate `endPrefix`, so the last row is prefixed like the middle ones and there is no
	/// third case here either.
	fn title(&self, status: Status) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let guide = self.guided();

		let prefix = if guide { GUIDE_PREFIX_LENGTH } else { 0 };
		let width = (self.columns as usize).saturating_sub(prefix).max(1);

		let wrapped = wrap(self.message, width);
		wrapped
			.split('\n')
			.enumerate()
			.map(|(index, text)| {
				let mut line = Line::blank();
				if index == 0 {
					line.push(self.theme.step(status));
					line.push(Span::raw("  "));
				} else if guide {
					line.push(Span::styled(symbols.bar, styles.guide));
					line.push(Span::raw("  "));
				}
				line.push(Span::styled(text, styles.message));
				line
			})
			.collect()
	}

	/// One of the two choices: a radio and its label, lit or not.
	fn choice(&self, line: &mut Line, label: &str, selected: bool) {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;

		if selected {
			line.push(Span::styled(symbols.radio_active, styles.radio_selected));
			line.push(Span::raw(" "));
			line.push(Span::styled(label, styles.message));
		} else {
			line.push(Span::styled(
				symbols.radio_inactive,
				styles.radio_unselected,
			));
			line.push(Span::raw(" "));
			line.push(Span::styled(label, styles.option_unselected));
		}
	}
}

/// The default Theme, as somewhere to borrow from.
static THEME: Theme = Theme::clack();

impl Widget for &ConfirmWidget<'_> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		(&self.frame()).render(area, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::line_editor::{Key, KeyName};
	use crate::prompt::Outcome;

	fn confirm() -> Prompt<ConfirmState> {
		Prompt::new(ConfirmState::new())
	}

	fn press(prompt: &mut Prompt<ConfirmState>, name: KeyName) {
		prompt.key(None, &Key::named(name));
	}

	fn typed(prompt: &mut Prompt<ConfirmState>, c: char) {
		prompt.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
	}

	fn answer(prompt: &Prompt<ConfirmState>) -> Option<bool> {
		match prompt.outcome() {
			Some(Outcome::Submitted(v)) => v.copied(),
			_ => None,
		}
	}

	#[test]
	fn a_confirm_opens_on_yes() {
		let mut prompt = confirm();
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt), Some(true));
	}

	#[test]
	fn an_initial_value_picks_the_other_choice() {
		let mut prompt = Prompt::new(ConfirmState::new().with_initial_value(false));
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt), Some(false));
	}

	#[test]
	fn an_arrow_key_flips_the_answer_whichever_way_it_points() {
		// Upstream's listener does not look at the action, so `right` and `left` do the same thing.
		let mut prompt = confirm();
		press(&mut prompt, KeyName::Right);
		assert!(!prompt.state().value());
		press(&mut prompt, KeyName::Right);
		assert!(prompt.state().value());
		press(&mut prompt, KeyName::Left);
		assert!(!prompt.state().value());
	}

	#[test]
	fn the_vim_aliases_reach_a_prompt_that_does_not_type() {
		let mut prompt = confirm();
		typed(&mut prompt, 'h');
		assert!(!prompt.state().value());
	}

	/// The behaviour the module docs call out: `escape` is an alias for `cancel`, and a non-tracking
	/// Prompt turns every alias into a `cursor` event before the cancel check runs.
	#[test]
	fn cancelling_flips_the_answer_on_its_way_out() {
		let mut prompt = confirm();
		press(&mut prompt, KeyName::Escape);
		assert_eq!(prompt.outcome(), Some(Outcome::Cancelled));
		assert!(!prompt.state().value(), "the cancelled Frame reads `No`");
	}

	#[test]
	fn a_y_settles_the_prompt_without_a_return() {
		let mut prompt = confirm();
		typed(&mut prompt, 'n');
		assert_eq!(prompt.status(), Status::Submit);
		assert_eq!(answer(&prompt), Some(false));
		assert!(
			prompt.closed_early(),
			"the Prompt settled inside its own confirm listener"
		);
	}

	#[test]
	fn a_capital_y_confirms_too() {
		let mut prompt = Prompt::new(ConfirmState::new().with_initial_value(false));
		typed(&mut prompt, 'Y');
		assert_eq!(answer(&prompt), Some(true));
	}

	/// `return` settles at the end of the keypress like every other Prompt, and nothing was closed
	/// early — which is the difference the Session writes down as two extra sequences.
	#[test]
	fn a_return_settles_the_ordinary_way() {
		let mut prompt = confirm();
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Submit);
		assert!(!prompt.closed_early());
	}

	// --- The widget -----------------------------------------------------------------------------

	fn drawn(widget: &ConfirmWidget<'_>) -> Vec<String> {
		widget
			.frame()
			.lines
			.iter()
			.map(|line| line.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn an_opening_frame_puts_the_two_choices_side_by_side() {
		let prompt = confirm();
		let widget = ConfirmWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "◆  foo", "│  ● Yes / ○ No", "└", ""]);
	}

	#[test]
	fn the_unselected_choice_is_the_one_the_value_is_not() {
		let mut prompt = confirm();
		press(&mut prompt, KeyName::Right);
		let widget = ConfirmWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget)[2], "│  ○ Yes / ● No");
	}

	#[test]
	fn vertical_puts_them_one_above_the_other() {
		let prompt = confirm();
		let widget = ConfirmWidget::new(&prompt, "foo").with_vertical(true);
		assert_eq!(
			drawn(&widget),
			["│", "◆  foo", "│  ● Yes", "│  ○ No", "└", ""]
		);
	}

	#[test]
	fn custom_labels_replace_both_the_choice_and_the_answer() {
		let mut prompt = confirm();
		let widget = ConfirmWidget::new(&prompt, "foo").with_active("bleep");
		assert_eq!(drawn(&widget)[2], "│  ● bleep / ○ No");

		press(&mut prompt, KeyName::Return);
		let widget = ConfirmWidget::new(&prompt, "foo").with_active("bleep");
		assert_eq!(drawn(&widget), ["│", "◇  foo", "│  bleep"]);
	}

	#[test]
	fn a_cancelled_frame_always_closes_with_a_second_guide() {
		let mut prompt = confirm();
		press(&mut prompt, KeyName::Escape);
		let widget = ConfirmWidget::new(&prompt, "foo");
		assert_eq!(drawn(&widget), ["│", "■  foo", "│  No", "│"]);
	}

	#[test]
	fn without_a_guide_nothing_is_left_in_the_margin() {
		let prompt = confirm();
		let widget = ConfirmWidget::new(&prompt, "foo").with_guide(false);
		assert_eq!(drawn(&widget), ["◆  foo", "● Yes / ○ No", "", ""]);
	}

	/// A message with newlines in it is several rows, each but the first carrying the Guide.
	#[test]
	fn a_multi_line_message_is_prefixed_row_by_row() {
		let prompt = confirm();
		let widget = ConfirmWidget::new(&prompt, "foo\nbar\nbaz");
		assert_eq!(
			drawn(&widget),
			[
				"│",
				"◆  foo",
				"│  bar",
				"│  baz",
				"│  ● Yes / ○ No",
				"└",
				""
			]
		);
	}

	/// The ten columns upstream loses to its own escape sequences. At 40 columns a guided `confirm`
	/// breaks a message as though the terminal were 27 wide, not 37.
	#[test]
	fn a_guided_message_wraps_ten_columns_early() {
		let prompt = confirm();
		let message = "one two three four five six seven";
		let widget = ConfirmWidget::new(&prompt, message).with_columns(40);
		assert_eq!(drawn(&widget)[1], "◆  one two three four five six");
		assert_eq!(drawn(&widget)[2], "│   seven");

		// Without a Guide there is no prefix to mismeasure, and the whole 40 columns are used.
		let widget = ConfirmWidget::new(&prompt, message)
			.with_columns(40)
			.with_guide(false);
		assert_eq!(drawn(&widget)[0], "◆  one two three four five six seven");
	}
}
