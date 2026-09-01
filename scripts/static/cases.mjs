// The static Recorder's cases: `log`, `intro`, `outro` and `cancel`, which are not Prompts.
//
// A third corpus, and the simplest of the three. There is no state machine here and no keypress: a
// case is a function, its message, its options and the terminal it is written to, and what clack
// writes is one string. So unlike ../authored/cases.mjs there is no `value` to declare — a case
// that ran at all recorded everything there was, and a case that threw recorded nothing.
//
// Upstream's own suite covers `log` well and `intro`/`outro`/`cancel` not at all (they have no test
// file). What it does not cover anywhere is a message wider than the terminal: nothing here wraps,
// which is a claim worth a recording, because a Prompt's Frame is wrapped by clack itself and these
// are left to the terminal.

/** Whatever `log.message` is given, at the defaults. */
const log = (name, message, options = {}) => ({ name, kind: 'log', message, options });

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
];
