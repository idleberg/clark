// The hand-authored Scenarios: what clack does at widths its own test suite never uses.
//
// Every harvested Scenario runs at one width and none of them resizes, because upstream's tests
// have no reason to vary the terminal. Wrapping and re-layout are therefore the least-tested paths
// in the port and the two places `session.rs` records a known divergence. These cases exist to put
// a recording under them.
//
// A few cases are here for the other reason: a keypress upstream's suite never sends. They earn
// their place the same way — there is a branch in clack nothing recorded reaches — and they say so
// in their own comment.
//
// A case is upstream's input and nothing else — a prompt to call, options, a terminal width, an
// ordered sequence of events, and what that prompt should return. No expected output appears here:
// that is the whole point of a Scenario, and the bytes are whatever clack writes when the case is
// run. `kind` names the prompt and defaults to `text`.
//
// `value` is not decoration. A hand-authored Scenario has no upstream snapshot behind it, so the
// only evidence that the events drove clack anywhere is what clack handed back; ./record.test.mjs
// asserts it and refuses to write a recording that fails.

/** A run of ordinary characters, as readline reports them: the character, named after itself. */
const typing = (text) => [...text].map((s) => ({ kind: 'key', s, key: { name: s } }));

const enter = { kind: 'key', s: '', key: { name: 'return' } };
const tab = { kind: 'key', s: '', key: { name: 'tab' } };
const shiftTab = { kind: 'key', s: '', key: { name: 'tab', shift: true } };
const up = { kind: 'key', s: '', key: { name: 'up' } };
const down = { kind: 'key', s: '', key: { name: 'down' } };
const left = { kind: 'key', s: '', key: { name: 'left' } };
const right = { kind: 'key', s: '', key: { name: 'right' } };
const escape = { kind: 'key', s: '', key: { name: 'escape' } };

/** A UTC midnight, which is the only kind of `Date` a `date` prompt ever holds. */
const utc = (iso) => {
	const [y, m, d] = iso.split('-').map(Number);
	return new Date(Date.UTC(y, m - 1, d));
};
const backspace = (n) =>
	Array.from({ length: n }, () => ({ kind: 'key', s: '', key: { name: 'backspace' } }));

/** The same event, n times. */
const repeat = (n, event) => Array.from({ length: n }, () => event);

/** The terminal changes width under a Prompt that is already open.
 *
 * This is the event upstream's tests never send, and the only thing that can settle the divergence
 * `session.rs` records: on re-render clack re-wraps the *previous* Frame at the terminal's current
 * width to count the rows it walks back over, while the Emitter keeps the rows it laid out. The two
 * agree whenever the terminal has not narrowed, so a narrowing is the case that decides it. */
const resize = (columns) => ({ kind: 'resize', columns });

/** Wide enough that the prompt's own `◇  ` prefix matters, narrow enough to wrap a short sentence. */
const NARROW = 40;

export const cases = [
	{
		// The message is upstream's own longest realistic question, at a width it does not fit.
		name: 'narrow › the message wraps',
		columns: NARROW,
		opts: { message: 'What is the name of the package you would like to publish?' },
		events: [...typing('acme'), enter],
		value: 'acme',
	},
	{
		// The value crosses the width while it is being typed, so the Frame gains a row between two
		// keypresses. Upstream re-wraps the previous Frame to count the rows it walks back over;
		// the port keeps the rows it laid out. Growing is the direction the two agree on.
		name: 'narrow › the value wraps as it is typed',
		columns: NARROW,
		opts: { message: 'Path' },
		events: [...typing('packages/prompts/src/prompts/text-input.ts'), enter],
		value: 'packages/prompts/src/prompts/text-input.ts',
	},
	{
		// And shrinking is the direction they might not. Deleting back across the wrap takes a row
		// away, which is the erase path rather than the write path.
		name: 'narrow › deleting back across the wrap',
		columns: NARROW,
		opts: { message: 'Path' },
		events: [...typing('packages/prompts/src/prompts/text-input.ts'), ...backspace(22), enter],
		value: 'packages/prompts/src',
	},
	{
		// Cancelled, wrapped, and struck through: the one Theme attribute `vt100` cannot see, on a
		// value long enough to need two rows. See ADR-0015 for why the emulator is `avt`.
		name: 'narrow › a cancelled value wraps',
		columns: NARROW,
		opts: { message: 'Path' },
		events: [...typing('packages/prompts/src/prompts/text-input.ts'), escape],
		cancelled: true,
	},
	{
		// Wide characters at a width they fit: nothing wraps, and the only question is whether the
		// port agrees with `string-width` about how much of the row they occupy.
		name: 'cjk › a wide message and a wide value',
		columns: 80,
		opts: { message: '新しいパッケージの名前は何ですか' },
		events: [...typing('日本語'), enter],
		value: '日本語',
	},
	{
		// A wide character cannot straddle the wrap. With the `◇  ` prefix the usable width is odd,
		// so the boundary falls in the middle of a two-column character and something has to give.
		name: 'cjk › a wide message wraps where a character cannot split',
		columns: NARROW,
		opts: { message: '新しいパッケージの名前を入力してください、よろしくお願いします' },
		events: [...typing('acme'), enter],
		value: 'acme',
	},
	{
		// The same question about the value rather than the message, and the cursor sits after a
		// wide character rather than a narrow one.
		name: 'cjk › a wide value wraps',
		columns: 20,
		opts: { message: 'Name' },
		events: [...typing('日本語テキストの入力'), enter],
		value: '日本語テキストの入力',
	},
	{
		// The one that decides it. The value already occupies two rows at 40; at 20 it needs four, so
		// the previous Frame re-wraps under the cursor and upstream's row count and the Emitter's part
		// company — if they do.
		name: 'resize › the terminal narrows under a wrapped value',
		columns: NARROW,
		opts: { message: 'Path' },
		events: [
			...typing('packages/prompts/src/prompts/text-input.ts'),
			resize(20),
			enter,
		],
		value: 'packages/prompts/src/prompts/text-input.ts',
	},
	{
		// Widening is the direction they are expected to agree on: an already-wrapped row cannot wrap
		// again at a greater width, so re-wrapping the previous Frame and remembering it come to the
		// same number. Recorded anyway, because "expected to agree" is a claim and this is what
		// turns it into a test.
		name: 'resize › the terminal widens and the value fits again',
		columns: 20,
		opts: { message: 'Path' },
		events: [...typing('packages/prompts/src'), resize(60), enter],
		value: 'packages/prompts/src',
	},
	{
		// Narrowing with nothing typed, so what re-wraps is the message rather than the value, and
		// the Frame grows a row without any key being pressed.
		name: 'resize › the terminal narrows under a wrapped message',
		columns: NARROW,
		opts: { message: 'What is the name of the package you would like to publish?' },
		events: [resize(24), ...typing('acme'), enter],
		value: 'acme',
	},
	{
		// Twice, and back, so the recording covers a Frame that has been re-laid-out more than once
		// and a port that only resets its remembered rows on the first resize is caught.
		name: 'resize › narrowed, widened and narrowed again',
		columns: NARROW,
		opts: { message: 'Path' },
		events: [
			...typing('packages/prompts/src'),
			resize(20),
			...typing('/text.ts'),
			resize(60),
			...typing('!'),
			resize(24),
			enter,
		],
		value: 'packages/prompts/src/text.ts!',
	},

	// --- confirm ------------------------------------------------------------------------------
	//
	// Upstream's `confirm` suite never sends a `y` or an `n` and never uses a terminal narrow enough
	// to wrap, so two of the Prompt's behaviours reach no recording at all. Both are things a
	// terminal can see and neither is anything a port would invent, so they are recorded here.

	{
		// A `y` settles the Prompt from inside `ConfirmPrompt`'s own `confirm` listener: the listener
		// writes a cursor-up, closes — which writes a newline and shows the cursor — and only then
		// does `onKeypress` carry on to render the settled Frame and close a second time. Nothing
		// else in clack does this, and no harvested Scenario reaches it.
		name: 'confirm › a y settles the prompt without a return',
		kind: 'confirm',
		columns: 80,
		opts: { message: 'Continue?' },
		events: typing('y'),
		value: true,
	},
	{
		// The same path to the other answer, and past a `cursor` event first: the arrow flips the
		// value to `false` and the `n` then sets it to `false` again, so the two agree and what is
		// being recorded is the write ordering rather than the arithmetic.
		name: 'confirm › an n settles it after an arrow key',
		kind: 'confirm',
		columns: 80,
		opts: { message: 'Continue?' },
		events: [{ kind: 'key', s: '', key: { name: 'right' } }, ...typing('n')],
		value: false,
	},
	{
		// `confirm` is the only Prompt that wraps its own message, and it does it against the styled
		// Guide prefix — `wrapTextWithPrefix` subtracts `prefix.length`, which counts the escape
		// sequence as well as the bar. So a guided `confirm` breaks ten columns early, and this is
		// the recording that says so.
		name: 'confirm › the message wraps early because the prefix is measured with its escapes',
		kind: 'confirm',
		columns: NARROW,
		opts: { message: 'Do you want to publish this package to the public registry?' },
		events: [enter],
		value: true,
	},
	{
		// Without a Guide the prefix is the empty string, so there is nothing to mismeasure and the
		// whole terminal is used. The pair is what pins the ten columns to the prefix rather than to
		// the wrap.
		name: 'confirm › an unguided message wraps against the whole terminal',
		kind: 'confirm',
		columns: NARROW,
		opts: {
			message: 'Do you want to publish this package to the public registry?',
			withGuide: false,
		},
		events: [enter],
		value: true,
	},
	{
		// The choices themselves at a width they do not fit. They are not wrapped by the widget —
		// only the message is — so this is the outer `Prompt.render` wrap landing in the middle of a
		// row the port assembled.
		name: 'confirm › the two choices are wider than the terminal',
		kind: 'confirm',
		columns: 20,
		opts: { message: 'Ship?', active: 'Yes, right now', inactive: 'No, not yet' },
		events: [enter],
		value: true,
	},
	{
		// Cancelled: the answer flips on the way out, because `escape` is an alias for `cancel` and a
		// non-tracking Prompt turns every alias into a `cursor` event first. The harvested Scenario
		// records that at 80 columns; this one records it struck through and wrapped.
		name: 'confirm › a cancelled answer wraps',
		kind: 'confirm',
		columns: 20,
		opts: { message: 'Ship?', active: 'Yes, right now', inactive: 'No, not yet' },
		events: [escape],
		cancelled: true,
	},

	// --- password -----------------------------------------------------------------------------

	{
		// A row of masks long enough to wrap, with the cursor-shaped hole at the end of it — the one
		// place conceal and a wrap meet. The emulator cannot see conceal (ADR-0015), so what this
		// pins is where the row breaks; the opening-Frame comparison covers the attribute.
		name: 'password › a masked value wraps',
		kind: 'password',
		columns: NARROW,
		opts: { message: 'Passphrase' },
		events: [...typing('correct horse battery staple and then some'), enter],
		value: 'correct horse battery staple and then some',
	},
	{
		// Deleting back across the wrap, so the mask row shrinks rather than grows.
		name: 'password › deleting back across the wrap',
		kind: 'password',
		columns: NARROW,
		opts: { message: 'Passphrase' },
		events: [...typing('correct horse battery staple and then some'), ...backspace(21), enter],
		value: 'correct horse battery',
	},
	{
		// A custom mask that is wider than one column. Upstream slices `masked` at an offset counted
		// in units of `userInput`, which stops being the same thing the moment a mask is not one
		// character — so the cursor lands somewhere neither side would choose, and both have to land
		// there together.
		name: 'password › a wide mask puts the cursor somewhere nobody chose',
		kind: 'password',
		columns: NARROW,
		opts: { message: 'Passphrase', mask: '••' },
		events: [
			...typing('hunter2'),
			{ kind: 'key', s: '', key: { name: 'left' } },
			{ kind: 'key', s: '', key: { name: 'left' } },
			enter,
		],
		value: 'hunter2',
	},
	{
		// `masked` is `userInput.replaceAll(/./g, mask)` with no `u` flag, so `.` matches a UTF-16
		// unit and an astral character becomes *two* masks. Nothing in the harvest has one, and
		// mutating the rule to one-mask-per-character passes every other Scenario — so without this
		// case the behaviour is asserted only by a unit test written from the same reading that
		// produced it. The `left` then puts the cursor inside the pair.
		name: 'password › an astral character is two masks',
		kind: 'password',
		columns: NARROW,
		opts: { message: 'Passphrase' },
		events: [
			...typing('a\u{1F600}b'),
			{ kind: 'key', s: '', key: { name: 'left' } },
			{ kind: 'key', s: '', key: { name: 'left' } },
			enter,
		],
		value: 'a\u{1F600}b',
	},
	{
		// Cancelled and wrapped: struck through and dim, across a row break.
		name: 'password › a cancelled masked value wraps',
		kind: 'password',
		columns: NARROW,
		opts: { message: 'Passphrase' },
		events: [...typing('correct horse battery staple and then some'), escape],
		cancelled: true,
	},
	{
		// `groupMultiselect` wraps each option itself, before `limitOptions` wraps the row again —
		// and the width it uses is the terminal less `prefix.length`, where the prefix has already
		// been through `styleText`. So the same label breaks in two different places depending on
		// whether its branch was dimmed, which is the one thing in this Prompt no harvested Scenario
		// can see: upstream's suite is all short labels at eighty columns.
		//
		// The cursor opens on the first group's header, so that group's options are drawn
		// `group-active` — the one look whose prefix is passed unstyled — and the second group's are
		// dimmed. One recording, both widths.
		name: 'narrow › a group option wraps differently under a dimmed branch',
		kind: 'groupMultiselect',
		columns: NARROW,
		opts: {
			message: 'Pick',
			options: {
				Testing: [{ value: 'jest', label: 'Jest, a JavaScript testing framework' }],
				Language: [{ value: 'ts', label: 'TypeScript, which is a static type checker' }],
			},
		},
		events: [{ kind: 'key', s: ' ', key: { name: 'space' } }, enter],
		value: ['jest'],
	},
	{
		// And with the groups unselectable there is no branch and no closing corner — upstream sets
		// `prefix` to two spaces and leaves `prefixEnd` at the empty string it was declared with, so
		// the rows of a wrapped option after the first sit two columns to the *left* of its first.
		// Nothing upstream records this either.
		name: 'narrow › an unselectable group wraps its option back to the margin',
		kind: 'groupMultiselect',
		columns: NARROW,
		opts: {
			message: 'Pick',
			selectableGroups: false,
			options: {
				Language: [{ value: 'ts', label: 'TypeScript, which is a static type checker' }],
			},
		},
		events: [{ kind: 'key', s: ' ', key: { name: 'space' } }, enter],
		value: ['ts'],
	},
	{
		// `autocomplete` hands `limitOptions` a `columnPadding` of 3 — the bar and its two spaces,
		// counted as the columns they draw rather than as the characters they take with their escapes
		// around them. It is the only list Prompt in clack that does, and no harvested Scenario is
		// narrow enough to show it: upstream's are all short labels at eighty columns.
		name: 'narrow › an autocomplete option wraps three columns short of the terminal',
		kind: 'autocomplete',
		columns: NARROW,
		opts: {
			message: 'Pick',
			options: [
				{ value: 'ts', label: 'TypeScript, a static type checker for JS' },
				{ value: 'js', label: 'JavaScript, which is not one at all' },
			],
		},
		events: [enter],
		value: 'ts',
	},
	{
		// The same list under the other Prompt, which passes no `columnPadding` at all — so the same
		// label is wrapped as though the bar beside it were not there, and every row of it overruns
		// the terminal by those three columns. One pair of recordings, both widths.
		name: 'narrow › an autocompleteMultiselect option overruns its own guide',
		kind: 'autocompleteMultiselect',
		columns: NARROW,
		opts: {
			message: 'Pick',
			options: [
				{ value: 'ts', label: 'TypeScript, a static type checker for JS' },
				{ value: 'js', label: 'JavaScript, which is not one at all' },
			],
		},
		events: [tab, enter],
		value: ['ts'],
	},
	{
		// Not a width: a key. `userInputWithCursor` asks `_cursor` whether the text cursor is past the
		// end of the search text and then slices the text at `this.cursor`, which is the getter for
		// the *option list's* cursor. Nothing upstream records presses left in an autocomplete, so
		// nothing upstream can tell the two apart. Here the list cursor is on the second match and the
		// text cursor is at the start, so clack inverts the second character of a two-character search
		// and leaves the first one plain.
		name: 'autocomplete › the search cursor is drawn where the option cursor is',
		kind: 'autocomplete',
		columns: 80,
		opts: {
			message: 'Pick',
			options: [
				{ value: 'apple', label: 'Apple' },
				{ value: 'banana', label: 'Banana' },
				{ value: 'grape', label: 'Grape' },
			],
		},
		events: [...typing('ap'), down, left, left, enter],
		value: 'grape',
	},

	// --- date ---------------------------------------------------------------------------------
	//
	// Upstream's `date` suite is eight Scenarios and nine keypresses, seven of which are a bare
	// `return` on a field that was filled by `initialValue`. It never types a digit, never presses an
	// arrow, never tabs and never backspaces — so the segment editor, which is three hundred lines of
	// `DatePrompt`, reaches no recording at all. These are that editor.
	//
	// Every one of them passes an explicit `format`. Without one the constructor asks
	// `Intl.DateTimeFormat` which segments to draw and in what order, and the recording would be of
	// whichever machine ran the harvest rather than of clack.

	{
		// The ordinary path, keystroke by keystroke: a lone digit into an empty month becomes `07`
		// and hands the cursor on, one that could still be a tens digit waits for a second, and the
		// segment advances as each fills. Ten Frames, none of which any harvested Scenario draws.
		name: 'date › a whole date typed straight through',
		kind: 'date',
		columns: 80,
		opts: { message: 'Pick a date', format: 'MDY' },
		events: [...typing('12252025'), enter],
		value: utc('2025-12-25'),
	},
	{
		// A year fills from the *right* — `(digits + char).padStart(4, '_')` — so it shows as `___2`,
		// `__20`, `_202` and only then `2025`. The other two segments fill from the left. Nothing
		// upstream draws a partly-typed segment of either kind, and the underscores of one the cursor
		// has left are drawn as dim spaces rather than as underscores, which is a style boundary in
		// the middle of a number.
		name: 'date › a year fills from the right',
		kind: 'date',
		columns: 80,
		opts: { message: 'Pick a date', format: 'YMD', separator: '-' },
		events: [...typing('20250214'), enter],
		value: utc('2025-02-14'),
	},
	{
		// The inline error, which is a row of its own between the field and the foot and is the one
		// thing in this Prompt that is neither a value nor a validation failure. And the trap behind
		// it: a refused two-digit entry leaves the segment *unselected*, so the next digit is written
		// into position 0 of what is still there — `01` refused a `9`, then refuses a `7` as `71`.
		// A backspace is the only way out that is not a move.
		name: 'date › a month over twelve is refused, and refuses the next digit too',
		kind: 'date',
		columns: 80,
		opts: { message: 'Pick a date', format: 'MDY' },
		events: [...typing('19'), ...typing('7'), ...backspace(1), ...typing('7152025'), enter],
		value: utc('2025-07-15'),
	},
	{
		// The same row without a Guide, where the bar in front of it is the empty string.
		name: 'date › an unguided inline error',
		kind: 'date',
		columns: 80,
		opts: { message: 'Pick a date', format: 'MDY', withGuide: false },
		events: [...typing('19'), escape],
		cancelled: true,
	},
	{
		// The arrows on blank segments, which jump to a bound rather than stepping: up lands on the
		// minimum and down on the maximum. A blank day's maximum is `daysInMonth(0, 0)` — the two `||`
		// fallbacks make that January — so a day of 31 is offered before any month is known. And a
		// blank year's minimum is 1, which `Date.UTC` reads as 1901, so `0001` is drawn, accepted by
		// every segment check, and still resolves to nothing: the Prompt then demands an answer it is
		// already showing.
		name: 'date › the arrows fill a blank field with a date that is not one',
		kind: 'date',
		columns: 80,
		opts: { message: 'Pick a date', format: 'DMY', separator: '.' },
		events: [down, right, up, right, up, enter, escape],
		cancelled: true,
	},
	{
		// `minDate` and `maxDate` bound the arrows as well as the answer, and only inside their own
		// year and month — so a day is held between the tenth and the twentieth while the month either
		// side of it is free.
		name: 'date › the arrows are held between a minimum and a maximum',
		kind: 'date',
		columns: 80,
		opts: {
			message: 'Pick a date',
			format: 'MDY',
			initialValue: utc('2025-01-15'),
			minDate: utc('2025-01-10'),
			maxDate: utc('2025-01-20'),
		},
		events: [right, ...repeat(10, down), ...repeat(20, up), enter],
		value: utc('2025-01-20'),
	},
	{
		// `defaultValue` reaches the fallback it is documented as only once the field it also seeded
		// has been erased. What is drawn then is the third thing worth recording here: the submitted
		// Frame asks whether `this.value` is a `Date` and then prints `formattedValue`, which is the
		// segments — so a Prompt that answered 2025-12-25 writes `__/__/____` on the terminal.
		name: 'date › a default value is answered with while the field shows underscores',
		kind: 'date',
		columns: 80,
		opts: { message: 'Pick a date', format: 'MDY', defaultValue: utc('2025-12-25') },
		events: [...backspace(1), right, ...backspace(1), right, ...backspace(1), enter],
		value: utc('2025-12-25'),
	},
	{
		// Tab and shift-tab, which walk the segments the way the arrows do except that they refuse to
		// move rather than clamping at the ends. The year is filled first and the month last, so the
		// walk is doing the work rather than the auto-advance.
		name: 'date › tab and shift-tab walk the segments',
		kind: 'date',
		columns: 80,
		opts: { message: 'Pick a date', format: 'MDY' },
		events: [
			tab,
			tab,
			...typing('2025'),
			shiftTab,
			shiftTab,
			...typing('07'),
			...typing('04'),
			enter,
		],
		value: utc('2025-07-04'),
	},
	{
		// Backspace blanks the segment under the cursor, and a second one on the segment it has just
		// blanked steps *backwards* instead — which at the first segment means it does nothing, and
		// which is why erasing a whole field means walking forwards by hand.
		name: 'date › backspace blanks a segment and then steps back',
		kind: 'date',
		columns: 80,
		opts: { message: 'Pick a date', format: 'MDY', initialValue: utc('2025-06-15') },
		events: [right, ...backspace(1), left, ...backspace(2), ...typing('0704'), enter],
		value: utc('2025-07-04'),
	},
	{
		// A year can be typed past its own length: `padStart` pads and does not truncate, so a fifth
		// digit into a full year makes it five characters wide. Only a year, and only when it is the
		// last segment — one that is not hands the cursor on the moment it fills.
		name: 'date › a fifth digit makes the year five characters wide',
		kind: 'date',
		columns: 80,
		opts: { message: 'Pick a date', format: 'MDY' },
		events: [...typing('12'), ...typing('25'), ...typing('20255'), escape],
		cancelled: true,
	},
];
