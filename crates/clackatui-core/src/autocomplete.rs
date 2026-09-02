//! Ported from `@clack/core`'s `prompts/autocomplete.ts` and `@clack/prompts`' `autocomplete.ts`.
//!
//! Two Prompts out of one state — `autocomplete`, which answers with one option, and
//! `autocompleteMultiselect`, which answers with several — and the first Prompt in the port that
//! both types and navigates. A `select` reads its keys as directions; a `text` reads them as
//! characters; this one does both at once, so `j` is a `j` and `↑` is still a step. That is
//! upstream's `super(opts)` without the `false`: [`PromptState::TRACKS_INPUT`] is on, which is also
//! what keeps the vim aliases off a Prompt you are supposed to be able to spell in.
//!
//! # The list is a filter of a list
//!
//! Everything downstream of the search box works on `filteredOptions`, and everything upstream of it
//! on `options`, and the two are indexed by different cursors — `#cursor` walks the filtered list
//! while the constructor's initial value is looked up in the whole one. What holds the two together
//! is `focusedValue`, a *value* rather than a position, re-found after every keystroke. Here the
//! filtered list is a `Vec<usize>` into the options rather than a second copy of them, which is the
//! same indirection said once instead of twice.
//!
//! # Three things reproduced rather than corrected
//!
//! - **The cursor drawn in the search box is the list's cursor.** `userInputWithCursor` decides
//!   *whether* the cursor is past the end of the text with `_cursor`, the text cursor, and then
//!   slices the text with `this.cursor`, which is the getter for the option list's. Type three
//!   characters, press left, and the highlight lands on whichever character the option cursor's
//!   index happens to point at. See [`search_input`].
//! - **The two Prompts measure the same prefix differently.** `autocomplete` passes `columnPadding:
//!   3` — the bar and its two spaces, counted honestly, which is the one list Prompt in clack that
//!   does not charge itself for the escapes around them (ADR-0019). `autocompleteMultiselect` passes
//!   no padding at all, so its options are wrapped as though the prefix were not there and overrun
//!   it by three columns.
//! - **An unguided `autocompleteMultiselect` draws a blank row under its title.** Its header is
//!   `${title}${hasGuide ? bar : ''}` split on newlines, and `title` ends in one — so the bar's place
//!   is taken by the empty string rather than left out. `autocomplete` pushes its bar inside an
//!   `if (hasGuide)` and has no such row.

use std::fmt::Display;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::widgets::Widget;

use crate::cursor::find_cursor;
use crate::frame::{Frame, Line, Span};
use crate::limit_options::LimitOptions;
use crate::line_editor::{Key, KeyName};
use crate::prompt::{Prompt, PromptState, Status};
use crate::select::{SelectOption, plain_title};
use crate::text::CURSOR_BLOCK;
use crate::theme::Theme;

/// The columns the two Prompts take off the terminal for the prefix beside an option.
///
/// Three, which is what the bar and its two spaces actually draw. Every other list Prompt subtracts
/// thirteen here, the length of the same prefix with its escapes still in it — see the module docs.
pub const GUIDE_PREFIX_COLUMNS: usize = 3;

/// The label in front of the search box.
pub const SEARCH: &str = "Search:";

/// What a search matching nothing says, in place of the list.
pub const NO_MATCHES: &str = "No matches found";

/// `autocomplete`'s instruction footer. The keys are dim and the verbs are not.
pub const INSTRUCTIONS: [(&str, &str); 3] = [
	("↑/↓", " to select"),
	("Enter:", " confirm"),
	("Type:", " to search"),
];

/// `autocompleteMultiselect`'s, whose second entry changes with the mode — `Space/Tab:` once the
/// arrows have been used, `Tab:` until then, because space is a space while you are still typing.
pub const MULTI_INSTRUCTIONS: [(&str, &str); 4] = [
	("↑/↓", " to navigate"),
	("Tab:", " select"),
	("Enter:", " confirm"),
	("Type:", " to search"),
];

/// The second instruction's key once the arrows have been used.
pub const NAVIGATING_KEY: &str = "Space/Tab:";

/// The separator upstream joins the instructions with.
pub const INSTRUCTION_SEPARATOR: &str = " • ";

/// `autocompleteMultiselect`'s `required` message. Not `multiselect`'s, which ends in a period and
/// says "option" — the two prompts were written apart and word it differently.
pub const REQUIRED_ERROR: &str = "Please select at least one item";

/// `required`, as `autocompleteMultiselect` writes it: at least one option, or the message above.
pub fn required<T>(value: Option<&Vec<T>>) -> Option<String> {
	value
		.is_none_or(|values| values.is_empty())
		.then(|| REQUIRED_ERROR.to_string())
}

/// `getFilteredOption`: the filter both Prompts install when the caller does not.
///
/// Case-insensitive, and it looks at three things — the label, the hint, and the value printed —
/// so a search can find an option by a word that is nowhere on the screen. An empty search matches
/// everything.
///
/// `@clack/core` has a `defaultFilter` of its own that looks only at the label, but neither Prompt
/// can reach it: both pass `opts.filter ?? getFilteredOption`, so the fallback below the fallback
/// is dead from here.
pub fn default_filter<T: Display>(search: &str, option: &SelectOption<T>) -> bool {
	if search.is_empty() {
		return true;
	}
	let term = search.to_lowercase();
	option.label().to_lowercase().contains(&term)
		|| option
			.hint()
			.unwrap_or_default()
			.to_lowercase()
			.contains(&term)
		|| option.value().to_string().to_lowercase().contains(&term)
}

/// `FilterFunction`: what a search matches, over one option at a time.
pub type Filter<T> = dyn Fn(&str, &SelectOption<T>) -> bool;

/// `options: (this: AutocompletePrompt<T>) => T[]`: the list, worked out from the search text every
/// time it is asked for. `FnMut` because upstream's closes over a Prompt it can read and write.
pub type Provider<T> = dyn FnMut(&str) -> Vec<SelectOption<T>>;

/// The state behind both Prompts: a list, a search over it, and what has been chosen.
pub struct AutocompleteState<T> {
	options: Vec<SelectOption<T>>,
	/// `filteredOptions`, as positions in [`options`](Self::options) rather than a second list.
	filtered: Vec<usize>,
	multiple: bool,
	is_navigating: bool,
	selected: Vec<T>,
	focused: Option<T>,
	cursor: usize,
	/// `#lastUserInput`: what the filter was last run against, so that a keypress which leaves the
	/// text alone leaves the list alone too.
	last_user_input: String,
	/// `this.userInput`. A state is told the text and cannot ask for it, and the tab branch needs to
	/// know what is in the field before it decides whether to fill it.
	input: String,
	/// `initialValue`, kept because the answer depends on `multiple`, which is set after this.
	initial: Option<Vec<T>>,
	/// `#filterFn`, which is genuinely absent for a Prompt whose options are a function: the
	/// constructor writes `typeof opts.options === 'function' ? opts.filter : (opts.filter ??
	/// defaultFilter)`, so only the array form gets a fallback. `None` filters nothing — see
	/// [`Self::with_options_fn`].
	filter: Option<Box<Filter<T>>>,
	/// The `options` getter where upstream was handed a function rather than an array. Called again
	/// wherever upstream reads the getter, because that is what re-reads a filesystem.
	provider: Option<Box<Provider<T>>>,
	placeholder: Option<String>,
	/// The pending `_setUserInput` of the tab branch — see [`PromptState::sets_user_input`].
	fill: Option<String>,
}

impl<T: Clone + PartialEq + Display + 'static> AutocompleteState<T> {
	/// The list, under [`default_filter`].
	pub fn new(options: Vec<SelectOption<T>>) -> Self {
		Self::with_filter(options, default_filter)
	}
}

impl<T: Clone + PartialEq> AutocompleteState<T> {
	/// The list, under a filter of the caller's own — upstream's `opts.filter`.
	pub fn with_filter(
		options: Vec<SelectOption<T>>,
		filter: impl Fn(&str, &SelectOption<T>) -> bool + 'static,
	) -> Self {
		Self::build(options, Some(Box::new(filter)), None)
	}

	/// The list as a function of the search text — upstream's `options: () => T[]`, which only
	/// [`crate::path`] passes.
	///
	/// It is not the array form with a callback in front of it. A Prompt built this way has **no
	/// filter at all**: upstream's constructor gives the fallback filter to the array form only, so
	/// everything the function returns is shown and the narrowing is the function's own job. The
	/// function is called again wherever upstream reads its `options` getter — once on the way in,
	/// and once for every change to the text.
	pub fn with_options_fn(provider: impl FnMut(&str) -> Vec<SelectOption<T>> + 'static) -> Self {
		let mut provider = Box::new(provider);
		// `super(opts)` runs before `initialUserInput` is applied, so the first call sees an empty
		// field however the Prompt is about to be seeded.
		let options = provider("");
		Self::build(options, None, Some(provider))
	}

	fn build(
		options: Vec<SelectOption<T>>,
		filter: Option<Box<Filter<T>>>,
		provider: Option<Box<Provider<T>>>,
	) -> Self {
		let filtered = (0..options.len()).collect();
		let mut state = Self {
			options,
			filtered,
			multiple: false,
			is_navigating: false,
			selected: Vec::new(),
			focused: None,
			cursor: 0,
			last_user_input: String::new(),
			input: String::new(),
			initial: None,
			filter,
			provider,
			placeholder: None,
			fill: None,
		};
		state.settle();
		state
	}

	/// `multiple`: whether space and tab tick options rather than choosing one.
	pub fn with_multiple(mut self, multiple: bool) -> Self {
		self.multiple = multiple;
		self.settle();
		self
	}

	/// `initialValue`, which is an array on both Prompts. A single `autocomplete` keeps the first of
	/// them and drops the rest.
	pub fn with_initial_values(mut self, values: impl IntoIterator<Item = T>) -> Self {
		self.initial = Some(values.into_iter().collect());
		self.settle();
		self
	}

	/// `placeholder`. The state needs it as well as the widget: tab fills the field with it, but only
	/// when it matches something, so that the text it types is text the Prompt can answer with.
	pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
		self.placeholder = Some(placeholder.into());
		self
	}

	pub fn options(&self) -> &[SelectOption<T>] {
		&self.options
	}

	/// The options the search left, in order.
	pub fn filtered(&self) -> impl Iterator<Item = &SelectOption<T>> {
		self.filtered.iter().map(|index| &self.options[*index])
	}

	/// How many of them there are, which is what the `(n matches)` counter reports.
	pub fn matches(&self) -> usize {
		self.filtered.len()
	}

	/// Where the cursor sits *in the filtered list*.
	pub fn cursor(&self) -> usize {
		self.cursor
	}

	/// `isNavigating`: whether the arrows have been used since the last thing was typed. It decides
	/// whether the search box draws a text cursor and what the second instruction says.
	pub fn is_navigating(&self) -> bool {
		self.is_navigating
	}

	/// `focusedValue`: the option the cursor is on, as a value. None where the filtered list is empty
	/// or the cursor landed on something disabled.
	pub fn focused(&self) -> Option<&T> {
		self.focused.as_ref()
	}

	pub fn selected(&self) -> &[T] {
		&self.selected
	}

	pub fn multiple(&self) -> bool {
		self.multiple
	}

	/// `getSelectedOptions`: the chosen options, in the list's order rather than the order they were
	/// chosen in.
	pub fn chosen(&self) -> impl Iterator<Item = &SelectOption<T>> {
		self.options
			.iter()
			.filter(|option| self.selected.contains(option.value()))
	}

	/// Whether this option is one of the chosen. Only a multiple Prompt draws the answer.
	pub fn is_selected(&self, option: &SelectOption<T>) -> bool {
		self.selected.contains(option.value())
	}

	/// The constructor's selection and cursor, recomputed.
	///
	/// Upstream settles once, having been handed everything at once. The builders here arrive one at
	/// a time and `multiple` decides what `initialValue` means, so this runs again after each — it
	/// reads only the three fields it is given and is the same answer however many times it is asked.
	fn settle(&mut self) {
		self.selected.clear();
		self.cursor = 0;

		let initial = match &self.initial {
			Some(values) if self.multiple => values.clone(),
			Some(values) => values.iter().take(1).cloned().collect(),
			// A single `autocomplete` opens on its first option, chosen. A multiple one opens on
			// nothing, because upstream's `else` branch is guarded by `!this.multiple`.
			None if !self.multiple => self
				.options
				.first()
				.map(|option| vec![option.value().clone()])
				.unwrap_or_default(),
			None => Vec::new(),
		};

		for value in initial {
			// A value the list does not hold selects nothing and moves nothing, which is what
			// `findIndex(…) !== -1` says.
			if let Some(at) = self.options.iter().position(|o| o.value() == &value) {
				self.toggle_selected(value);
				self.cursor = at;
			}
		}

		self.focused = self.options.get(self.cursor).map(|o| o.value().clone());
	}

	/// `toggleSelected`. Nothing can be chosen while the search matches nothing, whatever the cursor
	/// is still pointing at.
	///
	/// That guard is upstream's and is unreachable here: every caller passes the focused value, and
	/// the focus is `None` wherever the filtered list is empty. Kept because it is upstream's, and
	/// because the day something else calls this it is the answer that was wanted.
	fn toggle_selected(&mut self, value: T) {
		if self.filtered.is_empty() {
			return;
		}
		if !self.multiple {
			self.selected = vec![value];
		} else if self.selected.contains(&value) {
			self.selected.retain(|held| held != &value);
		} else {
			self.selected.push(value);
		}
	}

	/// Whether an option of the filtered list is disabled, by its position in it.
	fn disabled(&self) -> impl Fn(&usize) -> bool {
		let options = &self.options;
		move |index: &usize| options[*index].disabled()
	}
}

impl<T: Clone + PartialEq> PromptState for AutocompleteState<T> {
	/// The selection, for both Prompts. A single `autocomplete` answers with the first of it —
	/// upstream's `normalisedValue`, which reads `selectedValues[0]` on the way out.
	type Value = Vec<T>;

	/// `_isActionKey`: tab always, and space once the arrows have been used and there is a character
	/// behind it. A space emitted with no character — which is how a test sends one — is not one,
	/// so the editor keeps whatever readline did with it.
	fn is_action_key(&self, s: Option<&str>, key: &Key) -> bool {
		s == Some("\t")
			|| (self.multiple
				&& self.is_navigating
				&& key.name == Some(KeyName::Char(' '))
				&& !matches!(s, None | Some("")))
	}

	/// `#onUserInputChanged`: the search text changed, so the list is filtered again and the cursor
	/// re-found by value rather than by position.
	fn user_input(&mut self, input: &str) {
		self.input = input.to_string();
		// `#lastUserInput`. Everything below is derived from the text, so running it again on text
		// that has not changed would land in the same place — this is upstream's own short-circuit
		// rather than a correctness guard.
		if input == self.last_user_input {
			return;
		}
		self.last_user_input = input.to_string();

		// `const options = this.options` — the getter, so a Prompt whose options are a function asks
		// it again here, against the text that has just changed. The list is replaced rather than
		// added to: `filteredOptions` upstream holds the options themselves, and holding positions
		// into a list means the list they index has to be this one.
		if let Some(provider) = &mut self.provider {
			self.options = provider(input);
		}

		let filtered: Vec<usize> = match &self.filter {
			// `if (value && this.#filterFn)`. No filter, or nothing typed, and the filtered list is
			// everything there is.
			Some(filter) if !input.is_empty() => (0..self.options.len())
				.filter(|index| filter(input, &self.options[*index]))
				.collect(),
			_ => (0..self.options.len()).collect(),
		};
		self.filtered = filtered;

		// `getCursorForValue`: keep the same option if the filter kept it, and start again if not.
		let at = match &self.focused {
			Some(_) if self.filtered.is_empty() => 0,
			Some(value) => self
				.filtered
				.iter()
				.position(|index| self.options[*index].value() == value)
				.unwrap_or(0),
			None => 0,
		};
		// A delta of zero: stay where you are unless that option is disabled, then walk forwards.
		self.cursor = find_cursor(at, 0, &self.filtered, self.disabled());

		self.focused = match self.filtered.get(self.cursor) {
			Some(index) if !self.options[*index].disabled() => {
				Some(self.options[*index].value().clone())
			}
			_ => None,
		};

		if !self.multiple {
			match self.focused.clone() {
				Some(value) => self.toggle_selected(value),
				None => self.selected.clear(),
			}
		}
	}

	/// `#onKey`, in its own order — which is why none of this is in
	/// [`cursor`](PromptState::cursor). Upstream reads the arrows inside its `key` listener and
	/// subscribes nothing to `cursor` at all, so a `select`'s split between the two would put the
	/// walk before the branch that decides whether the Prompt is navigating.
	fn key(&mut self, _s: Option<&str>, key: &Key) {
		let up = key.name == Some(KeyName::Up);
		let down = key.name == Some(KeyName::Down);
		let tab = key.name == Some(KeyName::Tab);
		let space = key.name == Some(KeyName::Char(' '));

		// Tab on an empty field fills it from the placeholder — but only when the placeholder finds
		// something, so that the Prompt can answer with what it has just typed for you. `'\t'` is the
		// other empty: readline has already put the tab in the field by the time this runs.
		let empty = self.input.is_empty() || self.input == "\t";
		let fills = self.placeholder.clone().is_some_and(|placeholder| {
			// `const options = this.options` again — and it is read here for this one question, so a
			// function's answer is looked at and thrown away rather than becoming the list. Asked
			// only when there is a placeholder to ask about, because asking is a filesystem call.
			let fresh = self
				.provider
				.as_mut()
				.map(|provider| provider(&self.input))
				.unwrap_or_default();
			let options = match &self.provider {
				Some(_) => fresh.as_slice(),
				None => self.options.as_slice(),
			};
			!placeholder.is_empty()
				&& options.iter().any(|option| {
					!option.disabled()
						// `this.#filterFn ? this.#filterFn(placeholder, opt) : true`
						&& self
							.filter
							.as_ref()
							.is_none_or(|filter| filter(&placeholder, option))
				})
		});
		if tab && empty && fills {
			self.fill = self.placeholder.clone();
			self.is_navigating = false;
			return;
		}

		if up || down {
			self.cursor = find_cursor(
				self.cursor,
				if up { -1 } else { 1 },
				&self.filtered,
				self.disabled(),
			);
			self.focused = self
				.filtered
				.get(self.cursor)
				.map(|index| self.options[*index].value().clone());
			if !self.multiple {
				// `this.selectedValues = [this.focusedValue]`, which is an array holding `undefined`
				// where there is no focused value. Here it is an empty one: the two are the same
				// question asked of `selectedValues[0]`, and the same nothing to everything that
				// draws the selection.
				self.selected = self.focused.clone().into_iter().collect();
			}
			self.is_navigating = true;
		} else if key.name == Some(KeyName::Return) {
			// `this.value = normalisedValue(…)`. The selection already is the value here, so there is
			// nothing to copy across.
		} else if self.multiple {
			match self.focused.clone() {
				Some(value) if tab || (self.is_navigating && space) => self.toggle_selected(value),
				_ => self.is_navigating = false,
			}
		} else {
			if let Some(value) = self.focused.clone() {
				self.selected = vec![value];
			}
			self.is_navigating = false;
		}
	}

	fn sets_user_input(&mut self) -> Option<String> {
		self.fill.take()
	}

	/// The selection as it stands, which is what `autocompleteMultiselect`'s own validator reads —
	/// it closes over `prompt.selectedValues` rather than over the value, so it sees the selection
	/// before `return` has turned it into one.
	fn value(&self) -> Option<&Vec<T>> {
		Some(&self.selected)
	}
}

/// `userInputWithCursor`: the search text with the cursor drawn into it.
///
/// The bug the module docs name lives here. Whether the cursor is past the end is asked of
/// `_cursor`, the position in the text; where to slice is asked of `this.cursor`, the position in
/// the option list. They are the same number only by coincidence, so a text cursor moved off the end
/// with the left arrow highlights whichever character the list's cursor indexes — and the two spans
/// on either side of it are the text cut at that same wrong place.
fn search_input<T: Clone + PartialEq>(
	prompt: &Prompt<AutocompleteState<T>>,
	theme: &Theme,
) -> Vec<Span> {
	let styles = &theme.styles;
	let text = prompt.user_input();

	if text.is_empty() {
		return vec![Span::styled("_", styles.placeholder_empty)];
	}
	// UTF-16 on both sides, as upstream's `_cursor >= userInput.length` is — and the same answer as
	// bytes on both sides would give, since either way it is asking whether the caret is at the end.
	if prompt.cursor_utf16() >= text.encode_utf16().count() {
		return vec![Span::raw(text), Span::raw(CURSOR_BLOCK)];
	}

	// `slice` on a string clamps rather than panicking, and the index is a UTF-16 offset.
	let at = utf16_offset(text, prompt.state().cursor());
	let end = utf16_offset(text, prompt.state().cursor() + 1);
	let mut spans = Vec::with_capacity(3);
	if at > 0 {
		spans.push(Span::raw(&text[..at]));
	}
	spans.push(Span::styled(&text[at..end], styles.cursor));
	if end < text.len() {
		spans.push(Span::raw(&text[end..]));
	}
	spans
}

/// A UTF-16 offset as a byte offset, clamped to the end of the string like `String.prototype.slice`.
fn utf16_offset(text: &str, units: usize) -> usize {
	let mut seen = 0;
	for (at, c) in text.char_indices() {
		if seen >= units {
			return at;
		}
		seen += c.len_utf16();
	}
	text.len()
}

/// ` (n matches)`: how much of the list the search left, drawn only once it has left some of it out.
fn matches_counted<T: Clone + PartialEq>(state: &AutocompleteState<T>) -> Option<String> {
	let matched = state.matches();
	(matched != state.options().len()).then(|| {
		let plural = if matched == 1 { "" } else { "es" };
		format!(" ({matched} match{plural})")
	})
}

/// The `↑/↓ to select • …` row, joined from a table of keys and verbs.
fn instructions(theme: &Theme, entries: &[(&str, &str)]) -> Vec<Span> {
	let mut spans = Vec::new();
	for (index, (key, verb)) in entries.iter().enumerate() {
		if index > 0 {
			spans.push(Span::raw(INSTRUCTION_SEPARATOR));
		}
		spans.push(Span::styled(*key, theme.styles.instruction_key));
		spans.push(Span::raw(*verb));
	}
	spans
}

/// An `autocomplete` Prompt drawn as a Frame — the `render` callback of `@clack/prompts`'
/// `autocomplete()`.
pub struct AutocompleteWidget<'a, T: Clone + PartialEq> {
	prompt: &'a Prompt<AutocompleteState<T>>,
	message: &'a str,
	theme: &'a Theme,
	columns: usize,
	rows: usize,
	max_items: Option<usize>,
	placeholder: Option<&'a str>,
	with_guide: Option<bool>,
}

impl<'a, T: Clone + PartialEq> AutocompleteWidget<'a, T> {
	pub fn new(prompt: &'a Prompt<AutocompleteState<T>>, message: &'a str) -> Self {
		Self {
			prompt,
			message,
			theme: &THEME,
			columns: 80,
			rows: 20,
			max_items: None,
			placeholder: None,
			with_guide: None,
		}
	}

	pub fn with_theme(mut self, theme: &'a Theme) -> Self {
		self.theme = theme;
		self
	}

	pub fn with_columns(mut self, columns: usize) -> Self {
		self.columns = columns;
		self
	}

	pub fn with_rows(mut self, rows: usize) -> Self {
		self.rows = rows;
		self
	}

	pub fn with_max_items(mut self, max_items: usize) -> Self {
		self.max_items = Some(max_items);
		self
	}

	/// `placeholder`: what the search box shows while it is empty. Set it on the state too, or tab
	/// will have nothing to fill the field with.
	pub fn with_placeholder(mut self, placeholder: &'a str) -> Self {
		self.placeholder = Some(placeholder);
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
		let mut header = plain_title(self.theme, self.message, status, guide);

		match status {
			// One row: the Guide, and the labels chosen beside it. Neither branch wraps, and neither
			// closes with a second bar.
			Status::Submit | Status::Cancel => {
				for line in header {
					frame.push(line);
				}
				let mut line = Line::blank();
				if guide {
					line.push(Span::styled(symbols.bar, styles.guide));
				}
				let settled = if status == Status::Submit {
					let labels: Vec<&str> = state.chosen().map(SelectOption::label).collect();
					(!labels.is_empty()).then(|| Span::styled(labels.join(", "), styles.submitted))
				} else {
					let input = self.prompt.user_input();
					(!input.is_empty()).then(|| Span::styled(input, styles.cancelled))
				};
				if let Some(settled) = settled {
					line.push(Span::raw("  "));
					line.push(settled);
				}
				// A label with a break in it is written straight into the row, so the row is several.
				for paragraph in line.paragraphs() {
					frame.push(paragraph);
				}
			}

			_ => {
				let bar_style = if status == Status::Error {
					styles.guide_error
				} else {
					styles.guide_active
				};
				let prefix = |line: &mut Line| {
					if guide {
						line.push(Span::styled(symbols.bar, bar_style));
						line.push(Span::raw("  "));
					}
				};

				// `guidePrefix.trimEnd()`: the same bar without its two spaces. Pushed inside the
				// `if`, so an unguided Prompt has no row here at all.
				if guide {
					header.push(Line::from(Span::styled(symbols.bar, bar_style)));
				}

				let mut search = Line::blank();
				prefix(&mut search);
				search.push(Span::styled(SEARCH, styles.hint));
				for span in self.search(state) {
					search.push(span);
				}
				if let Some(counted) = matches_counted(state) {
					search.push(Span::styled(counted, styles.hint));
				}
				header.push(search);

				if state.matches() == 0 && !self.prompt.user_input().is_empty() {
					let mut line = Line::blank();
					prefix(&mut line);
					line.push(Span::styled(NO_MATCHES, styles.error));
					header.push(line);
				}
				if status == Status::Error {
					let mut line = Line::blank();
					prefix(&mut line);
					line.push(Span::styled(self.prompt.error(), styles.error));
					header.push(line);
				}

				let mut footer = Line::blank();
				prefix(&mut footer);
				for span in instructions(self.theme, &INSTRUCTIONS) {
					footer.push(span);
				}
				// `guidePrefixEnd` is the empty string without a Guide, and an empty string is still
				// a row.
				let mut end = Line::blank();
				if guide {
					end.push(Span::styled(symbols.bar_end, bar_style));
				}
				let footers = [footer, end];

				let row_padding = header.len() + footers.len();
				for line in header {
					frame.push(line);
				}

				// A search that matched nothing skips the list rather than drawing an empty one —
				// which `limitOptions` would also do, given nothing to draw. Upstream's guard, kept
				// because upstream has it and not because the two differ.
				if state.matches() > 0 {
					let indices: Vec<usize> = (0..state.matches()).collect();
					let mut limit = LimitOptions::new(&indices, state.cursor())
						.with_columns(self.columns)
						.with_rows(self.rows)
						.with_column_padding(if guide { GUIDE_PREFIX_COLUMNS } else { 0 })
						.with_row_padding(row_padding)
						.with_theme(self.theme);
					if let Some(max_items) = self.max_items {
						limit = limit.with_max_items(max_items);
					}
					for row in limit.lines(|index, active| self.option(*index, active)) {
						let mut line = Line::blank();
						prefix(&mut line);
						for span in row.spans {
							line.push(span);
						}
						frame.push(line);
					}
				}

				for line in footers {
					frame.push(line);
				}
			}
		}

		frame
	}

	/// The search box: the placeholder, the text as typed, or the text with a cursor in it.
	///
	/// Navigating draws the text plainly — the cursor belongs to the list while the arrows are
	/// driving it — and an empty search with a placeholder draws the placeholder the same way. Both
	/// are dim, and both draw *nothing at all* rather than a leading space when the text is empty.
	fn search(&self, state: &AutocompleteState<T>) -> Vec<Span> {
		let styles = &self.theme.styles;
		let input = self.prompt.user_input();
		let showing = input.is_empty() && self.placeholder.is_some();

		if state.is_navigating() || showing {
			let text = if showing {
				self.placeholder.unwrap_or_default()
			} else {
				input
			};
			if text.is_empty() {
				return Vec::new();
			}
			return vec![Span::raw(" "), Span::styled(text, styles.placeholder)];
		}

		let mut spans = vec![Span::raw(" ")];
		spans.extend(search_input(self.prompt, self.theme));
		spans
	}

	/// `opt(option, state)`: one option of the filtered list, as the rows it occupies.
	///
	/// Three branches, `select`'s three — except that the active one carries the hint only when the
	/// option is also the focused one, which is the same option except where the cursor has landed on
	/// something disabled.
	fn option(&self, index: usize, active: bool) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let state = self.prompt.state();
		let option = state
			.filtered()
			.nth(index)
			.expect("the filtered list holds this index");

		let (radio, radio_style, label_style) = if option.disabled() {
			(
				symbols.radio_inactive,
				styles.option_disabled,
				styles.option_disabled_label,
			)
		} else if active {
			(symbols.radio_active, styles.radio_selected, styles.message)
		} else {
			(
				symbols.radio_inactive,
				styles.radio_unselected,
				styles.option_unselected,
			)
		};
		// Upstream builds the hint for every branch and interpolates it into one: the active option
		// carries it, and only when it is also the focused one — the same option, except where the
		// cursor has landed on something disabled.
		let hint = option
			.hint()
			.filter(|_| active && !option.disabled() && state.focused() == Some(option.value()));

		rows(
			option.label(),
			radio,
			radio_style,
			label_style,
			hint,
			styles.hint,
		)
	}
}

/// An `autocompleteMultiselect` Prompt drawn as a Frame.
pub struct AutocompleteMultiSelectWidget<'a, T: Clone + PartialEq> {
	prompt: &'a Prompt<AutocompleteState<T>>,
	message: &'a str,
	theme: &'a Theme,
	columns: usize,
	rows: usize,
	max_items: Option<usize>,
	placeholder: Option<&'a str>,
	with_guide: Option<bool>,
}

impl<'a, T: Clone + PartialEq> AutocompleteMultiSelectWidget<'a, T> {
	pub fn new(prompt: &'a Prompt<AutocompleteState<T>>, message: &'a str) -> Self {
		Self {
			prompt,
			message,
			theme: &THEME,
			columns: 80,
			rows: 20,
			max_items: None,
			placeholder: None,
			with_guide: None,
		}
	}

	pub fn with_theme(mut self, theme: &'a Theme) -> Self {
		self.theme = theme;
		self
	}

	pub fn with_columns(mut self, columns: usize) -> Self {
		self.columns = columns;
		self
	}

	pub fn with_rows(mut self, rows: usize) -> Self {
		self.rows = rows;
		self
	}

	pub fn with_max_items(mut self, max_items: usize) -> Self {
		self.max_items = Some(max_items);
		self
	}

	pub fn with_placeholder(mut self, placeholder: &'a str) -> Self {
		self.placeholder = Some(placeholder);
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
		let mut header = plain_title(self.theme, self.message, status, guide);

		match status {
			// A count rather than the labels, and — unlike every other settled Frame in clack — the
			// two spaces after the bar are written whether or not anything follows them.
			Status::Submit | Status::Cancel => {
				for line in header {
					frame.push(line);
				}
				let mut line = Line::blank();
				if guide {
					line.push(Span::styled(symbols.bar, styles.guide));
					line.push(Span::raw("  "));
				}
				if status == Status::Submit {
					let count = state.selected().len();
					line.push(Span::styled(
						format!("{count} items selected"),
						styles.submitted,
					));
				} else {
					line.push(Span::styled(self.prompt.user_input(), styles.cancelled));
				}
				for paragraph in line.paragraphs() {
					frame.push(paragraph);
				}
			}

			_ => {
				let bar_style = if status == Status::Error {
					styles.guide_error
				} else {
					styles.guide_active
				};
				let prefix = |line: &mut Line| {
					if guide {
						line.push(Span::styled(symbols.bar, bar_style));
						line.push(Span::raw("  "));
					}
				};

				// `${title}${hasGuide ? bar : ''}`, split on the newline `title` ends with: the bar
				// has a row of its own, and so does the empty string standing in for it.
				header.push(if guide {
					Line::from(Span::styled(symbols.bar, bar_style))
				} else {
					Line::blank()
				});

				let mut search = Line::blank();
				prefix(&mut search);
				search.push(Span::styled(SEARCH, styles.hint));
				// One space, always — where `autocomplete` writes it as part of the text and drops it
				// with an empty one.
				search.push(Span::raw(" "));
				for span in self.search(state) {
					search.push(span);
				}
				if let Some(counted) = matches_counted(state) {
					search.push(Span::styled(counted, styles.hint));
				}
				header.push(search);

				if state.matches() == 0 && !self.prompt.user_input().is_empty() {
					let mut line = Line::blank();
					prefix(&mut line);
					line.push(Span::styled(NO_MATCHES, styles.error));
					header.push(line);
				}
				if status == Status::Error {
					let mut line = Line::blank();
					prefix(&mut line);
					line.push(Span::styled(self.prompt.error(), styles.error));
					header.push(line);
				}

				let mut keys = MULTI_INSTRUCTIONS;
				if state.is_navigating() {
					keys[1].0 = NAVIGATING_KEY;
				}
				let mut footer = Line::blank();
				prefix(&mut footer);
				for span in instructions(self.theme, &keys) {
					footer.push(span);
				}
				let mut end = Line::blank();
				if guide {
					end.push(Span::styled(symbols.bar_end, bar_style));
				}
				let footers = [footer, end];

				let row_padding = header.len() + footers.len();
				for line in header {
					frame.push(line);
				}

				// No `columnPadding`, so the options are wrapped as though the bar beside them were
				// not there. See the module docs — `autocomplete` passes three.
				let indices: Vec<usize> = (0..state.matches()).collect();
				let mut limit = LimitOptions::new(&indices, state.cursor())
					.with_columns(self.columns)
					.with_rows(self.rows)
					.with_row_padding(row_padding)
					.with_theme(self.theme);
				if let Some(max_items) = self.max_items {
					limit = limit.with_max_items(max_items);
				}
				for row in limit.lines(|index, active| self.option(*index, active)) {
					let mut line = Line::blank();
					prefix(&mut line);
					for span in row.spans {
						line.push(span);
					}
					frame.push(line);
				}

				for line in footers {
					frame.push(line);
				}
			}
		}

		frame
	}

	/// The search box. `autocomplete`'s, without the leading space and without the empty case — this
	/// one draws a dim nothing rather than skipping the span.
	fn search(&self, state: &AutocompleteState<T>) -> Vec<Span> {
		let styles = &self.theme.styles;
		let input = self.prompt.user_input();
		let showing = input.is_empty() && self.placeholder.is_some();

		if state.is_navigating() || showing {
			let text = if showing {
				self.placeholder.unwrap_or_default()
			} else {
				input
			};
			return vec![Span::styled(text, styles.placeholder)];
		}
		search_input(self.prompt, self.theme)
	}

	/// `formatOption`: a checkbox, and a label whose dimming says where the cursor is.
	///
	/// Four branches to `multiselect`'s five: a ticked option the cursor is on and one it is not are
	/// drawn the same, because the box answers only the first question and the label only the second.
	fn option(&self, index: usize, active: bool) -> Vec<Line> {
		let styles = &self.theme.styles;
		let symbols = &self.theme.symbols;
		let state = self.prompt.state();
		let option = state
			.filtered()
			.nth(index)
			.expect("the filtered list holds this index");

		if option.disabled() {
			return rows(
				option.label(),
				symbols.checkbox_inactive,
				styles.option_disabled,
				styles.option_disabled_label,
				None,
				styles.hint,
			);
		}

		let selected = state.is_selected(option);
		let (box_symbol, box_style) = if selected {
			(symbols.checkbox_selected, styles.checkbox_selected)
		} else {
			(symbols.checkbox_inactive, styles.checkbox_inactive)
		};
		let label_style = if active {
			styles.message
		} else {
			styles.option_selected
		};
		let hint = option
			.hint()
			.filter(|_| active && state.focused() == Some(option.value()));

		rows(
			option.label(),
			box_symbol,
			box_style,
			label_style,
			hint,
			styles.hint,
		)
	}
}

/// One option as the rows its label occupies: the box on the first, the hint after the last.
///
/// The same shape `select` and `multiselect` build, and built here once for the two widgets rather
/// than twice. It is not shared with them: theirs style each row of a multi-line label separately
/// and this one does too, but the branches that decide the styles are three different tables.
fn rows(
	label: &str,
	symbol: &str,
	symbol_style: ratatui_core::style::Style,
	label_style: ratatui_core::style::Style,
	hint: Option<&str>,
	hint_style: ratatui_core::style::Style,
) -> Vec<Line> {
	let parts: Vec<&str> = label.split('\n').collect();
	let last = parts.len() - 1;
	parts
		.into_iter()
		.enumerate()
		.map(|(index, text)| {
			let mut line = Line::blank();
			if index == 0 {
				line.push(Span::styled(symbol, symbol_style));
				line.push(Span::raw(" "));
			}
			line.push(Span::styled(text, label_style));
			if index == last {
				if let Some(hint) = hint {
					line.push(Span::styled(format!(" ({hint})"), hint_style));
				}
			}
			line
		})
		.collect()
}

/// The default Theme, as somewhere to borrow from.
static THEME: Theme = Theme::clack();

impl<T: Clone + PartialEq> Widget for &AutocompleteWidget<'_, T> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		(&self.frame()).render(area, buf);
	}
}

impl<T: Clone + PartialEq> Widget for &AutocompleteMultiSelectWidget<'_, T> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		(&self.frame()).render(area, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::prompt::Outcome;

	fn options(pairs: &[(&str, &str)]) -> Vec<SelectOption<String>> {
		pairs
			.iter()
			.map(|(value, label)| SelectOption::labelled(value.to_string(), *label))
			.collect()
	}

	fn fruit() -> Vec<SelectOption<String>> {
		options(&[
			("apple", "Apple"),
			("banana", "Banana"),
			("cherry", "Cherry"),
			("grape", "Grape"),
			("orange", "Orange"),
		])
	}

	fn open(state: AutocompleteState<String>) -> Prompt<AutocompleteState<String>> {
		Prompt::new(state)
	}

	/// The options function is asked against the text as it stands, starting with no text at all —
	/// `super(opts)` runs before `initialUserInput` does, so the first list a Prompt holds is the one
	/// for an empty field however it is about to be seeded.
	#[test]
	fn a_function_is_asked_for_the_list_and_asked_again_as_the_text_changes() {
		use std::cell::RefCell;
		use std::rc::Rc;

		let asked: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let seen = Rc::clone(&asked);
		let state = AutocompleteState::with_options_fn(move |input| {
			seen.borrow_mut().push(input.to_string());
			options(&[(input, input)])
		});

		assert_eq!(asked.borrow().as_slice(), [""]);
		assert_eq!(state.options().len(), 1);
		assert_eq!(state.options()[0].value(), "");

		let mut prompt = open(state);
		typed(&mut prompt, "ab");
		assert_eq!(asked.borrow().as_slice(), ["", "a", "ab"]);
		assert_eq!(prompt.state().options()[0].value(), "ab");
	}

	/// A function's options are not filtered. Upstream gives the fallback filter to the array form
	/// only, so everything the function returns is shown however little of it the text matches.
	#[test]
	fn a_function_gets_no_filter_at_all() {
		let state = AutocompleteState::with_options_fn(|_| fruit());
		let mut prompt = open(state);
		typed(&mut prompt, "zzz");
		assert_eq!(prompt.state().matches(), 5);
	}

	fn press(prompt: &mut Prompt<AutocompleteState<String>>, name: KeyName) {
		prompt.key(None, &Key::named(name));
	}

	/// A space as upstream's own tests send it: named, with no character behind it.
	fn space(prompt: &mut Prompt<AutocompleteState<String>>) {
		prompt.key(Some(""), &Key::named(KeyName::Char(' ')));
	}

	fn typed(prompt: &mut Prompt<AutocompleteState<String>>, text: &str) {
		for c in text.chars() {
			prompt.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
		}
	}

	fn answer(prompt: &Prompt<AutocompleteState<String>>) -> Vec<String> {
		match prompt.outcome() {
			Some(Outcome::Submitted(Some(values))) => values.clone(),
			_ => Vec::new(),
		}
	}

	fn labels(state: &AutocompleteState<String>) -> Vec<&str> {
		state.filtered().map(SelectOption::label).collect()
	}

	#[test]
	fn a_single_autocomplete_opens_on_its_first_option_already_chosen() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt), ["apple"]);
	}

	#[test]
	fn a_multiple_one_opens_on_nothing() {
		let mut prompt = open(AutocompleteState::new(fruit()).with_multiple(true));
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt), Vec::<String>::new());
	}

	#[test]
	fn an_initial_value_is_chosen_and_walked_to() {
		let state = AutocompleteState::new(fruit()).with_initial_values(["cherry".to_string()]);
		assert_eq!(state.cursor(), 2);
		let mut prompt = open(state);
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt), ["cherry"]);
	}

	/// `findIndex(…) !== -1`: a value the list does not hold chooses nothing and moves nothing.
	#[test]
	fn an_initial_value_the_list_does_not_hold_is_ignored() {
		let state = AutocompleteState::new(fruit()).with_initial_values(["kiwi".to_string()]);
		assert_eq!(state.cursor(), 0);
		assert!(state.selected().is_empty());
	}

	#[test]
	fn typing_filters_the_list_and_counts_what_is_left() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		typed(&mut prompt, "an");
		assert_eq!(labels(prompt.state()), ["Banana", "Orange"]);
		assert_eq!(prompt.state().matches(), 2);
	}

	/// The default filter reads the value and the hint as well as the label, so a search can find an
	/// option by text that is nowhere on the screen.
	#[test]
	fn the_default_filter_looks_past_the_label() {
		let list = vec![
			SelectOption::labelled("nextjs".to_string(), "The React one").with_hint("very meta"),
			SelectOption::labelled("astro".to_string(), "The content one"),
		];
		let mut by_hint = open(AutocompleteState::new(list.clone()));
		typed(&mut by_hint, "meta");
		assert_eq!(labels(by_hint.state()), ["The React one"]);

		let mut by_value = open(AutocompleteState::new(list));
		typed(&mut by_value, "nextj");
		assert_eq!(labels(by_value.state()), ["The React one"]);
	}

	/// The three fields the default filter reads, and the two answers it gives without reading any of
	/// them. Asserted on the function rather than through a Prompt: an empty search never reaches it
	/// from a state, which skips the filter entirely for one.
	#[test]
	fn the_default_filter_matches_everything_on_an_empty_search_and_ignores_case() {
		let option = SelectOption::labelled("Next".to_string(), "Next.js").with_hint("React");
		assert!(default_filter("", &option));
		assert!(default_filter("NEXT.J", &option));
		assert!(default_filter("react", &option));
		assert!(!default_filter("astro", &option));
	}

	#[test]
	fn a_search_is_matched_in_either_case() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		typed(&mut prompt, "AP");
		assert_eq!(labels(prompt.state()), ["Apple", "Grape"]);
	}

	/// `opts.initialValue.slice(0, 1)`: a single `autocomplete` takes the first and drops the rest,
	/// so the cursor lands on the first rather than being walked to the last.
	#[test]
	fn a_single_autocomplete_keeps_only_the_first_initial_value() {
		let state = AutocompleteState::new(fruit())
			.with_initial_values(["apple".to_string(), "cherry".to_string()]);
		assert_eq!(state.cursor(), 0);
		assert_eq!(state.selected(), ["apple"]);
	}

	/// A multiple one keeps all of them, and the cursor ends up on the last that the list holds.
	#[test]
	fn a_multiple_one_keeps_every_initial_value() {
		let state = AutocompleteState::new(fruit())
			.with_multiple(true)
			.with_initial_values(["apple".to_string(), "cherry".to_string()]);
		assert_eq!(state.cursor(), 2);
		assert_eq!(state.selected(), ["apple", "cherry"]);
	}

	/// `_isActionKey` again, from the other side: a space with a character behind it — which is what
	/// a real terminal sends — is taken back out of the field rather than searched for.
	#[test]
	fn a_space_that_ticks_is_not_also_typed() {
		let mut prompt = open(AutocompleteState::new(fruit()).with_multiple(true));
		press(&mut prompt, KeyName::Down);
		prompt.key(Some(" "), &Key::named(KeyName::Char(' ')));
		assert_eq!(prompt.user_input(), "");
		assert_eq!(prompt.state().selected(), ["banana"]);
		assert_eq!(prompt.state().matches(), 5);
	}

	/// `if (value && this.#filterFn)`: an empty search skips the filter rather than asking it, so a
	/// filter that refuses one never sees it.
	#[test]
	fn an_empty_search_is_not_put_to_the_filter_at_all() {
		let state = AutocompleteState::with_filter(fruit(), |search: &str, option| {
			!search.is_empty()
				&& option
					.label()
					.to_lowercase()
					.contains(&search.to_lowercase())
		});
		let mut prompt = open(state);
		typed(&mut prompt, "a");
		assert_eq!(prompt.state().matches(), 4);
		press(&mut prompt, KeyName::Backspace);
		assert_eq!(prompt.user_input(), "");
		assert_eq!(prompt.state().matches(), 5);
	}

	/// `findCursor(valueCursor, 0, …)`: the delta is zero, so the cursor stays where the value put it
	/// — unless that option is disabled, and then it walks forwards off it.
	#[test]
	fn a_filter_that_lands_the_cursor_on_a_disabled_option_walks_off_it() {
		let list = vec![
			SelectOption::labelled("a".to_string(), "Apple"),
			SelectOption::labelled("bd".to_string(), "Banana, but not really").with_disabled(true),
			SelectOption::labelled("b".to_string(), "Banana"),
		];
		let mut prompt = open(AutocompleteState::new(list));
		typed(&mut prompt, "banana");
		assert_eq!(prompt.state().cursor(), 1);
		assert_eq!(prompt.state().focused().map(String::as_str), Some("b"));
	}

	#[test]
	fn a_filter_of_ones_own_replaces_it_entirely() {
		let state = AutocompleteState::with_filter(fruit(), |search, option| {
			option
				.label()
				.to_lowercase()
				.starts_with(&search.to_lowercase())
		});
		let mut prompt = open(state);
		typed(&mut prompt, "a");
		// `Banana` and `Grape` contain an `a` and start with something else.
		assert_eq!(labels(prompt.state()), ["Apple"]);
	}

	#[test]
	fn a_search_that_matches_nothing_leaves_nothing_focused_or_chosen() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		typed(&mut prompt, "z");
		assert_eq!(prompt.state().matches(), 0);
		assert_eq!(prompt.state().focused(), None);
		assert!(prompt.state().selected().is_empty());
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt), Vec::<String>::new());
	}

	/// `getCursorForValue`: the focus is kept by value, so an option that survives the filter keeps
	/// the cursor however far it moved in the list.
	#[test]
	fn the_focus_follows_the_option_rather_than_its_position() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		press(&mut prompt, KeyName::Down);
		press(&mut prompt, KeyName::Down);
		assert_eq!(prompt.state().focused().map(String::as_str), Some("cherry"));
		// `Apple`, `Cherry`, `Grape` and `Orange`, so the focus is at 1 rather than at 2 or 0.
		typed(&mut prompt, "e");
		assert_eq!(prompt.state().cursor(), 1);
		assert_eq!(prompt.state().focused().map(String::as_str), Some("cherry"));
	}

	#[test]
	fn the_arrows_walk_the_filtered_list_and_wrap() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		typed(&mut prompt, "an");
		press(&mut prompt, KeyName::Down);
		assert_eq!(prompt.state().focused().map(String::as_str), Some("orange"));
		press(&mut prompt, KeyName::Down);
		assert_eq!(prompt.state().focused().map(String::as_str), Some("banana"));
		press(&mut prompt, KeyName::Up);
		assert_eq!(prompt.state().focused().map(String::as_str), Some("orange"));
	}

	#[test]
	fn a_disabled_option_cannot_be_focused_by_a_search_that_leaves_only_it() {
		let mut list = fruit();
		list.push(SelectOption::labelled("kiwi".to_string(), "Kiwi").with_disabled(true));
		let mut prompt = open(AutocompleteState::new(list));
		typed(&mut prompt, "k");
		assert_eq!(prompt.state().matches(), 1);
		assert_eq!(prompt.state().focused(), None);
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt), Vec::<String>::new());
	}

	#[test]
	fn tab_ticks_an_option_and_ticks_it_off_again() {
		let mut prompt = open(AutocompleteState::new(fruit()).with_multiple(true));
		press(&mut prompt, KeyName::Tab);
		assert_eq!(prompt.state().selected(), ["apple"]);
		press(&mut prompt, KeyName::Tab);
		assert!(prompt.state().selected().is_empty());
	}

	/// Space is a space until the arrows have been used, because until then you are typing.
	#[test]
	fn space_only_ticks_once_the_arrows_have_been_used() {
		let mut prompt = open(AutocompleteState::new(fruit()).with_multiple(true));
		space(&mut prompt);
		assert!(prompt.state().selected().is_empty());
		assert!(!prompt.state().is_navigating());

		press(&mut prompt, KeyName::Down);
		assert!(prompt.state().is_navigating());
		space(&mut prompt);
		assert_eq!(prompt.state().selected(), ["banana"]);
	}

	#[test]
	fn anything_else_stops_the_navigating() {
		let mut prompt = open(AutocompleteState::new(fruit()).with_multiple(true));
		press(&mut prompt, KeyName::Down);
		assert!(prompt.state().is_navigating());
		typed(&mut prompt, "a");
		assert!(!prompt.state().is_navigating());
	}

	#[test]
	fn tab_fills_the_field_from_the_placeholder() {
		let state = AutocompleteState::new(fruit()).with_placeholder("apple");
		let mut prompt = open(state);
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		assert_eq!(prompt.user_input(), "apple");
		assert_eq!(prompt.state().matches(), 1);
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt), ["apple"]);
	}

	/// Only on an empty field. Tab on a search that has been typed is a tab like any other.
	#[test]
	fn a_field_with_something_in_it_is_not_overwritten_by_the_placeholder() {
		let state = AutocompleteState::new(fruit()).with_placeholder("apple");
		let mut prompt = open(state);
		typed(&mut prompt, "gra");
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		assert_eq!(prompt.user_input(), "gra");
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt), ["grape"]);
	}

	/// And only for something that can be chosen: `!opt.disabled && filter(placeholder, opt)`.
	#[test]
	fn a_placeholder_that_matches_only_a_disabled_option_is_not_typed_for_you() {
		let mut list = fruit();
		list.push(SelectOption::labelled("kiwi".to_string(), "Kiwi").with_disabled(true));
		let state = AutocompleteState::new(list).with_placeholder("kiwi");
		let mut prompt = open(state);
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		assert_eq!(prompt.user_input(), "");
	}

	/// Only when it matches something. A placeholder that is a piece of advice is not a search.
	#[test]
	fn a_placeholder_that_matches_nothing_is_not_typed_for_you() {
		let state = AutocompleteState::new(fruit()).with_placeholder("Type to search...");
		let mut prompt = open(state);
		prompt.key(Some("\t"), &Key::named(KeyName::Tab));
		assert_eq!(prompt.user_input(), "");
		assert_eq!(prompt.state().matches(), 5);
	}

	// --- The widgets ----------------------------------------------------------------------------

	fn drawn(frame: &Frame) -> Vec<String> {
		frame
			.lines
			.iter()
			.map(|line| line.spans.iter().map(|s| s.text.as_str()).collect())
			.collect()
	}

	fn single(prompt: &Prompt<AutocompleteState<String>>) -> Vec<String> {
		drawn(&AutocompleteWidget::new(prompt, "Pick").frame())
	}

	fn multi(prompt: &Prompt<AutocompleteState<String>>) -> Vec<String> {
		drawn(&AutocompleteMultiSelectWidget::new(prompt, "Pick").frame())
	}

	#[test]
	fn an_opening_frame_is_the_title_the_search_the_list_and_the_footer() {
		let prompt = open(AutocompleteState::new(options(&[("a", "A"), ("b", "B")])));
		assert_eq!(
			single(&prompt),
			[
				"│",
				"◆  Pick",
				"│",
				"│  Search: _",
				"│  ● A",
				"│  ○ B",
				"│  ↑/↓ to select • Enter: confirm • Type: to search",
				"└",
			]
		);
	}

	#[test]
	fn the_multiple_one_draws_checkboxes_and_a_fourth_instruction() {
		let state = AutocompleteState::new(options(&[("a", "A"), ("b", "B")])).with_multiple(true);
		let prompt = open(state);
		assert_eq!(
			multi(&prompt),
			[
				"│",
				"◆  Pick",
				"│",
				"│  Search: _",
				"│  ◻ A",
				"│  ◻ B",
				"│  ↑/↓ to navigate • Tab: select • Enter: confirm • Type: to search",
				"└",
			]
		);
	}

	/// The second instruction changes once the arrows have been used, because that is when space
	/// stops being a space.
	#[test]
	fn navigating_renames_the_second_instruction() {
		let state = AutocompleteState::new(options(&[("a", "A"), ("b", "B")])).with_multiple(true);
		let mut prompt = open(state);
		press(&mut prompt, KeyName::Down);
		assert_eq!(
			multi(&prompt)[6],
			"│  ↑/↓ to navigate • Space/Tab: select • Enter: confirm • Type: to search"
		);
	}

	/// The quirk the module docs name: the empty string standing in for the bar is still a row.
	#[test]
	fn an_unguided_multiple_one_keeps_a_blank_row_where_its_bar_was() {
		let state = AutocompleteState::new(options(&[("a", "A")])).with_multiple(true);
		let prompt = open(state);
		let widget = AutocompleteMultiSelectWidget::new(&prompt, "Pick").with_guide(false);
		assert_eq!(
			drawn(&widget.frame()),
			[
				"◆  Pick",
				"",
				"Search: _",
				"◻ A",
				"↑/↓ to navigate • Tab: select • Enter: confirm • Type: to search",
				"",
			]
		);
	}

	#[test]
	fn an_unguided_single_one_has_no_such_row() {
		let prompt = open(AutocompleteState::new(options(&[("a", "A")])));
		let widget = AutocompleteWidget::new(&prompt, "Pick").with_guide(false);
		assert_eq!(
			drawn(&widget.frame()),
			[
				"◆  Pick",
				"Search: _",
				"● A",
				"↑/↓ to select • Enter: confirm • Type: to search",
				"",
			]
		);
	}

	#[test]
	fn a_search_draws_its_own_cursor_and_a_count_of_what_it_left() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		typed(&mut prompt, "an");
		assert_eq!(single(&prompt)[3], "│  Search: an█ (2 matches)");
	}

	/// One match is one `match`, which is the only reason the counter is built rather than formatted.
	#[test]
	fn one_match_is_not_pluralised() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		typed(&mut prompt, "app");
		assert_eq!(single(&prompt)[3], "│  Search: app█ (1 match)");
	}

	#[test]
	fn a_search_that_matches_nothing_says_so_in_place_of_the_list() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		typed(&mut prompt, "z");
		assert_eq!(
			single(&prompt),
			[
				"│",
				"◆  Pick",
				"│",
				"│  Search: z█ (0 matches)",
				"│  No matches found",
				"│  ↑/↓ to select • Enter: confirm • Type: to search",
				"└",
			]
		);
	}

	#[test]
	fn navigating_draws_the_text_without_a_cursor() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		typed(&mut prompt, "an");
		press(&mut prompt, KeyName::Down);
		assert_eq!(single(&prompt)[3], "│  Search: an (2 matches)");
	}

	#[test]
	fn a_placeholder_stands_in_for_an_empty_search() {
		let prompt = open(AutocompleteState::new(fruit()));
		let widget = AutocompleteWidget::new(&prompt, "Pick").with_placeholder("Type to search...");
		assert_eq!(drawn(&widget.frame())[3], "│  Search: Type to search...");
	}

	/// The bug the module docs name, and the one an authored recording pins: the highlight is drawn
	/// at the *option list's* index. Two matches, the cursor on the second, the text cursor at the
	/// start — and it is the second character that is inverted.
	#[test]
	fn the_search_cursor_is_drawn_where_the_option_cursor_is() {
		let list = options(&[("apple", "Apple"), ("grape", "Grape")]);
		let mut prompt = open(AutocompleteState::new(list));
		typed(&mut prompt, "ap");
		press(&mut prompt, KeyName::Down);
		press(&mut prompt, KeyName::Left);
		assert_eq!(prompt.cursor(), 1);
		assert_eq!(prompt.state().cursor(), 1);

		let spans = search_input(&prompt, &THEME);
		let text: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
		assert_eq!(text, ["a", "p"]);
		assert_eq!(spans[1].style, THEME.styles.cursor);
	}

	#[test]
	fn a_hint_follows_the_focused_option_and_nothing_else() {
		let list = vec![
			SelectOption::labelled("a".to_string(), "A").with_hint("first"),
			SelectOption::labelled("b".to_string(), "B").with_hint("second"),
		];
		let prompt = open(AutocompleteState::new(list));
		assert_eq!(single(&prompt)[4], "│  ● A (first)");
		assert_eq!(single(&prompt)[5], "│  ○ B");
	}

	#[test]
	fn a_disabled_option_is_drawn_and_cannot_be_reached() {
		let list = vec![
			SelectOption::labelled("a".to_string(), "A"),
			SelectOption::labelled("b".to_string(), "B")
				.with_hint("nope")
				.with_disabled(true),
		];
		let prompt = open(AutocompleteState::new(list));
		// No hint: upstream builds one and interpolates it into the active branch alone.
		assert_eq!(single(&prompt)[5], "│  ○ B");
	}

	#[test]
	fn a_submitted_frame_is_the_labels_and_a_cancelled_one_is_the_search() {
		let mut prompt = open(AutocompleteState::new(fruit()));
		press(&mut prompt, KeyName::Down);
		press(&mut prompt, KeyName::Return);
		assert_eq!(single(&prompt), ["│", "◇  Pick", "│  Banana"]);

		let mut cancelled = open(AutocompleteState::new(fruit()));
		typed(&mut cancelled, "ban");
		press(&mut cancelled, KeyName::Escape);
		assert_eq!(single(&cancelled), ["│", "■  Pick", "│  ban"]);
	}

	/// Nothing chosen and nothing typed: a bare bar, because the two spaces belong to the value.
	#[test]
	fn a_settled_frame_with_nothing_to_show_is_a_bare_bar() {
		let mut prompt = open(AutocompleteState::new(Vec::new()));
		press(&mut prompt, KeyName::Return);
		assert_eq!(single(&prompt), ["│", "◇  Pick", "│"]);

		let mut cancelled = open(AutocompleteState::new(fruit()));
		press(&mut cancelled, KeyName::Escape);
		assert_eq!(single(&cancelled), ["│", "■  Pick", "│"]);
	}

	/// The multiple one counts rather than listing, and writes its two spaces either way.
	#[test]
	fn the_multiple_one_settles_on_a_count() {
		let state = AutocompleteState::new(fruit()).with_multiple(true);
		let mut prompt = open(state);
		press(&mut prompt, KeyName::Tab);
		press(&mut prompt, KeyName::Down);
		space(&mut prompt);
		press(&mut prompt, KeyName::Return);
		assert_eq!(answer(&prompt), ["apple", "banana"]);
		assert_eq!(multi(&prompt), ["│", "◇  Pick", "│  2 items selected"]);
	}

	#[test]
	fn a_settled_multiple_one_with_nothing_chosen_still_writes_its_spaces() {
		let state = AutocompleteState::new(fruit()).with_multiple(true);
		let mut prompt = open(state);
		press(&mut prompt, KeyName::Return);
		assert_eq!(multi(&prompt), ["│", "◇  Pick", "│  0 items selected"]);
	}

	#[test]
	fn required_refuses_an_empty_selection_and_says_so_under_the_search() {
		let state = AutocompleteState::new(fruit()).with_multiple(true);
		let mut prompt = Prompt::new(state).with_validator(required);
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Error);
		assert_eq!(multi(&prompt)[4], "│  Please select at least one item");

		press(&mut prompt, KeyName::Tab);
		press(&mut prompt, KeyName::Return);
		assert_eq!(prompt.status(), Status::Submit);
	}

	/// The two widths, transcribed from `authored.json`: `autocomplete` takes three columns off the
	/// terminal for the bar beside its options, and `autocompleteMultiselect` takes none.
	#[test]
	fn the_two_prompts_wrap_the_same_option_three_columns_apart() {
		let list = options(&[
			("ts", "TypeScript, a static type checker for JS"),
			("js", "JavaScript, which is not one at all"),
		]);
		let prompt = open(AutocompleteState::new(list.clone()));
		let rows = drawn(
			&AutocompleteWidget::new(&prompt, "Pick")
				.with_columns(40)
				.frame(),
		);
		assert_eq!(rows[4], "│  ● TypeScript, a static type checker ");
		assert_eq!(rows[5], "│  for JS");

		let state = AutocompleteState::new(list).with_multiple(true);
		let prompt = open(state);
		let rows = drawn(
			&AutocompleteMultiSelectWidget::new(&prompt, "Pick")
				.with_columns(40)
				.frame(),
		);
		assert_eq!(rows[4], "│  ◻ TypeScript, a static type checker for ");
		assert_eq!(rows[5], "│  JS");
	}
}
