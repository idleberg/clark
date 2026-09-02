//! `date()`, ported from `@clack/prompts`' `prompts/date.ts`.
//!
//! # There is no locale here
//!
//! Upstream's `date()` takes a `locale` and asks `Intl.DateTimeFormat` which segments to draw, in
//! what order and with what between them. Rust's standard library carries no locale data, and
//! pulling in an ICU crate to answer one question about three characters is not a trade this crate
//! makes. [`Date::format`] takes the order outright and [`Date::separator`] the string between,
//! and the default is `MDY` with `/` — the pair every recorded Scenario in the port runs at, because
//! upstream's own suite asks for `en-US`.
//!
//! # `initialValue` and `defaultValue` are the same option here, nearly
//!
//! Unlike [`text`](crate::text), where the two are kept carefully apart. `date`'s constructor reads
//! `opts.initialValue ?? opts.defaultValue`, so a default is typed into the field exactly as an
//! initial value is. What is left of the difference is the fallback: erase the field and a
//! `defaultValue` is still what the Prompt answers with — while the terminal shows `__/__/____`,
//! because the settled Frame draws the segments and not the value.

use clark_core::date::{Date as CivilDate, DateFormat, DateState, DateWidget, validator};
use clark_core::prompt::{Outcome, Prompt, Validator};
use clark_core::session::Session;
use clark_core::settings::{DateMessages, Settings};
use clark_core::theme::Theme;

use crate::driver;
use crate::error::ClackError;

/// A `date` Prompt, waiting to be configured and run.
///
/// ```no_run
/// use clark::DateFormat;
///
/// let birthday = clark::date("Pick your birthday")
///     .format(DateFormat::Ymd)
///     .separator("-")
///     .interact()?;
/// # Ok::<_, clark::ClackError>(())
/// ```
pub struct Date {
	message: String,
	format: DateFormat,
	separator: String,
	initial_value: Option<CivilDate>,
	default_value: Option<CivilDate>,
	min_date: Option<CivilDate>,
	max_date: Option<CivilDate>,
	messages: DateMessages,
	theme: Option<Theme>,
	settings: Option<Settings>,
	with_guide: Option<bool>,
	validator: Option<Box<dyn Validator<CivilDate>>>,
}

/// Ask for a date, one segment at a time.
pub fn date(message: impl Into<String>) -> Date {
	Date {
		message: message.into(),
		// Upstream's default is whatever `Intl` says for the ambient locale. There is no such thing
		// to read here, so the default is the one every recording in the port was made at.
		format: DateFormat::Mdy,
		separator: "/".into(),
		initial_value: None,
		default_value: None,
		min_date: None,
		max_date: None,
		messages: DateMessages::default(),
		theme: None,
		settings: None,
		with_guide: None,
		validator: None,
	}
}

impl Date {
	/// Which segments are drawn, and in what order.
	pub fn format(mut self, format: DateFormat) -> Self {
		self.format = format;
		self
	}

	/// What is drawn between them. `/` unless told otherwise, as upstream is when a `format` is given.
	pub fn separator(mut self, separator: impl Into<String>) -> Self {
		self.separator = separator.into();
		self
	}

	/// The date the field opens on, editable from the first keypress.
	pub fn initial_value(mut self, date: CivilDate) -> Self {
		self.initial_value = Some(date);
		self
	}

	/// The answer to an empty field — which is also typed into the field. See the module docs.
	pub fn default_value(mut self, date: CivilDate) -> Self {
		self.default_value = Some(date);
		self
	}

	/// The earliest date this Prompt will take. It bounds the arrows as well as the answer.
	pub fn min_date(mut self, date: CivilDate) -> Self {
		self.min_date = Some(date);
		self
	}

	/// The latest date this Prompt will take.
	pub fn max_date(mut self, date: CivilDate) -> Self {
		self.max_date = Some(date);
		self
	}

	/// `settings.date.messages`: what the Prompt says when it will not take an answer.
	pub fn messages(mut self, messages: DateMessages) -> Self {
		self.messages = messages;
		self
	}

	/// Reject a date with a message. `None` accepts it.
	///
	/// Asked last, and never about a date already outside [`min_date`](Self::min_date) or
	/// [`max_date`](Self::max_date) — but asked about *nothing at all*, which is what an empty field
	/// reaches the validator as, unless a [`default_value`](Self::default_value) stands in first.
	pub fn validate(mut self, validator: impl Validator<CivilDate> + 'static) -> Self {
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
	pub fn interact(self) -> Result<CivilDate, ClackError> {
		self.interact_opt()?.ok_or(ClackError::Cancelled)
	}

	/// Ask, and report a cancel as [`None`].
	pub fn interact_opt(self) -> Result<Option<CivilDate>, ClackError> {
		let prompt = driver::run(self.session())?;
		Ok(match prompt.outcome() {
			Some(Outcome::Submitted(value)) => value.copied(),
			_ => None,
		})
	}

	/// The Session this Prompt would be run as, without running it.
	pub fn session(self) -> Session<DateState> {
		let mut state =
			DateState::new(self.format, self.separator).with_messages(self.messages.clone());
		// `opts.initialValue ?? opts.defaultValue`, in that order — the default seeds the field only
		// when there is no initial value to seed it with.
		if let Some(default) = self.default_value {
			state = state.with_default_value(default);
		}
		if let Some(initial) = self.initial_value {
			state = state.with_initial_value(initial);
		}
		if let Some(min) = self.min_date {
			state = state.with_min_date(min);
		}
		if let Some(max) = self.max_date {
			state = state.with_max_date(max);
		}

		let mut prompt = Prompt::new(state).with_validator(validator(
			self.min_date,
			self.max_date,
			self.default_value,
			self.messages,
			self.validator,
		));
		if let Some(settings) = self.settings {
			prompt = prompt.with_settings(settings);
		}

		let message = self.message;
		let theme = self.theme.unwrap_or_else(Theme::clack);
		let with_guide = self.with_guide;

		Session::new(prompt, move |prompt, _columns, _rows| {
			let mut widget = DateWidget::new(prompt, &message).with_theme(&theme);
			if let Some(with_guide) = with_guide {
				widget = widget.with_guide(with_guide);
			}
			widget.frame()
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use clark_core::line_editor::{Key, KeyName};
	use clark_core::prompt::Status;

	fn on(year: i64, month: i64, day: i64) -> CivilDate {
		CivilDate::new(year, month, day).expect("a date the test meant")
	}

	fn typed(session: &mut Session<DateState>, digits: &str) {
		for c in digits.chars() {
			session.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
		}
	}

	fn submit(session: &mut Session<DateState>) {
		session.key(None, &Key::named(KeyName::Return));
	}

	fn answer(session: &Session<DateState>) -> Option<CivilDate> {
		match session.outcome() {
			Some(Outcome::Submitted(value)) => value.copied(),
			_ => None,
		}
	}

	#[test]
	fn a_typed_date_comes_back() {
		let mut session = date("Pick a date").session();
		session.open();
		typed(&mut session, "12252025");
		submit(&mut session);
		assert_eq!(answer(&session), Some(on(2025, 12, 25)));
	}

	/// The field with its styling taken off. The highlighted segment and every separator are wrapped
	/// in escapes, so the row is not one run of characters even though it reads as one.
	fn plain(bytes: &str) -> String {
		let mut out = String::new();
		let mut rest = bytes;
		while let Some(at) = rest.find('\u{1b}') {
			out.push_str(&rest[..at]);
			rest = match rest[at..].find('m') {
				Some(end) => &rest[at + end + 1..],
				None => "",
			};
		}
		out.push_str(rest);
		out
	}

	#[test]
	fn the_default_format_is_the_one_every_recording_was_made_at() {
		let mut session = date("Pick").initial_value(on(2025, 1, 15)).session();
		assert!(plain(&session.open()).contains("01/15/2025"));
	}

	#[test]
	fn a_format_and_a_separator_change_what_is_drawn() {
		let mut session = date("Pick")
			.format(DateFormat::Ymd)
			.separator("-")
			.initial_value(on(2025, 1, 15))
			.session();
		assert!(plain(&session.open()).contains("2025-01-15"));
	}

	#[test]
	fn a_blank_field_opens_on_its_labels() {
		let mut session = date("Pick").session();
		assert!(session.open().contains("mm"));
	}

	#[test]
	fn an_empty_answer_is_refused_unless_a_default_stands_in() {
		let mut session = date("Pick").session();
		session.open();
		submit(&mut session);
		assert_eq!(session.status(), Status::Error);

		let mut session = date("Pick").default_value(on(2025, 12, 25)).session();
		session.open();
		// The default was typed into the field, so erasing it is what reaches the fallback.
		for _ in 0..3 {
			session.key(None, &Key::named(KeyName::Backspace));
			session.key(None, &Key::named(KeyName::Right));
		}
		submit(&mut session);
		assert_eq!(answer(&session), Some(on(2025, 12, 25)));
	}

	#[test]
	fn a_date_outside_the_bounds_is_refused_by_name() {
		let mut session = date("Pick")
			.initial_value(on(2025, 1, 10))
			.min_date(on(2025, 1, 15))
			.session();
		session.open();
		let frame = session.key(None, &Key::named(KeyName::Return));
		assert_eq!(session.status(), Status::Error);
		assert!(frame.contains("Date must be on or after 2025-01-15"));
	}

	#[test]
	fn a_callers_validator_has_the_last_word_and_only_over_a_date_clack_allows() {
		let mut session = date("Pick")
			.initial_value(on(2025, 1, 15))
			.validate(|value: Option<&CivilDate>| {
				value
					.filter(|d| d.year() >= 2026)
					.is_none()
					.then(|| "Too early".to_owned())
			})
			.session();
		session.open();
		submit(&mut session);
		assert_eq!(session.status(), Status::Error);

		session.key(None, &Key::named(KeyName::Right));
		session.key(None, &Key::named(KeyName::Right));
		session.key(None, &Key::named(KeyName::Up));
		submit(&mut session);
		assert_eq!(answer(&session), Some(on(2026, 1, 15)));
	}

	#[test]
	fn custom_messages_replace_the_ones_clack_ships() {
		let messages = DateMessages {
			required: "Give me a date".into(),
			..DateMessages::default()
		};
		let mut session = date("Pick").messages(messages).session();
		session.open();
		let frame = session.key(None, &Key::named(KeyName::Return));
		assert!(frame.contains("Give me a date"));
	}

	#[test]
	fn a_cancel_is_no_answer_at_all() {
		let mut session = date("Pick").initial_value(on(2025, 1, 15)).session();
		session.open();
		session.key(None, &Key::named(KeyName::Escape));
		assert_eq!(session.outcome(), Some(Outcome::Cancelled));
		assert_eq!(answer(&session), None);
	}

	#[test]
	fn turning_the_guide_off_takes_the_bar_out_of_the_frame() {
		let mut guided = date("Pick").session();
		let mut bare = date("Pick").with_guide(false).session();
		assert!(guided.open().contains('│'));
		assert!(!bare.open().contains('│'));
	}
}
