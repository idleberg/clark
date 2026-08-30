//! A port of `fast-string-width@3.0.2`, the model clack measures with.
//!
//! Per [ADR-0005], text is measured the way clack measures it, not the way `unicode-width` does.
//! `fast-string-width` is a three-line wrapper over `fast-string-truncated-width@3.0.3` called with
//! no limit, so what is ported here is that inner scanner with truncation removed. Truncation is
//! deliberately left out until `fast-wrap-ansi` needs it: its return value is a *UTF-16* index into
//! the input, which has no honest Rust counterpart, and inventing one before there is a consumer
//! would be guessing.
//!
//! [ADR-0005]: ../../../docs/adr/0005-port-fast-string-width-not-unicode-width.md
//!
//! # How the original works
//!
//! Upstream is a scanner over six sticky regexes, tried in order at the current index:
//!
//! | block | width |
//! |---|---|
//! | latin — `[\x20-\x7E\xA0-\xFF]` not followed by VS16 | 1 per code point |
//! | ANSI escape or OSC 8 hyperlink | 0 |
//! | C0/C1 control, excluding tab | 0 per code point |
//! | tab | 8 per tab |
//! | emoji sequence | 2 per *sequence* |
//! | Han, Hiragana, Katakana, Hangul or Tangut outside `FF00..=FFEF` | 2 per code point |
//!
//! When none match, the index advances by one code point and that code point is banked as
//! *unmatched*. Unmatched runs are settled when the next block matches, or at end of input: marks
//! (`\p{M}`) are dropped outright, and what remains is measured 2 if fullwidth or wide, 1 otherwise.
//!
//! Two upstream details are deliberately not reproduced, because neither can change a width:
//!
//! - The `{1,1000}` repetition caps. A longer run simply matches again on the next turn of the
//!   loop, and the unmatched bookkeeping is empty across a block boundary either way.
//! - The truncation index arithmetic, per above.
//!
//! # What decides the answers
//!
//! The regexes lean on Unicode properties — `\p{Emoji}`, `\p{Emoji_Modifier_Base}`,
//! `\p{Emoji_Presentation}`, `\p{Script=…}`, `\p{M}` — so the tables, not this code, decide most
//! cases. Ours come from `unicode-properties` and `unicode-script`; upstream's come from whichever
//! Unicode version the running V8 was built against. Those can disagree, and the disagreement would
//! be silent, which is why both crates are pinned exactly and why the corpus in
//! `tests/fixtures/width.json` is harvested from the real JavaScript rather than reasoned out.

use unicode_properties::{EmojiStatus, GeneralCategoryGroup, UnicodeEmoji, UnicodeGeneralCategory};
use unicode_script::{Script, UnicodeScript};

/// The per-block widths upstream exposes as `WidthOptions`.
///
/// clack passes none of these, so [`Default`] is the only configuration parity depends on. They are
/// carried anyway because the ported scanner reads them, and dropping them would make the port
/// harder to check against the original.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidthOptions {
	/// C0/C1 controls other than tab. Upstream default 0.
	pub control_width: u16,
	/// One horizontal tab. Upstream default 8 — a fixed width, not a tab stop.
	pub tab_width: u16,
	/// One whole emoji sequence, however many code points it spans. Upstream default 2.
	pub emoji_width: u16,
	/// Anything not otherwise classified. Upstream default 1.
	pub regular_width: u16,
	/// CJKT scripts, and the wide-but-not-CJKT ranges. Upstream default 2.
	pub wide_width: u16,
}

impl Default for WidthOptions {
	fn default() -> Self {
		Self {
			control_width: 0,
			tab_width: 8,
			emoji_width: 2,
			regular_width: 1,
			wide_width: 2,
		}
	}
}

/// The width of the fullwidth forms, which upstream hard-codes rather than exposing as an option.
const FULL_WIDTH_WIDTH: u16 = 2;

/// The visual width of `input` once printed to a terminal, as clack would measure it.
///
/// This is `fastStringWidth(input)` with upstream's default options.
pub fn width(input: &str) -> usize {
	width_with(input, WidthOptions::default())
}

/// [`width`], with upstream's `WidthOptions` spelled out.
///
/// Defined as the sum of [`segments_with`] rather than as its own scan, so that the two can never
/// disagree about where a block begins — which is the failure the Frame would inherit silently.
pub fn width_with(input: &str, options: WidthOptions) -> usize {
	segments_with(input, options).map(|s| s.width).sum()
}

/// One placeable unit of text and the columns it occupies.
///
/// A Frame cannot ask for the width of a whole line and stop there: it has to put text into cells,
/// and a cell holds one unit. The unit is upstream's, not a grapheme cluster — an emoji sequence is
/// atomic because `fast-string-width` measures it as one, while conjoining jamo are three units of
/// two columns each because it measures them per code point. Segmenting any other way would place
/// text at columns the width model does not agree with, which is the one thing ADR-0005 exists to
/// prevent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment<'a> {
	pub text: &'a str,
	/// Columns occupied. Zero for an ANSI sequence, a control character, and a combining mark —
	/// none of which a terminal advances the cursor for.
	pub width: usize,
}

/// [`input`](str) split into placeable units, with upstream's default options.
pub fn segments(input: &str) -> Segments<'_> {
	segments_with(input, WidthOptions::default())
}

/// [`segments`], with upstream's `WidthOptions` spelled out.
pub fn segments_with(input: &str, options: WidthOptions) -> Segments<'_> {
	Segments {
		input,
		options,
		head: 0,
		run: None,
	}
}

/// The iterator [`segments`] returns.
pub struct Segments<'a> {
	input: &'a str,
	options: WidthOptions,
	/// The scan position, in bytes.
	head: usize,
	/// A matched block being handed out one code point at a time.
	run: Option<Run>,
}

/// A block whose width is per code point, part-way through being handed out.
#[derive(Clone, Copy)]
struct Run {
	end: usize,
	char_width: usize,
}

impl<'a> Iterator for Segments<'a> {
	type Item = Segment<'a>;

	/// The scan of [`width_with`] as it was, with the accumulator replaced by an emission.
	///
	/// Blocks are matched whole and *then* subdivided, never re-matched per code point. The
	/// difference is observable: `"\u{01}\u{1B}[0m"` is a two-character control run followed by the
	/// latin text `[0m`, but re-matching at the escape would find an ANSI sequence instead and lose
	/// three columns.
	fn next(&mut self) -> Option<Segment<'a>> {
		if let Some(run) = self.run {
			if self.head < run.end {
				let c = self.input[self.head..].chars().next()?;
				let text = &self.input[self.head..self.head + c.len_utf8()];
				self.head += c.len_utf8();
				return Some(Segment {
					text,
					width: run.char_width,
				});
			}
			self.run = None;
		}

		let rest = self.input.get(self.head..)?;
		if rest.is_empty() {
			return None;
		}

		if let Some(block) = match_block(rest, self.options) {
			let start = self.head;
			match block.char_width {
				Some(char_width) => {
					self.run = Some(Run {
						end: start + block.len,
						char_width,
					});
					return self.next();
				}
				None => {
					self.head += block.len;
					return Some(Segment {
						text: &self.input[start..self.head],
						width: block.width,
					});
				}
			}
		}

		// Unmatched. Upstream settles a whole run of these at once, but `unmatched_width` carries no
		// state from one code point to the next, so emitting them singly gives the same total.
		let c = rest.chars().next()?;
		let start = self.head;
		self.head += c.len_utf8();
		Some(Segment {
			text: &self.input[start..self.head],
			width: unmatched_width(&self.input[start..self.head], self.options),
		})
	}
}

/// A matched block: how far it reaches, how wide it is, and whether it can be subdivided.
struct Block {
	len: usize,
	width: usize,
	/// The width of each code point, for a block whose width is a per-code-point sum. `None` marks
	/// a block that is one indivisible unit however many code points it spans.
	char_width: Option<usize>,
}

impl Block {
	fn per_char(len: usize, count: usize, char_width: u16) -> Self {
		Self {
			len,
			width: count * char_width as usize,
			char_width: Some(char_width as usize),
		}
	}

	fn atomic(len: usize, width: usize) -> Self {
		Self {
			len,
			width,
			char_width: None,
		}
	}
}

/// The six blocks, in upstream's order. Order is load-bearing: latin runs before emoji so that the
/// `1` of a keycap sequence is not eaten as text, and emoji runs before CJKT so that an emoji whose
/// script happens to be Han is measured as a sequence.
fn match_block(s: &str, o: WidthOptions) -> Option<Block> {
	match_latin(s, o)
		.or_else(|| match_ansi(s).map(|len| Block::atomic(len, 0)))
		.or_else(|| match_control(s, o))
		.or_else(|| match_tab(s, o))
		.or_else(|| match_emoji(s, o))
		.or_else(|| match_cjkt(s, o))
}

/// `/(?:[\x20-\x7E\xA0-\xFF](?!\uFE0F)){1,1000}/y`
///
/// The negative lookahead is what keeps the leading digit of a keycap sequence out of
/// this block and in the emoji one.
fn match_latin(s: &str, o: WidthOptions) -> Option<Block> {
	let mut len = 0;
	let mut count = 0;

	while let Some(c) = s[len..].chars().next() {
		let cp = c as u32;
		if !((0x20..=0x7E).contains(&cp) || (0xA0..=0xFF).contains(&cp)) {
			break;
		}
		let after = len + c.len_utf8();
		if s[after..].starts_with('\u{FE0F}') {
			break;
		}
		len = after;
		count += 1;
	}

	(count > 0).then(|| Block::per_char(len, count, o.regular_width))
}

/// `/[\x00-\x08\x0A-\x1F\x7F-\x9F]{1,1000}/y` — tab is excluded, it has its own block.
fn match_control(s: &str, o: WidthOptions) -> Option<Block> {
	let mut len = 0;
	let mut count = 0;

	while let Some(c) = s[len..].chars().next() {
		let cp = c as u32;
		if !((0x00..=0x08).contains(&cp)
			|| (0x0A..=0x1F).contains(&cp)
			|| (0x7F..=0x9F).contains(&cp))
		{
			break;
		}
		len += c.len_utf8();
		count += 1;
	}

	(count > 0).then(|| Block::per_char(len, count, o.control_width))
}

/// `/\t{1,1000}/y`
fn match_tab(s: &str, o: WidthOptions) -> Option<Block> {
	let count = s.bytes().take_while(|b| *b == b'\t').count();
	(count > 0).then(|| Block::per_char(count, count, o.tab_width))
}

/// `/(?:(?![｡-ﾟ＀-￯])[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\p{Script=Tangut}]){1,1000}/yu`
///
/// Width is per code point, which is why conjoining jamo measure 6 here and 2 under Ratatui's
/// grapheme-aware model — the disagreement M0 was probed with.
fn match_cjkt(s: &str, o: WidthOptions) -> Option<Block> {
	let mut len = 0;
	let mut count = 0;

	while let Some(c) = s[len..].chars().next() {
		if !is_cjkt_wide(c) {
			break;
		}
		len += c.len_utf8();
		count += 1;
	}

	(count > 0).then(|| Block::per_char(len, count, o.wide_width))
}

fn is_cjkt_wide(c: char) -> bool {
	// The two excluded ranges upstream lists overlap; their union is the halfwidth and fullwidth
	// forms block, which is measured by the unmatched path instead.
	if (0xFF00..=0xFFEF).contains(&(c as u32)) {
		return false;
	}

	matches!(
		c.script(),
		Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul | Script::Tangut
	)
}

// --- ANSI ------------------------------------------------------------------------------------

/// `/[\x1B\x9B][[()#;?]*(?:[0-9]{1,4}(?:;[0-9]{0,4})*)?[0-9A-ORZcf-nqry=><]`
/// `|\x1B\]8;[^;]*;.*?(?:\x07|\x1B\\)/y`
///
/// Two alternatives, tried in order: a CSI-shaped sequence, then an OSC 8 hyperlink.
fn match_ansi(s: &str) -> Option<usize> {
	match_ansi_csi(s).or_else(|| match_ansi_osc8(s))
}

fn match_ansi_csi(s: &str) -> Option<usize> {
	let introducer = s.chars().next()?;
	if introducer != '\u{1B}' && introducer != '\u{9B}' {
		return None;
	}

	// `[[()#;?]*`. Greedy is safe: nothing this class matches can also serve as the final byte, so
	// giving a character back could never rescue a failed match.
	let mut j = introducer.len_utf8();
	while let Some(c) = s[j..].chars().next() {
		if !matches!(c, '[' | '(' | ')' | '#' | ';' | '?') {
			break;
		}
		j += c.len_utf8();
	}

	// The parameter group is optional and its last element may be a digit, which is also a legal
	// final byte — so the regex can be forced to give digits back. Try every end the group could
	// legitimately have, longest first, exactly as backtracking would.
	for end in params_ends(&s[j..]) {
		let k = j + end;
		if let Some(c) = s[k..].chars().next() {
			if is_ansi_final(c) {
				return Some(k + c.len_utf8());
			}
		}
	}

	None
}

/// Every offset at which `[0-9]{1,4}(?:;[0-9]{0,4})*` could stop, longest first, with 0 last for
/// the group being absent altogether. All ASCII, so byte offsets are character offsets.
fn params_ends(s: &str) -> Vec<usize> {
	let b = s.as_bytes();
	let mut ends = Vec::new();
	let mut i = 0;

	let mut run = 0;
	while i < b.len() && b[i].is_ascii_digit() && run < 4 {
		i += 1;
		run += 1;
		ends.push(i);
	}

	if run > 0 {
		while i < b.len() && b[i] == b';' {
			i += 1;
			ends.push(i);
			let mut r = 0;
			while i < b.len() && b[i].is_ascii_digit() && r < 4 {
				i += 1;
				r += 1;
				ends.push(i);
			}
		}
	} else {
		ends.clear();
	}

	ends.reverse();
	ends.push(0);
	ends
}

/// `[0-9A-ORZcf-nqry=><]`
fn is_ansi_final(c: char) -> bool {
	c.is_ascii_digit()
		|| matches!(c, 'A'..='O' | 'R' | 'Z' | 'c' | 'f'..='n' | 'q' | 'r' | 'y' | '=' | '>' | '<')
}

fn match_ansi_osc8(s: &str) -> Option<usize> {
	const PREFIX: &str = "\u{1B}]8;";
	let rest = s.strip_prefix(PREFIX)?;

	// `[^;]*;` — greedy, but the class cannot cross a `;`, so this is the first one.
	let mut j = rest.find(';')? + 1;

	// `.*?(?:\x07|\x1B\\)` -- lazy, so the earliest terminator wins. `.` excludes line
	// terminators, so one appearing before a terminator fails the whole alternative.
	loop {
		let tail = &rest[j..];
		if tail.starts_with('\u{07}') {
			return Some(PREFIX.len() + j + 1);
		}
		if tail.starts_with("\u{1B}\\") {
			return Some(PREFIX.len() + j + 2);
		}

		let c = tail.chars().next()?;
		if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
			return None;
		}
		j += c.len_utf8();
	}
}

// --- Emoji -----------------------------------------------------------------------------------

/// `/[\u{1F1E6}-\u{1F1FF}]{2}`
/// `|\u{1F3F4}[\u{E0061}-\u{E007A}]{2}[\u{E0030}-\u{E0039}\u{E0061}-\u{E007A}]{1,3}\u{E007F}`
/// `|(?:HEAD)(?:\u200D(?:TAIL))*/yu`
///
/// `HEAD` and `TAIL` are the two alternations spelled out on [`match_emoji_head`] and
/// [`match_emoji_tail_item`].
///
/// A whole sequence counts as one unit — upstream hard-codes the length at 1 for this block — so a
/// family of four people and a single `\u263A\uFE0F` both measure 2.
fn match_emoji(s: &str, o: WidthOptions) -> Option<Block> {
	emoji_len(s).map(|len| Block::atomic(len, o.emoji_width as usize))
}

fn emoji_len(s: &str) -> Option<usize> {
	if let Some(len) = match_flag(s) {
		return Some(len);
	}
	if let Some(len) = match_tag_sequence(s) {
		return Some(len);
	}

	let mut j = match_emoji_head(s)?;

	// `(?:\u200D...)*` -- greedy, but a ZWJ with nothing valid after it is simply not
	// consumed. No wider backtracking is needed, because the group may match zero times.
	// wider backtracking is needed because the group can always match zero times.
	while s[j..].starts_with('\u{200D}') {
		let after_zwj = j + '\u{200D}'.len_utf8();
		match match_emoji_tail_item(&s[after_zwj..]) {
			Some(len) => j = after_zwj + len,
			None => break,
		}
	}

	Some(j)
}

/// `[\u{1F1E6}-\u{1F1FF}]{2}` — a flag, as a pair of regional indicators.
fn match_flag(s: &str) -> Option<usize> {
	let mut chars = s.chars();
	let a = chars.next()?;
	let b = chars.next()?;
	(is_regional_indicator(a) && is_regional_indicator(b)).then(|| a.len_utf8() + b.len_utf8())
}

fn is_regional_indicator(c: char) -> bool {
	matches!(c, '\u{1F1E6}'..='\u{1F1FF}')
}

/// `\u{1F3F4}[\u{E0061}-\u{E007A}]{2}[\u{E0030}-\u{E0039}\u{E0061}-\u{E007A}]{1,3}\u{E007F}` — the
/// subdivision flags, such as the flag of Scotland.
fn match_tag_sequence(s: &str) -> Option<usize> {
	let rest = s.strip_prefix('\u{1F3F4}')?;
	let mut chars = rest.chars();

	let mut len = 0;
	for _ in 0..2 {
		let c = chars.next()?;
		if !matches!(c, '\u{E0061}'..='\u{E007A}') {
			return None;
		}
		len += c.len_utf8();
	}

	// `{1,3}` greedy, then a required terminator, so up to two characters may have to be given
	// back. Collect what the class allows and try the longest first.
	let mut spec = Vec::new();
	for c in chars.take(3) {
		if !matches!(c, '\u{E0030}'..='\u{E0039}' | '\u{E0061}'..='\u{E007A}') {
			break;
		}
		spec.push(c);
	}

	while !spec.is_empty() {
		let taken: usize = spec.iter().map(|c| c.len_utf8()).sum();
		if rest[len + taken..].starts_with('\u{E007F}') {
			return Some('\u{1F3F4}'.len_utf8() + len + taken + '\u{E007F}'.len_utf8());
		}
		spec.pop();
	}

	None
}

/// `(?:\p{Emoji}\uFE0F\u20E3?|\p{Emoji_Modifier_Base}\p{Emoji_Modifier}?|\p{Emoji_Presentation})`
fn match_emoji_head(s: &str) -> Option<usize> {
	let c = s.chars().next()?;

	if is_emoji(c) {
		let after = c.len_utf8();
		if s[after..].starts_with('\u{FE0F}') {
			let mut len = after + '\u{FE0F}'.len_utf8();
			if s[len..].starts_with('\u{20E3}') {
				len += '\u{20E3}'.len_utf8();
			}
			return Some(len);
		}
	}

	if is_emoji_modifier_base(c) {
		return Some(c.len_utf8() + optional_modifier_len(&s[c.len_utf8()..]));
	}

	if is_emoji_presentation(c) {
		return Some(c.len_utf8());
	}

	None
}

/// `(?:\p{Emoji_Modifier_Base}\p{Emoji_Modifier}?|\p{Emoji_Presentation}|\p{Emoji}\uFE0F\u20E3?)`
///
/// The same three alternatives as the head, in a different order — upstream's, reproduced as
/// written, because alternation order decides which one wins when two could match.
fn match_emoji_tail_item(s: &str) -> Option<usize> {
	let c = s.chars().next()?;

	if is_emoji_modifier_base(c) {
		return Some(c.len_utf8() + optional_modifier_len(&s[c.len_utf8()..]));
	}

	if is_emoji_presentation(c) {
		return Some(c.len_utf8());
	}

	if is_emoji(c) {
		let after = c.len_utf8();
		if s[after..].starts_with('\u{FE0F}') {
			let mut len = after + '\u{FE0F}'.len_utf8();
			if s[len..].starts_with('\u{20E3}') {
				len += '\u{20E3}'.len_utf8();
			}
			return Some(len);
		}
	}

	None
}

fn optional_modifier_len(s: &str) -> usize {
	match s.chars().next() {
		Some(c) if is_emoji_modifier(c) => c.len_utf8(),
		_ => 0,
	}
}

fn is_emoji(c: char) -> bool {
	c.is_emoji_char()
}

fn is_emoji_presentation(c: char) -> bool {
	matches!(
		c.emoji_status(),
		EmojiStatus::EmojiPresentation
			| EmojiStatus::EmojiPresentationAndModifierBase
			| EmojiStatus::EmojiPresentationAndEmojiComponent
			| EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
	)
}

fn is_emoji_modifier_base(c: char) -> bool {
	matches!(
		c.emoji_status(),
		EmojiStatus::EmojiModifierBase | EmojiStatus::EmojiPresentationAndModifierBase
	)
}

fn is_emoji_modifier(c: char) -> bool {
	matches!(
		c.emoji_status(),
		EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
	)
}

// --- Unmatched -------------------------------------------------------------------------------

/// What upstream does with everything no block claimed: strip `\p{M}+`, then measure per code point.
fn unmatched_width(s: &str, o: WidthOptions) -> usize {
	let mut width = 0;

	for c in s.chars() {
		if c.general_category_group() == GeneralCategoryGroup::Mark {
			continue;
		}

		let cp = c as u32;
		width += if is_full_width(cp) {
			FULL_WIDTH_WIDTH
		} else if is_wide_not_cjkt_not_emoji(cp) {
			o.wide_width
		} else {
			o.regular_width
		} as usize;
	}

	width
}

/// Upstream's `isFullWidth`.
fn is_full_width(cp: u32) -> bool {
	cp == 0x3000 || (0xFF01..=0xFF60).contains(&cp) || (0xFFE0..=0xFFE6).contains(&cp)
}

/// Upstream's `isWideNotCJKTNotEmoji` — the wide ranges left over once the CJKT and emoji blocks
/// have had their turn.
fn is_wide_not_cjkt_not_emoji(cp: u32) -> bool {
	cp == 0x231B
		|| cp == 0x2329
		|| (0x2FF0..=0x2FFF).contains(&cp)
		|| (0x3001..=0x303E).contains(&cp)
		|| (0x3099..=0x30FF).contains(&cp)
		|| (0x3105..=0x312F).contains(&cp)
		|| (0x3131..=0x318E).contains(&cp)
		|| (0x3190..=0x31E3).contains(&cp)
		|| (0x31EF..=0x321E).contains(&cp)
		|| (0x3220..=0x3247).contains(&cp)
		|| (0x3250..=0x4DBF).contains(&cp)
		|| (0xFE10..=0xFE19).contains(&cp)
		|| (0xFE30..=0xFE52).contains(&cp)
		|| (0xFE54..=0xFE66).contains(&cp)
		|| (0xFE68..=0xFE6B).contains(&cp)
		|| (0x1F200..=0x1F202).contains(&cp)
		|| (0x1F210..=0x1F23B).contains(&cp)
		|| (0x1F240..=0x1F248).contains(&cp)
		|| (0x20000..=0x2FFFD).contains(&cp)
		|| (0x30000..=0x3FFFD).contains(&cp)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Not parity, just the scanner's own seams: every block, and the boundaries between them. The
	/// numbers that have to match JavaScript live in `tests/fixtures/width.json`.

	#[test]
	fn latin_is_one_per_character() {
		assert_eq!(width("hello"), 5);
		assert_eq!(width(""), 0);
	}

	#[test]
	fn tabs_are_eight_and_do_not_join_the_control_block() {
		assert_eq!(width("\t"), 8);
		assert_eq!(width("\t\t"), 16);
		assert_eq!(width("a\tb"), 10);
	}

	#[test]
	fn controls_are_free() {
		assert_eq!(width("\u{07}"), 0);
		assert_eq!(width("a\u{07}\u{08}b"), 2);
	}

	#[test]
	fn csi_sequences_are_free() {
		assert_eq!(width("\u{1B}[31mred\u{1B}[0m"), 3);
		assert_eq!(width("\u{1B}[38;5;196mx"), 1);
	}

	/// The final byte may be a digit, which forces the parameter group to give one back.
	#[test]
	fn a_csi_sequence_may_end_in_a_digit() {
		assert_eq!(width("\u{1B}[12345"), 0);
	}

	#[test]
	fn osc8_hyperlinks_are_free() {
		assert_eq!(width("\u{1B}]8;;https://example.com\u{07}link"), 4);
		assert_eq!(width("\u{1B}]8;;https://example.com\u{1B}\\link"), 4);
	}

	/// An unterminated hyperlink is not an escape at all; it falls through to the other blocks.
	#[test]
	fn an_unterminated_osc8_is_measured_as_text() {
		assert!(width("\u{1B}]8;;https://example.com") > 0);
	}

	#[test]
	fn cjkt_is_two_per_code_point() {
		assert_eq!(width("\u{4F60}\u{597D}"), 4);
		assert_eq!(width("\u{AC01}"), 2);
	}

	/// The disagreement M0 was probed with: three jamo, measured one by one.
	#[test]
	fn conjoining_jamo_are_measured_per_code_point() {
		assert_eq!(width("\u{1100}\u{1161}\u{11A8}"), 6);
	}

	/// Halfwidth katakana are excluded from the CJKT block and fall through to the unmatched path.
	#[test]
	fn halfwidth_forms_are_not_cjkt() {
		assert_eq!(width("\u{FF76}"), 1);
	}

	#[test]
	fn fullwidth_forms_are_two() {
		assert_eq!(width("\u{FF21}"), 2);
		assert_eq!(width("\u{3000}"), 2);
	}

	#[test]
	fn a_zwj_sequence_is_one_unit() {
		assert_eq!(width("\u{1F469}\u{200D}\u{1F4BB}"), 2);
		assert_eq!(
			width("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}"),
			2
		);
	}

	#[test]
	fn flags_are_two() {
		assert_eq!(width("\u{1F1E9}\u{1F1EA}"), 2);
	}

	/// The flag of Scotland: a tag sequence, whose `{1,3}` run has to give characters back before
	/// the terminator matches.
	#[test]
	fn tag_sequences_are_two() {
		let scotland = "\u{1F3F4}\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}";
		assert_eq!(width(scotland), 2);
	}

	/// The latin block's lookahead exists for this: the `1` must not be eaten as text.
	#[test]
	fn keycaps_are_two() {
		assert_eq!(width("\u{0031}\u{FE0F}\u{20E3}"), 2);
	}

	/// VS16 decides it. Without one, U+2764 is not `Emoji_Presentation`, so no emoji block matches
	/// and it settles as one ordinary column.
	#[test]
	fn presentation_selectors_change_the_answer() {
		assert_eq!(width("\u{2764}\u{FE0F}"), 2);
		assert_eq!(width("\u{2764}"), 1);
	}

	#[test]
	fn combining_marks_are_dropped() {
		assert_eq!(width("\u{0065}\u{0301}"), 1);
		assert_eq!(width("\u{0041}\u{0300}\u{0301}\u{0302}"), 1);
	}

	/// A lone ZWJ matches nothing, is not a mark, and is not wide — so it costs a column, which is
	/// one of the three places clack and `unicode-width` still disagree.
	#[test]
	fn a_lone_zwj_costs_a_column() {
		assert_eq!(width("\u{200D}"), 1);
	}

	/// Unmatched runs are settled when the next block matches, not only at end of input.
	#[test]
	fn unmatched_runs_are_settled_before_the_block_that_ended_them() {
		assert_eq!(width("\u{2764}\u{4F60}"), 3);
		assert_eq!(width("\u{2764}abc"), 4);
	}

	#[test]
	fn options_are_honoured() {
		let o = WidthOptions {
			tab_width: 4,
			..WidthOptions::default()
		};
		assert_eq!(width_with("\t", o), 4);
	}

	// --- Segmentation ---------------------------------------------------------------------------

	fn split(input: &str) -> Vec<(&str, usize)> {
		segments(input).map(|s| (s.text, s.width)).collect()
	}

	#[test]
	fn a_per_code_point_block_is_handed_out_one_at_a_time() {
		assert_eq!(split("hi"), [("h", 1), ("i", 1)]);
		assert_eq!(split("\t\t"), [("\t", 8), ("\t", 8)]);
	}

	/// The jamo again, now as three units. A Frame that placed them as one cell would have to pick a
	/// width for it, and any choice would disagree with the model that laid the line out.
	#[test]
	fn conjoining_jamo_are_three_segments_of_two_columns() {
		assert_eq!(
			split("\u{1100}\u{1161}\u{11A8}"),
			[("\u{1100}", 2), ("\u{1161}", 2), ("\u{11A8}", 2)]
		);
	}

	/// The other direction: however many code points an emoji sequence spans, it is one unit, so it
	/// goes into one cell.
	#[test]
	fn an_emoji_sequence_is_a_single_segment() {
		let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
		assert_eq!(split(family), [(family, 2)]);
	}

	#[test]
	fn escapes_and_marks_are_segments_of_no_width() {
		assert_eq!(
			split("\u{1B}[31ma"),
			[("\u{1B}[31m", 0), ("a", 1)],
			"an escape is one unit, and occupies no column"
		);
		assert_eq!(split("e\u{0301}"), [("e", 1), ("\u{0301}", 0)]);
	}

	/// Blocks are matched whole and then subdivided. Re-matching at every code point would find an
	/// ANSI sequence inside this control run and swallow the three columns after it.
	#[test]
	fn a_block_is_not_re_matched_part_way_through() {
		assert_eq!(
			split("\u{01}\u{1B}[0m"),
			[("\u{01}", 0), ("\u{1B}", 0), ("[", 1), ("0", 1), ("m", 1)]
		);
	}

	/// The property the Frame depends on, and the reason `width_with` is defined as this sum.
	#[test]
	fn the_segments_of_a_string_measure_the_string() {
		for case in [
			"hello",
			"\u{1B}[31mred\u{1B}[0m",
			"\u{1100}\u{1161}\u{11A8}",
			"\u{1F469}\u{200D}\u{1F4BB}x",
			"a\tb",
			"e\u{0301}\u{4F60}\u{FF21}",
			"\u{0031}\u{FE0F}\u{20E3}",
		] {
			let summed: usize = segments(case).map(|s| s.width).sum();
			assert_eq!(summed, width(case), "{case:?}");
			let rejoined: String = segments(case).map(|s| s.text).collect();
			assert_eq!(rejoined, case, "{case:?}");
		}
	}
}
