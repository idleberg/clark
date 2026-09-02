// The static Recorder's cases: `log`, `intro`, `outro`, `cancel`, `note` and `box` — the renderers
// that are not Prompts.
//
// A third corpus, and the simplest of the three. There is no state machine here and no keypress: a
// case is a function, its message, its options and the terminal it is written to, and what clack
// writes is one string. So unlike ../authored/cases.mjs there is no `value` to declare — a case
// that ran at all recorded everything there was, and a case that threw recorded nothing.
//
// Upstream's own suite covers `log` well, `note` thinly, and `intro`/`outro`/`cancel`/`box` not at
// all (they have no test file). What it does not cover anywhere is a message wider than the
// terminal, which is a claim worth a recording either way it falls: the first four hand the line to
// the terminal and let it break, and `note` and `box` break it themselves because they draw a
// right-hand border.

/** Whatever `log.message` is given, at the defaults. */
const log = (name, message, options = {}) => ({ name, kind: 'log', message, options });

/** A `note` at eighty columns. */
const note = (name, message, title, options = {}) => ({ name, kind: 'note', message, title, options });

/** A `box` at eighty columns, unless the case says otherwise. */
const box = (name, message, title, options = {}, columns = 80) => ({
	name,
	kind: 'box',
	message,
	title,
	options,
	columns,
});

const wide = 'ばんは'.repeat(30);
const long = 'lorem ipsum dolor sit amet '.repeat(5);

export const cases = [
	// --- log.message ---------------------------------------------------------------------------

	log('a message under a bar', 'message'),
	log('a message of several lines', 'line 1\nline 2\nline 3'),
	log('a blank line keeps its bar and loses its spaces', 'foo\n\nbar'),
	log('a message with nothing in it is still a row', ''),
	log('spacing is rows above the message', 'spaced message', { spacing: 3 }),
	log('no spacing at all', 'tight message', { spacing: 0 }),
	log('a symbol of its own, and a second one for the rows below', 'custom\nsymbols', {
		symbol: '>>',
		secondarySymbol: '--',
	}),
	log('with the guide off', 'standalone message', { withGuide: false }),
	log('with the guide off, a blank line is blank', 'foo\n\nbar', { withGuide: false }),
	log('with the guide off and nothing to say', '', { withGuide: false }),

	// Nothing in `log` wraps. Both of these are longer than the terminal and are left for it to
	// break, which is the one thing these renderers do that a Prompt's Frame never does.
	{ name: 'a line longer than the terminal', kind: 'log', message: long, options: {} },
	{ name: 'a line of wide characters longer than the terminal', kind: 'log', message: wide, options: {} },
	{ name: 'a long line at forty columns', kind: 'log', message: long, options: {}, columns: 40 },

	// --- the five named symbols ----------------------------------------------------------------

	{ name: 'info', kind: 'log.info', message: 'info message', options: {} },
	{ name: 'success', kind: 'log.success', message: 'success message', options: {} },
	{ name: 'step', kind: 'log.step', message: 'step message', options: {} },
	{ name: 'warn', kind: 'log.warn', message: 'warn message', options: {} },
	{ name: 'error', kind: 'log.error', message: 'error message', options: {} },
	{
		name: 'a named symbol on a message of several lines',
		kind: 'log.warn',
		message: 'first\nsecond',
		options: {},
	},
	{
		name: 'a named symbol with the guide off',
		kind: 'log.error',
		message: 'error message',
		options: { withGuide: false },
	},

	// --- intro, outro, cancel ------------------------------------------------------------------

	{ name: 'intro', kind: 'intro', message: 'create-app', options: {} },
	{ name: 'intro with the guide off', kind: 'intro', message: 'create-app', options: { withGuide: false } },
	{ name: 'intro with nothing to say', kind: 'intro', message: '', options: {} },
	{ name: 'outro', kind: 'outro', message: "You're all set!", options: {} },
	{ name: 'outro with the guide off', kind: 'outro', message: "You're all set!", options: { withGuide: false } },
	// The two spaces after the corner are written whether or not there is a message.
	{ name: 'outro with nothing to say', kind: 'outro', message: '', options: {} },
	{ name: 'cancel', kind: 'cancel', message: 'Installation canceled', options: {} },
	{
		name: 'cancel with the guide off',
		kind: 'cancel',
		message: 'Installation canceled',
		options: { withGuide: false },
	},
	{ name: 'cancel with nothing to say', kind: 'cancel', message: '', options: {} },
	{ name: 'an outro longer than the terminal', kind: 'outro', message: long, options: {}, columns: 40 },

	// --- note ------------------------------------------------------------------------------------
	//
	// The first static renderer that reads the terminal's width, because it draws a right-hand border
	// and a border only lands in the right column if what is inside it was measured first. Upstream's
	// own note.test.ts is the source of most of these; the width the box settles on is the claim.

	note('a message with a title', 'message', 'title'),
	note('as wide as the longest line', 'short\nsomewhat questionably long line', 'title'),
	note('a title wider than the message', 'hi', 'a considerably longer title'),
	note('nothing to say', '', 'title'),
	note('no title either', 'message', ''),
	note('without the guide', 'message', 'title', { withGuide: false }),
	note('a message of blank lines', 'a\n\nb', 'title'),

	// `format` names one of the recorder's formatters; a Fixture cannot hold a function.
	note('a formatter that adds characters', 'line 0\nline 1\nline 2', 'title', { format: 'stars' }),
	note('a formatter that adds only colour', 'line 0\nline 1\nline 2', 'title', { format: 'red' }),

	// Upstream's two overflow cases, at its own seventy-five columns.
	{
		name: "don't overflow",
		kind: 'note',
		message: `${'test string '.repeat(32)}\n`.repeat(4).trim(),
		title: 'title',
		options: {},
		columns: 75,
	},
	{
		name: "don't overflow with a formatter",
		kind: 'note',
		message: `${'test string '.repeat(32)}\n`.repeat(4).trim(),
		title: 'title',
		options: { format: 'red-stars' },
		columns: 75,
	},
	// Ten columns, which leaves four for the message — every wide character is a row of its own.
	{
		name: 'wide characters in a narrow terminal',
		kind: 'note',
		message: '이게 첫 번째 줄이에요\nこれは次の行です',
		title: '这是标题',
		options: {},
		columns: 10,
	},
	{
		name: 'wide characters in a narrow terminal, with a formatter',
		kind: 'note',
		message: '이게 첫 번째 줄이에요\nこれは次の行です',
		title: '这是标题',
		options: { format: 'red-stars' },
		columns: 10,
	},
	// A formatter that costs more columns than the terminal has: the wrap width reaches zero, which
	// upstream lays out one code point per row.
	{
		name: 'a formatter wider than the terminal leaves it',
		kind: 'note',
		message: 'abcdef',
		title: 't',
		options: { format: 'stars' },
		columns: 10,
	},

	// --- box -------------------------------------------------------------------------------------
	//
	// `note`'s configurable cousin, and it decides its width the other way round: a `note` measures
	// its content and fits a box to it, a `box` settles on a width and fits the content to that. So
	// the claims here are the arithmetic — the even-width nudge, the alignments, the title it
	// truncates where a `note` never does — and the fraction upstream's `width` number turns out to
	// be.

	box('a message with a title', 'message', 'title'),
	box('several lines', 'line 1\nline 2\nline 3', 'title'),
	box('no title', 'message', ''),
	box('nothing to say', '', 'title'),
	box('without the guide', 'message', 'title', { withGuide: false }),
	// `rounded` is documented `@default true` and read as `opts?.rounded`, so the cases above are all
	// square and these are the only round ones.
	box('rounded corners', 'message', 'title', { rounded: true }),
	box('rounded corners without the guide', 'message', 'title', { rounded: true, withGuide: false }),
	box('square corners, said out loud', 'message', 'title', { rounded: false }),

	// --- box: alignment ----------------------------------------------------------------------------

	box('content centred', 'short\na much longer line', 'title', { contentAlign: 'center' }),
	box('content to the right', 'short\na much longer line', 'title', { contentAlign: 'right' }),
	box('title centred', 'a reasonably long line of content', 'title', { titleAlign: 'center' }),
	box('title to the right', 'a reasonably long line of content', 'title', { titleAlign: 'right' }),
	box('everything centred at a fixed width', 'short\nlonger line', 'title', {
		width: 1,
		contentAlign: 'center',
		titleAlign: 'center',
	}),

	// --- box: padding ------------------------------------------------------------------------------
	//
	// The title's padding is measured in border characters and the content's in spaces, which is the
	// one thing about these two numbers that is not symmetrical.

	box('no padding at all', 'message', 'title', { titlePadding: 0, contentPadding: 0 }),
	box('a generous title padding', 'message', 'title', { titlePadding: 5 }),
	box('a generous content padding', 'message', 'title', { contentPadding: 6 }),
	box('no title padding, centred', 'message', 'title', { titlePadding: 0, titleAlign: 'center' }),

	// --- box: width --------------------------------------------------------------------------------
	//
	// Every case above leaves `width` out, and every one of them is the width of the terminal: the
	// shrink-to-content branch is guarded by `opts?.width === 'auto'`, which an omitted option does
	// not satisfy, so the documented default is not the one you get. These are the ones that ask.

	box('auto, which is what the default is documented to be', 'message', 'title', { width: 'auto' }),
	box('auto with several lines', 'short\na much longer line of content', 'title', { width: 'auto' }),
	box('auto with a title wider than the content', 'hi', 'a considerably longer title', { width: 'auto' }),
	box('auto with nothing to say', '', '', { width: 'auto' }),
	box('auto with generous padding', 'message', 'title', { width: 'auto', titlePadding: 4, contentPadding: 5 }),
	// `auto` only ever shrinks: content wider than the terminal leaves the box the terminal's width.
	{
		name: 'auto with content wider than the terminal',
		kind: 'box',
		message: long,
		title: 'title',
		options: { width: 'auto' },
		columns: 40,
	},
	box('auto, centred', 'short\na much longer line', 'title', {
		width: 'auto',
		contentAlign: 'center',
		titleAlign: 'center',
	}),
	box('auto in a narrow terminal', 'hi', 'title', { width: 'auto' }, 10),
	box('auto with a title of wide characters at the end', 'hi', `${'a'.repeat(12)}这这`, { width: 'auto' }, 20),
	// The cut lands *inside* a surrogate pair here, which is the one place a code-unit slice and a
	// code-point one part company: eleven units is five emoji and half of a sixth.
	box('auto with a title cut through a surrogate pair', 'hi', '\u{1f600}'.repeat(8), { width: 'auto' }, 20),

	box('half the terminal', 'message', 'title', { width: 0.5 }),
	box('a third of the terminal', 'message', 'title', { width: 0.34 }),
	box('all of the terminal', 'message', 'title', { width: 1 }),
	// `Math.min(1, width)`: the option documented as a column count is a fraction, so this is the
	// whole terminal and not forty columns of it.
	box('forty, which is not forty columns', 'message', 'title', { width: 40 }),
	// An odd width with no room to grow into is nudged *down*, where every other one is nudged up.
	box('an odd terminal with nothing spare', 'a line long enough to fill it', 'title', { width: 1 }, 41),
	box('an odd terminal with room to spare', 'message', 'title', {}, 41),

	// --- box: the title it truncates ---------------------------------------------------------------

	box('a title wider than the box', 'hi', 'a considerably longer title', {}, 20),
	// The slice is by UTF-16 code unit and the truncation test is by column, so a title that is wider
	// than it is long is cut at a different place than a width-counting port would cut it. The wide
	// characters sit at the end, where the cut cannot land inside one.
	box('a title of wide characters at the end', 'hi', `${'a'.repeat(12)}这这`, {}, 20),
	box('a title truncated with the guide off', 'hi', 'a considerably longer title', { withGuide: false }, 20),

	// --- box: wrapping and narrow terminals --------------------------------------------------------

	{ name: 'a message longer than the box', kind: 'box', message: long, title: 'title', options: {}, columns: 80 },
	{ name: 'wide characters in a narrow box', kind: 'box', message: wide, title: 't', options: {}, columns: 20 },
	box('a narrow terminal', 'hi', 'title', {}, 10),
	box('a narrow terminal with no padding', 'hi there', 'title', { titlePadding: 0, contentPadding: 0 }, 10),

	// --- box: formatBorder -------------------------------------------------------------------------
	//
	// Applied to each border character *before* it is repeated, so a formatter that returns an escape
	// has it reopened per column, and one that returns characters makes the border wider than the box
	// the widths were computed for. Both are recorded.

	box('a coloured border', 'message', 'title', { formatBorder: 'red' }),
	box('a border wider than it was measured', 'message', 'title', { formatBorder: 'stars' }),
	box('a coloured border with rounded corners', 'message', 'title', { formatBorder: 'red', rounded: true }),
];
