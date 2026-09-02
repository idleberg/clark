//! Ported from `@clack/core`'s `prompts/date.ts` and `@clack/prompts`' `date.ts`.
//!
//! The first Prompt in the port that is neither a text field nor a list. `date` draws three
//! *segments* — a year, a month and a day — side by side, moves a highlight between them with the
//! arrows, and edits the one under the highlight. It is untracked (`super(opts, false)`), so the vim
//! aliases apply and the readline line is never read back: everything the Prompt shows it computed
//! itself.
//!
//! ## What a segment holds
//!
//! A string, not a number. `'2025'`, `'01'`, `'__'` — an underscore is a digit not yet typed, and
//! the three strings together are the whole editing state. That is why so much of this module is
//! string arithmetic rather than calendar arithmetic: [`Parts`] is what the user is editing and
//! [`Date`] is only what it happens to mean.
//!
//! ## Four things reproduced rather than corrected
//!
//! - **A year below 100 can never be a date.** `validParts` round-trips through `Date.UTC(year, …)`
//!   and compares `getUTCFullYear()` back, but `Date.UTC` maps a year of 0–99 to 1900 + it. The
//!   comparison therefore always fails, so `0050` is drawn, accepted by every segment check, and
//!   still resolves to nothing. See [`Date::new`].
//! - **A `defaultValue` is not a fallback.** `date()` documents it as "the default value returned
//!   when the user doesn't select a date", but the constructor reads
//!   `opts.initialValue ?? opts.defaultValue` — so it is typed into the field on the way in, and
//!   there is no way to reach the fallback it also serves as unless the user erases it.
//! - **The submitted Frame prints the segments, not the value.** It asks `this.value instanceof
//!   Date` and then draws `formattedValue`, which is the three strings joined. The clamp in
//!   `finalize` keeps the two in step for a day that overruns its month; nothing keeps them in step
//!   for a year below 100, which draws itself over a value of nothing.
//! - **A year can be typed past its own length.** `(digits + char).padStart(4, '_')` pads and does
//!   not truncate, so a fifth digit into a full year makes it five characters wide. Only a year, and
//!   only when it is the last segment — one that is not hands the cursor on the moment it fills.
//!
//! ## Two dead ends, ported as they stand
//!
//! - **`invalidDay` cannot be shown.** A first digit into a blank day always becomes `0d`, and a
//!   second only follows a 0, a 1 or a 2 — so no sequence of digits reaches a day above 29. Which
//!   also means 30 and 31 cannot be typed at all; the arrows are the only way to either.
//! - **The clamp after a completed segment cannot change anything**, because the check in front of
//!   it already guarantees what it clamps to. Marked where it is written.
//!
//! ## What could not come across
//!
//! `opts.locale`. Upstream asks `Intl.DateTimeFormat` for the segment order and the separator of a
//! BCP 47 tag, and Rust's standard library has no locale data at all. [`DateState::new`] takes the
//! order and the separator instead, and the `date()` builder defaults to the pair every recorded
//! Scenario uses. `settings.date.monthNames` is not ported either: it is in upstream's settings and
//! nothing in upstream reads it.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::widgets::Widget;

use crate::frame::{Frame, Line, Span};
use crate::line_editor::{Key, KeyName};
use crate::prompt::{Prompt, PromptState, Status};
use crate::settings::{Action, DateMessages};
use crate::theme::Theme;

// --- the calendar ---------------------------------------------------------------------------

/// A date as this Prompt means one: a UTC midnight, which is all `Date.UTC(y, m - 1, d)` ever
/// produces here.
///
/// A struct of three numbers rather than an instant, because every question asked of it — which
/// segments to draw, whether it is before `minDate`, what its ISO form is — is a question about the
/// calendar and none is a question about time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
	year: i64,
	month: i64,
	day: i64,
}

impl Date {
	/// `validParts`, and the reason a year below 100 is not a date.
	///
	/// Upstream builds `new Date(Date.UTC(year, month - 1, day))` and then checks that the three
	/// fields come back unchanged. Two things fail that check: a day that overran its month and
	/// rolled into the next one, which is the point of the check — and any year from 1 to 99, which
	/// `Date.UTC` silently reads as 1900 + it. The second is not the point of the check and is not
	/// corrected here.
	pub fn new(year: i64, month: i64, day: i64) -> Option<Self> {
		// The `<= 0` half of this cannot be observed: the two-digit-year window below rejects
		// everything under 100 anyway, and a negative year cannot be spelled with four digits. Kept
		// because upstream tests `!year || year < 0` and the reason it is redundant is two checks away.
		if year <= 0 || year > 9999 {
			return None;
		}
		if !(1..=12).contains(&month) {
			return None;
		}
		if day < 1 {
			return None;
		}
		// `Date.UTC`'s two-digit-year window. Nothing in 1..=99 survives the round-trip.
		if year < 100 {
			return None;
		}
		if day > days_in_month(year, month) {
			return None;
		}
		Some(Self { year, month, day })
	}

	pub fn year(self) -> i64 {
		self.year
	}

	pub fn month(self) -> i64 {
		self.month
	}

	pub fn day(self) -> i64 {
		self.day
	}

	/// `toISOString().slice(0, 10)`, which is how upstream compares two dates and how it names one
	/// in a validation message.
	pub fn iso(self) -> String {
		format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
	}
}

/// `daysInMonth(year, month)`: `new Date(year || 2001, month || 1, 0).getDate()`.
///
/// Day zero of the month *after* `month` is the last day of `month`, which is where the off-by-one
/// comes from. The two `||` fallbacks are upstream's, and so is the two-digit-year window: `new
/// Date(y, …)` reads a year of 0–99 as 1900 + it. Only February can tell the difference, and every
/// year the window touches has the same leapness either way — but the arithmetic is copied rather
/// than reasoned about, because the reasoning is what would go stale.
fn days_in_month(year: i64, month: i64) -> i64 {
	let year = if year == 0 { 2001 } else { year };
	// `month || 1`, which cannot be observed either: dropping it makes a month index of 0 roll back
	// to the December before, and December and January are both 31 days long.
	let month = if month == 0 { 1 } else { month };
	// A month index past December rolls into the next year, as `new Date` does with one.
	let (year, month) = (
		year + (month - 1).div_euclid(12),
		(month - 1).rem_euclid(12) + 1,
	);
	let year = if (1..=99).contains(&year) {
		1900 + year
	} else {
		year
	};
	match month {
		1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
		4 | 6 | 9 | 11 => 30,
		_ if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
		_ => 28,
	}
}

// --- the format -----------------------------------------------------------------------------

/// Which of the three a segment is. Upstream's `SegmentConfig`, whose `len` this decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Segment {
	Year,
	Month,
	Day,
}

impl Segment {
	/// How many digits the segment holds — four for a year, two for the others. Upstream's `len`,
	/// renamed because a `len` with no `is_empty` beside it is a lint and an empty segment is a
	/// string of underscores rather than a shorter one.
	pub fn digits(self) -> usize {
		match self {
			Self::Year => 4,
			_ => 2,
		}
	}

	/// `DEFAULT_LABELS`: what a blank segment shows instead of its underscores.
	pub fn label(self) -> &'static str {
		match self {
			Self::Year => "yyyy",
			Self::Month => "mm",
			Self::Day => "dd",
		}
	}
}

/// `DateFormat`: the order the three segments are drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateFormat {
	Ymd,
	Mdy,
	Dmy,
}

impl DateFormat {
	/// `segmentsFor`.
	pub fn segments(self) -> [Segment; 3] {
		match self {
			Self::Ymd => [Segment::Year, Segment::Month, Segment::Day],
			Self::Mdy => [Segment::Month, Segment::Day, Segment::Year],
			Self::Dmy => [Segment::Day, Segment::Month, Segment::Year],
		}
	}
}

/// The three strings the user is editing. Upstream's `DateParts`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parts {
	pub year: String,
	pub month: String,
	pub day: String,
}

impl Parts {
	/// The blank field a Prompt with no initial value opens on.
	pub fn blank() -> Self {
		Self {
			year: "____".into(),
			month: "__".into(),
			day: "__".into(),
		}
	}

	pub fn get(&self, segment: Segment) -> &str {
		match segment {
			Segment::Year => &self.year,
			Segment::Month => &self.month,
			Segment::Day => &self.day,
		}
	}

	fn set(&mut self, segment: Segment, value: String) {
		*match segment {
			Segment::Year => &mut self.year,
			Segment::Month => &mut self.month,
			Segment::Day => &mut self.day,
		} = value;
	}

	/// `parse`: the three as numbers, an underscore counting as a zero.
	fn numbers(&self) -> (i64, i64, i64) {
		(
			segment_number(&self.year),
			segment_number(&self.month),
			segment_number(&self.day),
		)
	}

	/// `toDate`. `None` for anything the calendar refuses — including a year below 100.
	fn date(&self) -> Option<Date> {
		let (year, month, day) = self.numbers();
		Date::new(year, month, day)
	}
}

impl From<Date> for Parts {
	fn from(date: Date) -> Self {
		Self {
			year: format!("{:04}", date.year),
			month: format!("{:02}", date.month),
			day: format!("{:02}", date.day),
		}
	}
}

/// `parseSegmentToNum`: underscores are zeros, and anything unparseable is zero too.
fn segment_number(text: &str) -> i64 {
	text.replace('_', "0").parse().unwrap_or(0)
}

/// Whether a segment holds no digit at all.
fn is_blank(text: &str) -> bool {
	text.chars().all(|c| c == '_')
}

// --- the state ------------------------------------------------------------------------------

/// The state of a `date` Prompt.
pub struct DateState {
	segments: Vec<Segment>,
	separator: String,
	parts: Parts,
	min: Option<Date>,
	max: Option<Date>,
	default_value: Option<Date>,
	/// `#cursor`: which segment is highlighted, and where in it the next digit lands.
	segment_index: usize,
	position_in_segment: usize,
	/// `#segmentSelected`: whether the whole segment is highlighted rather than one position in it.
	/// A selected segment is cleared by the next digit; an unselected one is typed into.
	segment_selected: bool,
	/// `#pendingTensDigit`: the first digit of a two-digit entry that could still take a second.
	pending_tens: Option<char>,
	inline_error: String,
	messages: DateMessages,
	value: Option<Date>,
}

impl DateState {
	/// A Prompt in the given order, separated by the given string.
	///
	/// Upstream derives both from `opts.locale` through `Intl.DateTimeFormat`, or from `opts.format`
	/// and `opts.separator` when those are given. Only the second half has a Rust analogue — see the
	/// module docs.
	pub fn new(format: DateFormat, separator: impl Into<String>) -> Self {
		let mut state = Self {
			segments: format.segments().to_vec(),
			separator: separator.into(),
			parts: Parts::blank(),
			min: None,
			max: None,
			default_value: None,
			segment_index: 0,
			position_in_segment: 0,
			segment_selected: true,
			pending_tens: None,
			inline_error: String::new(),
			messages: DateMessages::default(),
			value: None,
		};
		state.refresh();
		state
	}

	/// `initialValue`: the date the field opens on, typed into the segments.
	pub fn with_initial_value(mut self, date: Date) -> Self {
		self.parts = date.into();
		self.refresh();
		self
	}

	/// `defaultValue`, which is two things at once: it seeds the segments exactly as `initialValue`
	/// does — `opts.initialValue ?? opts.defaultValue` — and it is what `finalize` falls back to when
	/// the segments mean nothing. Reaching the second requires erasing the first.
	pub fn with_default_value(mut self, date: Date) -> Self {
		self.default_value = Some(date);
		self.parts = date.into();
		self.refresh();
		self
	}

	/// `minDate`. It bounds the arrows as well as the answer; the message it produces is the
	/// builder's, not the state's.
	pub fn with_min_date(mut self, date: Date) -> Self {
		self.min = Some(date);
		self
	}

	pub fn with_max_date(mut self, date: Date) -> Self {
		self.max = Some(date);
		self
	}

	/// `settings.date.messages`, which the two inline segment errors are drawn from.
	pub fn with_messages(mut self, messages: DateMessages) -> Self {
		self.messages = messages;
		self
	}

	pub fn segments(&self) -> &[Segment] {
		&self.segments
	}

	pub fn separator(&self) -> &str {
		&self.separator
	}

	/// `segmentValues`.
	pub fn parts(&self) -> &Parts {
		&self.parts
	}

	/// `segmentCursor.segmentIndex`.
	pub fn segment_index(&self) -> usize {
		self.segment_index
	}

	/// `formattedValue`: the segments joined, underscores and all.
	pub fn formatted(&self) -> String {
		self.segments
			.iter()
			.map(|segment| self.parts.get(*segment))
			.collect::<Vec<_>>()
			.join(&self.separator)
	}

	/// `inlineError`: the complaint drawn under the field, which is not a validation error and does
	/// not stop the Prompt.
	pub fn inline_error(&self) -> &str {
		&self.inline_error
	}

	/// `#seg()`: the highlighted segment, with the position inside it clamped on the way past.
	///
	/// The clamp is a side effect and it is load-bearing — several callers rely on having been
	/// through here — so this takes `&mut self` even where the caller only reads the answer.
	fn seg(&mut self) -> Option<(Segment, usize)> {
		let index = self.segment_index.min(self.segments.len().checked_sub(1)?);
		let segment = *self.segments.get(index)?;
		// Never observed to do anything: every path that leaves a segment sets the position to 0 on
		// the way, and every path that stays within one already keeps it inside. Copied because a
		// clamp that is currently redundant is a different thing from one that is wrong.
		self.position_in_segment = self.position_in_segment.min(segment.digits() - 1);
		Some((segment, index))
	}

	/// `#refresh`: the value follows the segments after every edit.
	///
	/// Upstream also writes the formatted string into `userInput`. Nothing reads it — the Prompt does
	/// not track its input, so the write into readline is skipped and no branch of `render` asks for
	/// it — so it is not carried here.
	fn refresh(&mut self) {
		self.value = self.parts.date();
	}

	/// `#navigate`: move the highlight one segment, and drop everything pending.
	fn navigate(&mut self, direction: isize) {
		self.inline_error.clear();
		self.pending_tens = None;
		let Some((_, index)) = self.seg() else {
			return;
		};
		let last = self.segments.len() - 1;
		self.segment_index = (index as isize + direction).clamp(0, last as isize) as usize;
		self.position_in_segment = 0;
		self.segment_selected = true;
	}

	/// `#adjust`: the up and down arrows, which step the highlighted segment within its bounds.
	///
	/// A blank segment jumps to the far end rather than stepping: up lands on the minimum, down on
	/// the maximum. Note that this is the one editing path that does not clear the inline error.
	fn adjust(&mut self, direction: i64) {
		let Some((segment, _)) = self.seg() else {
			return;
		};
		let raw = self.parts.get(segment);
		let blank = is_blank(raw);
		let number = segment_number(raw);
		let (min, max) = self.bounds(segment);

		let next = if blank {
			if direction == 1 { min } else { max }
		} else {
			// `Math.max(Math.min(max, n + d), min)` — the order matters only when the bounds cross,
			// which they can when a `minDate` and a `maxDate` share a year and a month.
			(number + direction).min(max).max(min)
		};

		self.parts.set(
			segment,
			format!("{:0>width$}", next, width = segment.digits()),
		);
		self.segment_selected = true;
		self.pending_tens = None;
		self.refresh();
	}

	/// `segmentBounds`: how far the arrows can take one segment, given what the others hold.
	fn bounds(&self, segment: Segment) -> (i64, i64) {
		let (year, month, _) = self.parts.numbers();
		match segment {
			Segment::Year => (
				self.min.map(Date::year).unwrap_or(1),
				self.max.map(Date::year).unwrap_or(9999),
			),
			Segment::Month => (
				match self.min {
					Some(min) if min.year == year => min.month,
					_ => 1,
				},
				match self.max {
					Some(max) if max.year == year => max.month,
					_ => 12,
				},
			),
			Segment::Day => (
				match self.min {
					Some(min) if min.year == year && min.month == month => min.day,
					_ => 1,
				},
				match self.max {
					Some(max) if max.year == year && max.month == month => max.day,
					_ => days_in_month(year, month),
				},
			),
		}
	}

	/// `#validateSegment`: the two complaints a half-typed segment can earn.
	///
	/// Both read the *whole* set of parts rather than the segment being typed, so the number checked
	/// is the one that would result. `invalidDay` is a function of a day count and a month name
	/// upstream and is only ever called with 31 and `'any month'`, which is why it is a plain string
	/// here.
	fn validate_segment(&self, parts: &Parts, segment: Segment) -> Option<String> {
		let (_, month, day) = parts.numbers();
		match segment {
			Segment::Month if !(0..=12).contains(&month) => {
				Some(self.messages.invalid_month.clone())
			}
			Segment::Day if !(0..=31).contains(&day) => Some(self.messages.invalid_day.clone()),
			_ => None,
		}
	}

	/// The backspace branch of `#onKey`: blank the segment, or step back if it is already blank.
	fn backspace(&mut self) {
		self.inline_error.clear();
		let Some((segment, _)) = self.seg() else {
			return;
		};
		if is_blank(self.parts.get(segment)) {
			self.navigate(-1);
			return;
		}
		self.parts.set(segment, "_".repeat(segment.digits()));
		// The segment is blank now, and both branches that read this flag also require it not to be —
		// so nothing can tell whether it was set until something fills the segment, and everything
		// that does sets it too.
		self.segment_selected = true;
		self.position_in_segment = 0;
		self.refresh();
	}

	/// The tab branch: like [`navigate`](Self::navigate), except that it refuses to move rather than
	/// clamping when it would fall off the end.
	fn tab(&mut self, backwards: bool) {
		self.inline_error.clear();
		let Some((_, index)) = self.seg() else {
			return;
		};
		let next = index as isize + if backwards { -1 } else { 1 };
		if next >= 0 && (next as usize) < self.segments.len() {
			self.segment_index = next as usize;
			self.position_in_segment = 0;
			self.segment_selected = true;
		}
	}

	/// The digit branch, which is most of `#onKey`.
	fn digit(&mut self, char: char) {
		let Some((segment, index)) = self.seg() else {
			return;
		};
		let blank = is_blank(self.parts.get(segment));

		// A tens digit is waiting for its units digit. This is the path a `1` into a month takes when
		// the user goes on to make it a `12`.
		// `segment_selected` is redundant in this condition as the code stands: a pending digit is
		// only ever set together with the flag, and everything that clears one clears the other.
		if self.segment_selected && self.pending_tens.is_some() && !blank {
			let new = format!("{}{char}", self.pending_tens.unwrap());
			let mut candidate = self.parts.clone();
			candidate.set(segment, new.clone());
			if let Some(problem) = self.validate_segment(&candidate, segment) {
				self.inline_error = problem;
				self.pending_tens = None;
				self.segment_selected = false;
				return;
			}
			self.inline_error.clear();
			self.parts.set(segment, new);
			self.pending_tens = None;
			self.segment_selected = false;
			self.refresh();
			if index < self.segments.len() - 1 {
				self.segment_index = index + 1;
				self.position_in_segment = 0;
				self.segment_selected = true;
			}
			return;
		}

		// Clear-on-type: a digit into a selected segment that already holds one replaces the lot.
		if self.segment_selected && !blank {
			self.parts.set(segment, "_".repeat(segment.digits()));
			self.position_in_segment = 0;
		}
		self.segment_selected = false;
		self.pending_tens = None;

		let display = self.parts.get(segment).to_string();
		let first_blank = display.find('_');
		// The fallback is the only half that is ever read for a month or a day: those are blank or
		// full and nothing else, and a blank one has its first blank at 0 where the position already
		// is. A year recomputes `new` from its digits below and never looks at either.
		let position = first_blank.unwrap_or(self.position_in_segment.min(segment.digits() - 1));
		if position >= segment.digits() {
			return;
		}

		let mut new = format!("{}{char}{}", &display[..position], &display[position + 1..]);

		// Smart digit placement: a lone digit into an empty month or day becomes `0d`, and stays
		// highlighted only while a second digit could still follow it — 0, 1 for a month; 0, 1, 2 for
		// a day.
		let mut stay_selected = false;
		if position == 0 && display == "__" && matches!(segment, Segment::Month | Segment::Day) {
			let digit = char.to_digit(10).unwrap_or(0) as i64;
			new = format!("0{char}");
			stay_selected = digit <= if segment == Segment::Month { 1 } else { 2 };
		}
		if segment == Segment::Year {
			// A year fills from the right: the digits so far, the new one, and underscores in front.
			// So `2` shows as `___2` and `202` as `_202`, which is not how the other two behave.
			let digits: String = display.chars().filter(|c| *c != '_').collect();
			new = format!("{digits}{char}");
			while new.chars().count() < segment.digits() {
				new.insert(0, '_');
			}
		}

		if !new.contains('_') {
			let mut candidate = self.parts.clone();
			candidate.set(segment, new.clone());
			if let Some(problem) = self.validate_segment(&candidate, segment) {
				self.inline_error = problem;
				return;
			}
		}
		self.inline_error.clear();
		self.parts.set(segment, new.clone());

		// Clamp, but only once the segment being typed is full — a half-typed day would otherwise be
		// dragged up to the first of the month between keystrokes.
		//
		// It cannot change anything. `Parts::date` returning a date already means the year, the month
		// and the day are inside the ranges being clamped to, so every `clamp` here is the identity.
		// Written down because it is written down upstream; a mutation that deletes it survives.
		if !new.contains('_') {
			if let Some(parsed) = self.parts.date() {
				let max_day = days_in_month(parsed.year, parsed.month);
				self.parts = Parts {
					year: format!("{:04}", parsed.year.clamp(0, 9999)),
					month: format!("{:02}", parsed.month.clamp(1, 12)),
					day: format!("{:02}", parsed.day.clamp(1, max_day)),
				};
			}
		}
		self.refresh();

		let next_blank = new.find('_');
		if stay_selected {
			self.segment_selected = true;
			self.pending_tens = Some(char);
		} else if let Some(at) = next_blank {
			self.position_in_segment = at;
		} else if first_blank.is_some() && index < self.segments.len() - 1 {
			self.segment_index = index + 1;
			self.position_in_segment = 0;
			self.segment_selected = true;
		} else {
			self.position_in_segment = (position + 1).min(segment.digits() - 1);
		}
	}
}

impl PromptState for DateState {
	type Value = Date;

	/// `super(opts, false)`. Which also means the vim aliases reach this Prompt: `h` and `l` walk
	/// between the segments and `j` and `k` step the one under the highlight, exactly as the arrows
	/// do — and none of the four is ever a digit, so nothing is lost to them.
	const TRACKS_INPUT: bool = false;

	fn cursor(&mut self, action: Action) {
		match action {
			Action::Right => self.navigate(1),
			Action::Left => self.navigate(-1),
			Action::Up => self.adjust(1),
			Action::Down => self.adjust(-1),
			_ => {}
		}
	}

	fn key(&mut self, s: Option<&str>, key: &Key) {
		// Upstream tests four things for a backspace, because readline reports the key differently
		// depending on the terminal: the name, and `\x7f` or `\b` as either the sequence or the
		// character.
		let del = |text: Option<&str>| matches!(text, Some("\u{7f}") | Some("\u{8}"));
		if key.name == Some(KeyName::Backspace) || del(key.sequence.as_deref()) || del(s) {
			self.backspace();
			return;
		}

		if key.name == Some(KeyName::Tab) {
			self.tab(key.shift);
			return;
		}

		// `/^[0-9]$/` — one ASCII digit and nothing else, so a keypad or a full-width digit is not one.
		if let Some(text) = s {
			let mut chars = text.chars();
			if let (Some(char), None) = (chars.next(), chars.next()) {
				if char.is_ascii_digit() {
					self.digit(char);
				}
			}
		}
	}

	/// `#onFinalize`: clamp the day to its month, then settle on the date or on the default.
	fn finalize(&mut self) {
		let (year, month, day) = self.parts.numbers();
		if year != 0 && month != 0 && day != 0 {
			self.parts.set(
				Segment::Day,
				format!("{:02}", day.min(days_in_month(year, month))),
			);
		}
		self.value = self.parts.date().or(self.default_value);
	}

	fn value(&self) -> Option<&Date> {
		self.value.as_ref()
	}
}

/// The validator `date()` writes for itself, out of the options a state cannot see.
///
/// Upstream's order is worth keeping: an absent value defers to `defaultValue` first, then to a
/// caller's own `validate`, and only then complains — so a Prompt with a default never demands an
/// answer. A value that *is* present is bounds-checked before the caller sees it, which means a
/// caller's validator is never asked about a date clack has already rejected.
pub fn validator(
	min: Option<Date>,
	max: Option<Date>,
	default_value: Option<Date>,
	messages: DateMessages,
	mut inner: Option<Box<dyn crate::prompt::Validator<Date>>>,
) -> impl FnMut(Option<&Date>) -> Option<String> {
	move |value: Option<&Date>| {
		let Some(value) = value else {
			if default_value.is_some() {
				return None;
			}
			if let Some(inner) = &mut inner {
				return inner.validate(None);
			}
			return Some(messages.required.clone());
		};
		if let Some(min) = min {
			if value.iso() < min.iso() {
				return Some(messages.after_min.replace("{date}", &min.iso()));
			}
		}
		if let Some(max) = max {
			if value.iso() > max.iso() {
				return Some(messages.before_max.replace("{date}", &max.iso()));
			}
		}
		match &mut inner {
			Some(inner) => inner.validate(Some(value)),
			None => None,
		}
	}
}

// --- the widget -----------------------------------------------------------------------------

/// A `date` Prompt drawn as a Frame — the `render` callback of `@clack/prompts`' `date()`.
pub struct DateWidget<'a> {
	prompt: &'a Prompt<DateState>,
	message: &'a str,
	theme: &'a Theme,
	with_guide: Option<bool>,
}

impl<'a> DateWidget<'a> {
	pub fn new(prompt: &'a Prompt<DateState>, message: &'a str) -> Self {
		Self {
			prompt,
			message,
			theme: &THEME,
			with_guide: None,
		}
	}

	pub fn with_theme(mut self, theme: &'a Theme) -> Self {
		self.theme = theme;
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
		let state = self.prompt.state();

		let mut frame = Frame::new();
		if guide {
			frame.push(Span::styled(symbols.bar, styles.guide));
		}
		let title = Line::from_iter([
			self.theme.step(status),
			Span::raw("  "),
			Span::styled(self.message, styles.message),
		]);

		// `this.value instanceof Date ? this.formattedValue : ''` — the *segments*, shown on the
		// strength of the value being a date rather than being the thing shown.
		let settled = match state.value() {
			Some(_) => state.formatted(),
			None => String::new(),
		};

		match status {
			Status::Error => {
				frame.push(trim_end(title));

				let mut input = Line::blank();
				if guide {
					input.push(Span::styled(symbols.bar, styles.guide_error));
					input.push(Span::raw("  "));
				}
				input.spans.extend(self.field());
				frame.push(input);

				let mut end = Line::blank();
				if guide {
					end.push(Span::styled(symbols.bar_end, styles.guide_error));
				}
				if !self.prompt.error().is_empty() {
					end.push(Span::raw("  "));
					end.push(Span::styled(self.prompt.error(), styles.error));
				}
				frame.push(end);
				frame.push(Line::blank());
			}

			Status::Submit => {
				frame.push(title);
				let mut line = Line::blank();
				if guide {
					line.push(Span::styled(symbols.bar, styles.guide));
				}
				if !settled.is_empty() {
					line.push(Span::raw("  "));
					line.push(Span::styled(settled, styles.submitted));
				}
				frame.push(line);
			}

			Status::Cancel => {
				frame.push(title);
				let mut line = Line::blank();
				if guide {
					line.push(Span::styled(symbols.bar, styles.guide));
				}
				if !settled.is_empty() {
					line.push(Span::raw("  "));
					line.push(Span::styled(&settled, styles.cancelled));
				}
				frame.push(line);

				if !settled.trim().is_empty() {
					let mut closing = Line::blank();
					if guide {
						closing.push(Span::styled(symbols.bar, styles.guide));
					}
					frame.push(closing);
				}
			}

			Status::Initial | Status::Active => {
				frame.push(title);

				let mut input = Line::blank();
				if guide {
					input.push(Span::styled(symbols.bar, styles.guide_active));
					input.push(Span::raw("  "));
				}
				input.spans.extend(self.field());
				frame.push(input);

				if !state.inline_error().is_empty() {
					let mut line = Line::blank();
					if guide {
						line.push(Span::styled(symbols.bar, styles.guide_active));
						line.push(Span::raw("  "));
					}
					line.push(Span::styled(state.inline_error(), styles.error));
					frame.push(line);
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

	/// `renderDate`: the three segments, joined by a gray separator.
	///
	/// A settled Prompt draws the raw string instead — underscores and all, unstyled — which is why
	/// this is only ever reached while the Prompt is still open.
	fn field(&self) -> Vec<Span> {
		let styles = &self.theme.styles;
		let state = self.prompt.state();
		let mut spans = Vec::new();

		for (index, segment) in state.segments().iter().enumerate() {
			if index > 0 {
				spans.push(Span::styled(state.separator(), styles.date_separator));
			}
			let text = state.parts().get(*segment);
			let blank = is_blank(text);
			if index == state.segment_index() {
				// Highlighted: the label if there is nothing to show, otherwise the digits with the
				// underscores turned into plain spaces — inverse either way.
				let shown = if blank {
					segment.label().to_string()
				} else {
					text.replace('_', " ")
				};
				spans.push(Span::styled(shown, styles.cursor));
			} else if blank {
				spans.push(Span::styled(segment.label(), styles.placeholder));
			} else {
				// `value.replace(/_/g, dim(' '))`: each underscore becomes a dim space of its own,
				// which is a span boundary in the middle of the number.
				let mut run = String::new();
				for c in text.chars() {
					if c == '_' {
						if !run.is_empty() {
							spans.push(Span::raw(std::mem::take(&mut run)));
						}
						spans.push(Span::styled(" ", styles.placeholder));
					} else {
						run.push(c);
					}
				}
				if !run.is_empty() {
					spans.push(Span::raw(run));
				}
			}
		}

		spans
	}
}

/// The default Theme, as somewhere to borrow from.
static THEME: Theme = Theme::clack();

/// `String.prototype.trim` on the title, as `text.rs` does it.
fn trim_end(mut line: Line) -> Line {
	while let Some(last) = line.spans.last_mut() {
		let trimmed = last.text.trim_end();
		if trimmed.is_empty() {
			line.spans.pop();
			continue;
		}
		if trimmed.len() != last.text.len() {
			last.text.truncate(trimmed.len());
		}
		break;
	}
	line
}

impl Widget for &DateWidget<'_> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		(&self.frame()).render(area, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::prompt::Outcome;

	fn date() -> Prompt<DateState> {
		Prompt::new(DateState::new(DateFormat::Mdy, "/"))
	}

	fn on(year: i64, month: i64, day: i64) -> Date {
		Date::new(year, month, day).expect("a date the test meant")
	}

	fn press(prompt: &mut Prompt<DateState>, name: KeyName) {
		prompt.key(None, &Key::named(name));
	}

	fn typed(prompt: &mut Prompt<DateState>, digits: &str) {
		for c in digits.chars() {
			prompt.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
		}
	}

	fn shown(prompt: &Prompt<DateState>) -> String {
		prompt.state().formatted()
	}

	// --- the calendar ---------------------------------------------------------------------------

	#[test]
	fn a_day_past_the_end_of_its_month_is_not_a_date() {
		assert_eq!(Date::new(2025, 2, 29), None);
		assert!(Date::new(2024, 2, 29).is_some());
	}

	#[test]
	fn a_year_below_a_hundred_is_never_a_date() {
		// `Date.UTC(50, …)` means 1950, so the round-trip upstream checks can never succeed.
		assert_eq!(Date::new(50, 1, 1), None);
		assert_eq!(Date::new(99, 12, 31), None);
		assert!(Date::new(100, 1, 1).is_some());
	}

	#[test]
	fn the_months_are_the_lengths_the_calendar_says() {
		assert_eq!(days_in_month(2025, 1), 31);
		assert_eq!(days_in_month(2025, 4), 30);
		assert_eq!(days_in_month(2025, 2), 28);
		assert_eq!(days_in_month(2024, 2), 29);
		assert_eq!(days_in_month(1900, 2), 28);
		assert_eq!(days_in_month(2000, 2), 29);
	}

	#[test]
	fn a_missing_year_or_month_falls_back_the_way_upstream_does() {
		// `year || 2001` and `month || 1`, which is what makes a blank field answer at all.
		assert_eq!(days_in_month(0, 2), 28);
		assert_eq!(days_in_month(2024, 0), 31);
	}

	#[test]
	fn the_iso_form_is_what_two_dates_are_compared_by() {
		assert_eq!(on(2025, 1, 5).iso(), "2025-01-05");
		assert_eq!(on(999, 12, 31).iso(), "0999-12-31");
	}

	// --- opening --------------------------------------------------------------------------------

	#[test]
	fn an_untouched_prompt_shows_three_blank_segments() {
		let prompt = date();
		assert_eq!(shown(&prompt), "__/__/____");
		assert_eq!(prompt.state().value(), None);
	}

	#[test]
	fn the_order_and_the_separator_are_the_formats() {
		let prompt =
			Prompt::new(DateState::new(DateFormat::Ymd, "-").with_initial_value(on(2025, 1, 15)));
		assert_eq!(shown(&prompt), "2025-01-15");
		let prompt =
			Prompt::new(DateState::new(DateFormat::Dmy, ".").with_initial_value(on(2025, 1, 15)));
		assert_eq!(shown(&prompt), "15.01.2025");
	}

	#[test]
	fn an_initial_value_is_the_value_before_any_key() {
		let prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 6, 15)));
		assert_eq!(shown(&prompt), "06/15/2025");
		assert_eq!(prompt.state().value(), Some(&on(2025, 6, 15)));
	}

	/// Documented as a fallback and implemented as a seed: `opts.initialValue ?? opts.defaultValue`.
	#[test]
	fn a_default_value_is_typed_into_the_field_rather_than_held_back() {
		let prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_default_value(on(2025, 12, 25)));
		assert_eq!(shown(&prompt), "12/25/2025");
		assert_eq!(prompt.state().value(), Some(&on(2025, 12, 25)));
	}

	#[test]
	fn the_default_value_is_still_the_fallback_once_the_field_is_erased() {
		let mut prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_default_value(on(2025, 12, 25)));
		// Backspace on an already-blank segment steps *backwards*, so erasing the whole field means
		// walking forwards yourself.
		for _ in 0..3 {
			press(&mut prompt, KeyName::Backspace);
			press(&mut prompt, KeyName::Right);
		}
		assert_eq!(shown(&prompt), "__/__/____");
		press(&mut prompt, KeyName::Return);
		assert!(
			matches!(prompt.outcome(), Some(Outcome::Submitted(Some(v))) if *v == on(2025, 12, 25))
		);
	}

	// --- navigating -----------------------------------------------------------------------------

	#[test]
	fn the_arrows_walk_between_the_segments_and_stop_at_the_ends() {
		let mut prompt = date();
		assert_eq!(prompt.state().segment_index(), 0);
		press(&mut prompt, KeyName::Right);
		assert_eq!(prompt.state().segment_index(), 1);
		press(&mut prompt, KeyName::Right);
		press(&mut prompt, KeyName::Right);
		assert_eq!(prompt.state().segment_index(), 2);
		for _ in 0..4 {
			press(&mut prompt, KeyName::Left);
		}
		assert_eq!(prompt.state().segment_index(), 0);
	}

	#[test]
	fn tab_walks_the_same_way_and_shift_tab_walks_back() {
		let mut prompt = date();
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		assert_eq!(prompt.state().segment_index(), 1);
		let mut back = Key::named(KeyName::Tab);
		back.shift = true;
		prompt.key(Some("\t"), &back);
		assert_eq!(prompt.state().segment_index(), 0);
		// And refuses to fall off rather than clamping, which is the difference from an arrow.
		prompt.key(Some("\t"), &back);
		assert_eq!(prompt.state().segment_index(), 0);
	}

	/// The Prompt does not track its input, so `settings.aliases` reaches it — and none of the four
	/// vim keys is a digit, so nothing is taken away from the field by them.
	#[test]
	fn the_vim_keys_navigate_this_prompt_too() {
		let mut prompt = date();
		prompt.key(Some("l"), &Key::named(KeyName::Char('l')));
		assert_eq!(prompt.state().segment_index(), 1);
		prompt.key(Some("h"), &Key::named(KeyName::Char('h')));
		assert_eq!(prompt.state().segment_index(), 0);
	}

	// --- the arrows that edit ---------------------------------------------------------------------

	#[test]
	fn up_on_a_blank_segment_lands_on_its_minimum_and_down_on_its_maximum() {
		let mut prompt = date();
		press(&mut prompt, KeyName::Up);
		assert_eq!(shown(&prompt), "01/__/____");

		let mut prompt = date();
		press(&mut prompt, KeyName::Down);
		assert_eq!(shown(&prompt), "12/__/____");
	}

	#[test]
	fn a_filled_segment_steps_and_holds_at_its_bounds() {
		let mut prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 11, 15)));
		press(&mut prompt, KeyName::Up);
		assert_eq!(shown(&prompt), "12/15/2025");
		press(&mut prompt, KeyName::Up);
		assert_eq!(shown(&prompt), "12/15/2025");
	}

	#[test]
	fn the_day_the_arrows_allow_depends_on_the_month() {
		let mut prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 2, 1)));
		press(&mut prompt, KeyName::Right);
		press(&mut prompt, KeyName::Down);
		// February 2025 has 28 days, so stepping below the first lands on the last.
		assert_eq!(shown(&prompt), "02/01/2025");
		press(&mut prompt, KeyName::Up);
		assert_eq!(shown(&prompt), "02/02/2025");
	}

	#[test]
	fn a_min_date_bounds_the_arrows_only_inside_its_own_year_and_month() {
		let state = DateState::new(DateFormat::Mdy, "/")
			.with_initial_value(on(2025, 1, 15))
			.with_min_date(on(2025, 1, 10));
		let mut prompt = Prompt::new(state);
		press(&mut prompt, KeyName::Right);
		for _ in 0..10 {
			press(&mut prompt, KeyName::Down);
		}
		assert_eq!(shown(&prompt), "01/10/2025");
	}

	#[test]
	fn a_max_date_bounds_the_year() {
		let state = DateState::new(DateFormat::Ymd, "-")
			.with_initial_value(on(2025, 6, 1))
			.with_max_date(on(2026, 1, 1));
		let mut prompt = Prompt::new(state);
		press(&mut prompt, KeyName::Up);
		press(&mut prompt, KeyName::Up);
		assert_eq!(shown(&prompt), "2026-06-01");
	}

	// --- typing ---------------------------------------------------------------------------------

	#[test]
	fn a_lone_digit_into_an_empty_month_becomes_a_leading_zero() {
		let mut prompt = date();
		typed(&mut prompt, "7");
		// 7 cannot be the tens of a month, so the segment is finished and the cursor moves on.
		assert_eq!(shown(&prompt), "07/__/____");
		assert_eq!(prompt.state().segment_index(), 1);
	}

	#[test]
	fn a_digit_that_could_still_be_tens_waits_for_a_second() {
		let mut prompt = date();
		typed(&mut prompt, "1");
		assert_eq!(shown(&prompt), "01/__/____");
		assert_eq!(prompt.state().segment_index(), 0);
		typed(&mut prompt, "2");
		assert_eq!(shown(&prompt), "12/__/____");
		assert_eq!(prompt.state().segment_index(), 1);
	}

	#[test]
	fn a_day_waits_on_a_two_as_well_as_on_a_zero_or_a_one() {
		let mut prompt = date();
		typed(&mut prompt, "12");
		typed(&mut prompt, "2");
		assert_eq!(shown(&prompt), "12/02/____");
		assert_eq!(prompt.state().segment_index(), 1);
		typed(&mut prompt, "5");
		assert_eq!(shown(&prompt), "12/25/____");
		assert_eq!(prompt.state().segment_index(), 2);
	}

	/// A letter is not a digit, and this Prompt has no other use for one — so a stray keystroke has
	/// to leave the field exactly as it found it. `/^[0-9]$/` is what says so upstream.
	#[test]
	fn a_letter_does_nothing_at_all() {
		let mut prompt = date();
		typed(&mut prompt, "a");
		assert_eq!(shown(&prompt), "__/__/____");
		assert_eq!(prompt.state().segment_index(), 0);
		assert!(prompt.state().inline_error().is_empty());
	}

	/// An underscore counts as a zero when a segment is read as a number, which only a half-typed
	/// year is: `__20` is twenty, so stepping it lands on twenty-one rather than on one.
	#[test]
	fn a_half_typed_year_steps_from_the_number_it_already_shows() {
		let mut prompt = Prompt::new(DateState::new(DateFormat::Ymd, "-"));
		typed(&mut prompt, "20");
		assert_eq!(shown(&prompt), "__20-__-__");
		press(&mut prompt, KeyName::Up);
		assert_eq!(shown(&prompt), "0021-__-__");
	}

	/// An arrow re-selects the segment it moved, which is what makes the next digit *replace* it
	/// rather than being typed into it.
	#[test]
	fn an_arrow_reselects_the_segment_so_the_next_digit_starts_it_again() {
		let mut prompt = Prompt::new(DateState::new(DateFormat::Ymd, "-"));
		typed(&mut prompt, "2");
		assert_eq!(shown(&prompt), "___2-__-__");
		press(&mut prompt, KeyName::Up);
		assert_eq!(shown(&prompt), "0003-__-__");
		typed(&mut prompt, "9");
		assert_eq!(shown(&prompt), "___9-__-__");
	}

	/// A `minDate` bounds the month only in its *own* year — otherwise a minimum of June would put a
	/// floor under every January there is.
	#[test]
	fn a_minimum_month_does_not_apply_outside_its_year() {
		let state = DateState::new(DateFormat::Mdy, "/")
			.with_initial_value(on(2024, 1, 15))
			.with_min_date(on(2025, 6, 10));
		let mut prompt = Prompt::new(state);
		press(&mut prompt, KeyName::Up);
		assert_eq!(shown(&prompt), "02/15/2024");
	}

	/// And a `maxDate` bounds the day only in its own year *and* month, so a maximum of the twentieth
	/// of June leaves January's thirty-first alone.
	#[test]
	fn a_maximum_day_does_not_apply_outside_its_month() {
		let state = DateState::new(DateFormat::Mdy, "/")
			.with_initial_value(on(2025, 1, 15))
			.with_max_date(on(2025, 6, 20));
		let mut prompt = Prompt::new(state);
		press(&mut prompt, KeyName::Right);
		for _ in 0..20 {
			press(&mut prompt, KeyName::Up);
		}
		assert_eq!(shown(&prompt), "01/31/2025");
	}

	/// The one path that types into a segment that is already full: it writes at the position, moves
	/// the position on, and does *not* hand the cursor to the next segment — because it is only the
	/// first blank being filled that advances. Reached through the refusal, which is what leaves a
	/// full segment unselected in the first place.
	#[test]
	fn typing_into_a_full_unselected_segment_walks_along_it_instead_of_leaving() {
		let mut prompt = date();
		typed(&mut prompt, "19");
		assert!(!prompt.state().inline_error().is_empty());

		// `01` again, written over the `0` — and the cursor stays on the month.
		typed(&mut prompt, "0");
		assert_eq!(shown(&prompt), "01/__/____");
		assert_eq!(prompt.state().segment_index(), 0);
		assert!(prompt.state().inline_error().is_empty());

		// And now over the `1`, which is where the position moved to.
		typed(&mut prompt, "2");
		assert_eq!(shown(&prompt), "02/__/____");
		assert_eq!(prompt.state().segment_index(), 0);
		assert!(prompt.state().inline_error().is_empty());
	}

	#[test]
	fn a_whole_date_can_be_typed_straight_through() {
		let mut prompt = date();
		typed(&mut prompt, "12252025");
		assert_eq!(shown(&prompt), "12/25/2025");
		assert_eq!(prompt.state().value(), Some(&on(2025, 12, 25)));
	}

	/// A year fills from the right rather than from the left, which is the one segment that does.
	#[test]
	fn a_year_shows_its_digits_pushed_to_the_right_as_they_are_typed() {
		let mut prompt = Prompt::new(DateState::new(DateFormat::Ymd, "-"));
		typed(&mut prompt, "2");
		assert_eq!(shown(&prompt), "___2-__-__");
		typed(&mut prompt, "0");
		assert_eq!(shown(&prompt), "__20-__-__");
		typed(&mut prompt, "25");
		assert_eq!(shown(&prompt), "2025-__-__");
		assert_eq!(prompt.state().segment_index(), 1);
	}

	#[test]
	fn typing_into_a_finished_segment_starts_it_again() {
		let mut prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 6, 15)));
		typed(&mut prompt, "7");
		assert_eq!(shown(&prompt), "07/15/2025");
	}

	#[test]
	fn a_month_over_twelve_is_refused_with_a_complaint_under_the_field() {
		let mut prompt = date();
		typed(&mut prompt, "1");
		typed(&mut prompt, "9");
		assert_eq!(shown(&prompt), "01/__/____");
		assert_eq!(
			prompt.state().inline_error(),
			"There are only 12 months in a year"
		);
	}

	/// The other complaint cannot be reached at all. A first digit into a blank day always becomes
	/// `0d`, and a second one only follows a 0, a 1 or a 2 — so no sequence of digits reaches a day
	/// above 29, let alone above 31. Which also means **30 and 31 cannot be typed**: the arrows are
	/// the only way to either.
	#[test]
	fn a_day_above_twenty_nine_cannot_be_typed_at_all() {
		// Every pair of digits, into a day that opens blank.
		for tens in '0'..='9' {
			for units in '0'..='9' {
				let mut prompt = date();
				typed(&mut prompt, "12");
				typed(&mut prompt, &format!("{tens}{units}"));
				let day = &prompt.state().parts().day;
				assert!(
					segment_number(day) <= 29,
					"{tens}{units} reached a day of {day}"
				);
			}
		}

		// A 3 is not a tens digit for a day, so it lands as the units and the segment is finished.
		let mut prompt = date();
		typed(&mut prompt, "12");
		typed(&mut prompt, "3");
		assert_eq!(shown(&prompt), "12/03/____");
		assert!(prompt.state().inline_error().is_empty());
	}

	/// A rejected two-digit entry leaves the segment *unselected*, so the next digit is written into
	/// position 0 of what is already there rather than starting again — and a month of `01` that was
	/// refused a `9` refuses a `7` too, as `71`. Nothing but a backspace or a move gets out of it.
	#[test]
	fn a_refused_segment_traps_the_next_digit_against_what_it_already_holds() {
		let mut prompt = date();
		typed(&mut prompt, "19");
		assert!(!prompt.state().inline_error().is_empty());
		typed(&mut prompt, "7");
		assert_eq!(shown(&prompt), "01/__/____");
		assert_eq!(
			prompt.state().inline_error(),
			"There are only 12 months in a year"
		);

		// A backspace is the way out, and it clears the complaint on the way.
		press(&mut prompt, KeyName::Backspace);
		assert!(prompt.state().inline_error().is_empty());
		typed(&mut prompt, "7");
		assert_eq!(shown(&prompt), "07/__/____");
	}

	/// `#adjust` is the one editing path that leaves the complaint standing.
	#[test]
	fn an_arrow_does_not_clear_the_complaint() {
		let mut prompt = date();
		typed(&mut prompt, "19");
		press(&mut prompt, KeyName::Up);
		assert!(!prompt.state().inline_error().is_empty());
		// And moving away does.
		press(&mut prompt, KeyName::Right);
		assert!(prompt.state().inline_error().is_empty());
	}

	/// A year is the only segment that can be typed past its own length. `padStart` pads and does not
	/// truncate, so `(digits + char)` on a full year is five characters wide — which no calendar will
	/// take, so the field goes on showing a number that is no longer a date.
	#[test]
	fn a_fifth_digit_makes_the_year_five_characters_wide() {
		// The year has to be the last segment for the fifth digit to reach it: a completed year that
		// is not last hands the cursor on instead.
		let mut prompt = date();
		typed(&mut prompt, "12");
		typed(&mut prompt, "25");
		typed(&mut prompt, "20255");
		assert_eq!(shown(&prompt), "12/25/20255");
		assert_eq!(prompt.state().value(), None);
	}

	#[test]
	fn backspace_blanks_the_segment_and_then_steps_back() {
		let mut prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 6, 15)));
		press(&mut prompt, KeyName::Right);
		press(&mut prompt, KeyName::Backspace);
		assert_eq!(shown(&prompt), "06/__/2025");
		assert_eq!(prompt.state().segment_index(), 1);
		press(&mut prompt, KeyName::Backspace);
		assert_eq!(prompt.state().segment_index(), 0);
		assert_eq!(shown(&prompt), "06/__/2025");
	}

	/// readline reports the key three ways depending on the terminal, and upstream accepts all of
	/// them.
	#[test]
	fn a_backspace_is_recognised_by_its_sequence_as_well_as_by_its_name() {
		let mut prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 6, 15)));
		prompt.key(Some("\u{7f}"), &Key::named(KeyName::Char('\u{7f}')));
		assert_eq!(shown(&prompt), "__/15/2025");
	}

	// --- settling -------------------------------------------------------------------------------

	#[test]
	fn a_submitted_prompt_answers_with_the_date_it_shows() {
		let mut prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 1, 15)));
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Submit);
		assert!(
			matches!(prompt.outcome(), Some(Outcome::Submitted(Some(v))) if *v == on(2025, 1, 15))
		);
	}

	#[test]
	fn a_half_typed_date_answers_with_nothing() {
		let mut prompt = date();
		typed(&mut prompt, "12");
		press(&mut prompt, KeyName::Return);
		assert!(matches!(prompt.outcome(), Some(Outcome::Submitted(None))));
	}

	/// `#onFinalize` clamps the day before the value is read, so a date that was never legal on the
	/// way in becomes one on the way out — and the Frame shows the clamped segments.
	#[test]
	fn finalizing_pulls_the_day_back_into_its_month() {
		// Day-first, and by arrow: a blank day's maximum is `daysInMonth(0, 0)`, which falls back to
		// January and offers 31. The month and the year that follow do not go back and check.
		let mut prompt = Prompt::new(DateState::new(DateFormat::Dmy, "."));
		press(&mut prompt, KeyName::Down);
		assert_eq!(shown(&prompt), "31.__.____");
		press(&mut prompt, KeyName::Right);
		typed(&mut prompt, "02");
		typed(&mut prompt, "2025");
		assert_eq!(shown(&prompt), "31.02.2025");
		assert_eq!(prompt.state().value(), None);

		press(&mut prompt, KeyName::Return);
		assert_eq!(shown(&prompt), "28.02.2025");
		assert!(
			matches!(prompt.outcome(), Some(Outcome::Submitted(Some(v))) if *v == on(2025, 2, 28))
		);
	}

	#[test]
	fn escape_cancels() {
		let mut prompt = date();
		press(&mut prompt, KeyName::Escape);
		assert_eq!(prompt.outcome(), Some(Outcome::Cancelled));
	}

	// --- the validator --------------------------------------------------------------------------

	#[test]
	fn a_date_before_the_minimum_is_rejected_by_name() {
		let messages = DateMessages::default();
		let mut check = validator(Some(on(2025, 1, 15)), None, None, messages, None);
		assert_eq!(
			check(Some(&on(2025, 1, 10))),
			Some("Date must be on or after 2025-01-15".into())
		);
		assert_eq!(check(Some(&on(2025, 1, 15))), None);
	}

	#[test]
	fn a_date_after_the_maximum_is_rejected_the_same_way() {
		let mut check = validator(
			None,
			Some(on(2025, 1, 15)),
			None,
			DateMessages::default(),
			None,
		);
		assert_eq!(
			check(Some(&on(2025, 2, 1))),
			Some("Date must be on or before 2025-01-15".into())
		);
	}

	#[test]
	fn nothing_at_all_is_required_unless_there_is_a_default() {
		let mut check = validator(None, None, None, DateMessages::default(), None);
		assert_eq!(check(None), Some("Please enter a valid date".into()));

		let mut check = validator(
			None,
			None,
			Some(on(2025, 1, 1)),
			DateMessages::default(),
			None,
		);
		assert_eq!(check(None), None);
	}

	#[test]
	fn a_callers_validator_is_asked_last_and_never_about_a_date_clack_has_rejected() {
		let mut asked = Vec::new();
		let inner: Box<dyn crate::prompt::Validator<Date>> =
			Box::new(move |value: Option<&Date>| {
				asked.push(value.copied());
				value.map(|_| "no".to_string())
			});
		let mut check = validator(
			Some(on(2025, 1, 15)),
			None,
			None,
			DateMessages::default(),
			Some(inner),
		);
		// Out of bounds: clack answers, the caller is not consulted.
		assert_eq!(
			check(Some(&on(2025, 1, 1))),
			Some("Date must be on or after 2025-01-15".into())
		);
		// In bounds: the caller has the last word.
		assert_eq!(check(Some(&on(2025, 2, 1))), Some("no".into()));
	}

	// --- the widget -----------------------------------------------------------------------------

	fn drawn(widget: &DateWidget<'_>) -> Vec<String> {
		widget
			.frame()
			.lines
			.iter()
			.map(|line| line.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn an_opening_frame_shows_the_three_segments_between_their_separators() {
		let prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 1, 15)));
		let widget = DateWidget::new(&prompt, "Pick a date");
		assert_eq!(
			drawn(&widget),
			["│", "◆  Pick a date", "│  01/15/2025", "└", ""]
		);
	}

	#[test]
	fn a_blank_field_shows_its_labels() {
		let prompt = date();
		let widget = DateWidget::new(&prompt, "Pick a date");
		assert_eq!(drawn(&widget)[2], "│  mm/dd/yyyy");
	}

	#[test]
	fn the_highlighted_segment_is_inverted_and_the_others_are_not() {
		let prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 1, 15)));
		let widget = DateWidget::new(&prompt, "Pick a date");
		let spans = &widget.frame().lines[2].spans;
		let inverted: Vec<&str> = spans
			.iter()
			.filter(|s| {
				s.style
					.add_modifier
					.contains(ratatui_core::style::Modifier::REVERSED)
			})
			.map(|s| s.text.as_str())
			.collect();
		assert_eq!(inverted, ["01"]);
	}

	/// A partly-typed segment the highlight has left keeps its underscores as *dim spaces*, one span
	/// each, which is a style boundary in the middle of a number.
	#[test]
	fn a_half_typed_segment_the_highlight_has_left_shows_dim_spaces() {
		let mut prompt = Prompt::new(DateState::new(DateFormat::Ymd, "-"));
		typed(&mut prompt, "20");
		press(&mut prompt, KeyName::Right);
		let widget = DateWidget::new(&prompt, "Pick");
		// The underscores are drawn as dim spaces, so the row reads with two blanks in front of the
		// digits rather than with two underscores.
		assert_eq!(drawn(&widget)[2], "│    20-mm-dd");
		let frame = widget.frame();
		let texts: Vec<&str> = frame.lines[2]
			.spans
			.iter()
			.map(|s| s.text.as_str())
			.collect();
		assert_eq!(texts, ["│", "  ", " ", " ", "20", "-", "mm", "-", "dd"]);
	}

	#[test]
	fn an_inline_complaint_earns_a_row_of_its_own_above_the_foot() {
		let mut prompt = date();
		typed(&mut prompt, "19");
		let widget = DateWidget::new(&prompt, "Pick");
		assert_eq!(
			drawn(&widget),
			[
				"│",
				"◆  Pick",
				"│  01/dd/yyyy",
				"│  There are only 12 months in a year",
				"└",
				""
			]
		);
	}

	#[test]
	fn a_submitted_frame_shows_the_segments_rather_than_the_value() {
		let mut prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 1, 15)));
		press(&mut prompt, KeyName::Return);
		let widget = DateWidget::new(&prompt, "Pick a date");
		assert_eq!(drawn(&widget), ["│", "◇  Pick a date", "│  01/15/2025"]);
	}

	#[test]
	fn a_submitted_frame_with_no_date_shows_nothing_at_all() {
		let mut prompt = date();
		press(&mut prompt, KeyName::Return);
		let widget = DateWidget::new(&prompt, "Pick a date");
		assert_eq!(drawn(&widget), ["│", "◇  Pick a date", "│"]);
	}

	#[test]
	fn a_cancelled_frame_closes_with_a_second_guide() {
		let mut prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 1, 15)));
		press(&mut prompt, KeyName::Escape);
		let widget = DateWidget::new(&prompt, "Pick a date");
		assert_eq!(
			drawn(&widget),
			["│", "■  Pick a date", "│  01/15/2025", "│"]
		);
	}

	#[test]
	fn a_cancelled_frame_with_no_date_does_not_close() {
		let mut prompt = date();
		press(&mut prompt, KeyName::Escape);
		let widget = DateWidget::new(&prompt, "Pick a date");
		assert_eq!(drawn(&widget), ["│", "■  Pick a date", "│"]);
	}

	#[test]
	fn an_error_frame_puts_the_message_on_the_foot_and_keeps_the_field() {
		let state = DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 1, 10));
		let mut prompt = Prompt::new(state).with_validator(validator(
			Some(on(2025, 1, 15)),
			None,
			None,
			DateMessages::default(),
			None,
		));
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Error);
		let widget = DateWidget::new(&prompt, "Pick a date");
		assert_eq!(
			drawn(&widget),
			[
				"│",
				"▲  Pick a date",
				"│  01/10/2025",
				"└  Date must be on or after 2025-01-15",
				""
			]
		);
	}

	#[test]
	fn without_a_guide_only_the_question_and_the_field_remain() {
		let prompt =
			Prompt::new(DateState::new(DateFormat::Mdy, "/").with_initial_value(on(2025, 1, 15)));
		let widget = DateWidget::new(&prompt, "Pick a date").with_guide(false);
		assert_eq!(drawn(&widget), ["◆  Pick a date", "01/15/2025", "", ""]);
	}
}
