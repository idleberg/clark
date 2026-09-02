// Records what Node's `readline` does to (line, cursor) for a corpus of key sequences, so that
// `crates/clackatui-core/tests/line_editor_parity.rs` can assert the port against it without needing a
// JavaScript runtime on CI. Same arrangement as scripts/harvest-width.mjs; see ADR-0008.
//
//   node scripts/harvest-line-editor.mjs
//
// Every string is written as an array of code points, never as a literal, so that nothing here is
// silently normalised on its way to disk and nothing invisible hides in the source.

import { writeFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { PassThrough } from 'node:stream';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const OUT = join(
	dirname(fileURLToPath(import.meta.url)),
	'..',
	'crates',
	'clackatui-core',
	'tests',
	'fixtures',
	'line-editor.json'
);

/** Code points of a string, the only representation this file stores. */
const a = (s) => [...s].map((c) => c.codePointAt(0));

// --- keys -----------------------------------------------------------------------------------
//
// Shaped the way `emitKeypressEvents` shapes them, because `rl.write(d, key)` in terminal mode
// hands its arguments straight to `kTtyWrite`.

/** A printable character: readline names the key after the character itself. */
const type = (ch) => ({ s: ch, key: { name: ch, sequence: ch } });

/** A named key with no modifiers. */
const press = (name, sequence = undefined) => ({ s: null, key: { name, sequence } });

/** ctrl + a letter. The sequence is the C0 control the terminal actually sends. */
const ctrl = (ch) => ({
	s: null,
	key: {
		name: ch,
		ctrl: true,
		sequence: String.fromCharCode(ch.charCodeAt(0) - 0x60),
	},
});

/** ctrl + a named key. */
const ctrlKey = (name) => ({ s: null, key: { name, ctrl: true } });

/** ctrl + shift + a named key. */
const ctrlShiftKey = (name) => ({ s: null, key: { name, ctrl: true, shift: true } });

/** alt/meta + a letter. */
const meta = (ch) => ({ s: null, key: { name: ch, meta: true, sequence: `\u001b${ch}` } });

/** alt/meta + a named key. */
const metaKey = (name) => ({ s: null, key: { name, meta: true } });

/** ctrl+_ and ctrl+^, which readline dispatches on the sequence rather than the name. */
const undo = { s: null, key: { sequence: '\u001f' } };
const redo = { s: null, key: { sequence: '\u001e' } };

/** Type a whole string, one keypress per character, the way a terminal delivers it. */
const typeAll = (s) => [...s].map(type);

// --- corpus ---------------------------------------------------------------------------------
//
// Each scenario replays from an empty editor. Steps are recorded individually, so a divergence is
// reported at the keypress that caused it rather than at the end of the run.
//
// Deliberately absent: ctrl+c, and ctrl+d on an empty line. Both close the interface upstream, so
// they cannot be recorded as a (line, cursor) transition; they are covered by unit tests instead.
// So are up/down and tab completion, which need a history and a completer that clack never gives
// readline. See the module docs on `line_editor`.

const scenarios = [
	{ name: 'typing appends', steps: typeAll('hello') },
	{
		name: 'insert in the middle',
		steps: [...typeAll('ac'), press('left'), type('b')],
	},
	{
		name: 'arrows clamp at both ends',
		steps: [
			...typeAll('ab'),
			press('left'),
			press('left'),
			press('left'),
			press('right'),
			press('right'),
			press('right'),
		],
	},
	{
		name: 'home and end',
		steps: [...typeAll('hello'), press('home'), press('end'), press('home')],
	},
	{
		name: 'ctrl a and ctrl e',
		steps: [...typeAll('hello'), ctrl('a'), ctrl('e'), ctrl('a')],
	},
	{
		name: 'ctrl b and ctrl f',
		steps: [...typeAll('abc'), ctrl('b'), ctrl('b'), ctrl('f'), ctrl('f'), ctrl('f')],
	},
	{
		name: 'backspace and ctrl h',
		steps: [...typeAll('abc'), press('backspace'), ctrl('h'), press('backspace'), press('backspace')],
	},
	{
		name: 'delete forward',
		steps: [...typeAll('abc'), ctrl('a'), press('delete'), press('delete'), press('delete')],
	},
	{
		name: 'ctrl d deletes forward',
		steps: [...typeAll('abc'), ctrl('a'), ctrl('d'), ctrl('e'), ctrl('d')],
	},

	// --- word boundaries ---------------------------------------------------------------------

	{
		name: 'alt b over words',
		steps: [...typeAll('foo bar baz'), meta('b'), meta('b'), meta('b'), meta('b')],
	},
	{
		name: 'alt f over words',
		steps: [...typeAll('foo bar baz'), ctrl('a'), meta('f'), meta('f'), meta('f'), meta('f')],
	},
	{
		name: 'ctrl left and ctrl right',
		steps: [...typeAll('foo bar'), ctrlKey('left'), ctrlKey('right'), ctrlKey('left')],
	},
	{
		name: 'punctuation is its own class',
		steps: [...typeAll('a->b.c'), ctrl('a'), meta('f'), meta('f'), meta('f'), meta('f')],
	},
	{
		name: 'underscores and digits are word characters',
		steps: [...typeAll('foo_bar9 baz'), ctrl('a'), meta('f'), meta('f')],
	},
	{
		name: 'leading whitespace is crossed first',
		steps: [...typeAll('   foo'), meta('b'), meta('b')],
	},
	{
		name: 'ctrl w deletes word left',
		steps: [...typeAll('foo bar baz'), ctrl('w'), ctrl('w'), ctrl('w'), ctrl('w')],
	},
	{
		name: 'alt backspace deletes word left',
		steps: [...typeAll('foo bar'), metaKey('backspace')],
	},
	{
		name: 'ctrl backspace deletes word left',
		steps: [...typeAll('foo bar'), ctrlKey('backspace')],
	},
	{
		name: 'alt d deletes word right',
		steps: [...typeAll('foo bar baz'), ctrl('a'), meta('d'), meta('d'), meta('d')],
	},
	{
		name: 'ctrl delete deletes word right',
		steps: [...typeAll('foo bar'), ctrl('a'), ctrlKey('delete')],
	},
	{
		name: 'alt delete deletes word right',
		steps: [...typeAll('foo bar'), ctrl('a'), metaKey('delete')],
	},
	// The one place upstream's two word patterns disagree: kWordRight uses [^\w\s]+ and
	// kDeleteWordRight uses \W+, which also swallows whitespace.
	{
		name: 'word right stops inside a punctuation run',
		steps: [...typeAll('! !x'), ctrl('a'), meta('f')],
	},
	{
		name: 'delete word right swallows the whole punctuation run',
		steps: [...typeAll('! !x'), ctrl('a'), meta('d')],
	},
	{
		name: 'word motion in the middle of a word',
		steps: [...typeAll('foobar'), press('left'), press('left'), meta('b'), meta('f')],
	},

	// --- kill and yank -----------------------------------------------------------------------

	{
		name: 'ctrl u and ctrl k',
		steps: [...typeAll('foobar'), press('left'), press('left'), press('left'), ctrl('k'), ctrl('u')],
	},
	{
		name: 'ctrl u on an empty line',
		steps: [ctrl('u'), ctrl('u')],
	},
	{
		name: 'ctrl shift backspace and delete',
		steps: [...typeAll('foobar'), ctrl('a'), ctrlShiftKey('delete'), ...typeAll('xy'), ctrlShiftKey('backspace')],
	},
	{
		name: 'yank a kill back',
		steps: [...typeAll('foo bar'), ctrl('u'), ctrl('y')],
	},
	{
		name: 'yank into the middle',
		steps: [...typeAll('abc'), ctrl('u'), ...typeAll('xy'), press('left'), ctrl('y')],
	},
	{
		name: 'yank pop walks the ring',
		steps: [...typeAll('first'), ctrl('u'), ...typeAll('second'), ctrl('u'), ctrl('y'), meta('y'), meta('y')],
	},
	{
		name: 'yank pop needs a yank first',
		steps: [...typeAll('first'), ctrl('u'), ...typeAll('second'), ctrl('u'), ctrl('y'), press('left'), meta('y')],
	},
	{
		name: 'a duplicate kill is not pushed twice',
		steps: [...typeAll('same'), ctrl('u'), ...typeAll('same'), ctrl('u'), ...typeAll('other'), ctrl('u'), ctrl('y'), meta('y'), meta('y')],
	},
	{
		name: 'yank with an empty ring does nothing',
		steps: [...typeAll('ab'), ctrl('y')],
	},

	// --- undo and redo -----------------------------------------------------------------------

	{
		name: 'undo steps back through typing',
		steps: [...typeAll('abc'), undo, undo, undo, undo],
	},
	{
		name: 'redo steps forward again',
		steps: [...typeAll('abc'), undo, undo, redo, redo, redo],
	},
	{
		name: 'movement is not an edit',
		steps: [...typeAll('ab'), press('left'), press('left'), undo],
	},
	{
		name: 'undo restores a killed line',
		steps: [...typeAll('foo bar'), ctrl('u'), undo],
	},
	{
		name: 'undo after a word delete',
		steps: [...typeAll('foo bar'), ctrl('w'), undo, redo],
	},

	// --- non-ASCII ---------------------------------------------------------------------------

	// An astral code point is two UTF-16 units and one cursor step.
	{
		name: 'astral code points move as one',
		steps: [type('\u{1F600}'), type('\u{1F602}'), press('left'), press('left'), press('right')],
	},
	{
		name: 'backspace deletes a whole astral code point',
		steps: [...typeAll('a'), type('\u{1F600}'), press('backspace')],
	},
	// Combining marks are separate stops: readline steps by code point, not by grapheme.
	{
		name: 'combining marks are separate stops',
		steps: [type('e'), type('\u0301'), press('left'), press('left'), press('right')],
	},
	{
		name: 'backspace peels one combining mark at a time',
		steps: [type('e'), type('\u0301'), type('\u0302'), press('backspace'), press('backspace')],
	},
	{
		name: 'latin one supplement is one unit wide in the buffer',
		steps: [type('\u00e9'), type('\u00e8'), press('left'), press('backspace')],
	},
	{
		name: 'cjk cursor steps',
		steps: [type('\u65e5'), type('\u672c'), press('left'), press('backspace')],
	},
	// JavaScript's \s includes U+FEFF, which most whitespace predicates do not.
	{
		name: 'a byte order mark counts as whitespace',
		steps: [...typeAll('foo'), type('\ufeff'), ...typeAll('bar'), ctrl('w'), ctrl('w')],
	},
	{
		name: 'an ideographic space counts as whitespace',
		steps: [...typeAll('foo'), type('\u3000'), ...typeAll('bar'), meta('b')],
	},
	// Non-ASCII letters are not \w without the u flag, so they group with punctuation.
	{
		name: 'accented letters are not word characters',
		steps: [...typeAll('caf'), type('\u00e9'), ...typeAll(' x'), ctrl('a'), meta('f')],
	},
	{
		name: 'word delete across an astral code point',
		steps: [...typeAll('ab'), type('\u{1F600}'), ...typeAll('cd'), ctrl('w')],
	},

	// --- odds and ends -----------------------------------------------------------------------

	{
		name: 'escape is ignored',
		steps: [...typeAll('ab'), { s: '\u001b', key: { name: 'escape', sequence: '\u001b' } }, ...typeAll('c')],
	},
	{
		name: 'tab types a tab',
		steps: [...typeAll('a'), { s: '\t', key: { name: 'tab', sequence: '\t' } }, ...typeAll('b')],
	},
	{
		name: 'up and down are inert with no history',
		steps: [...typeAll('ab'), press('up'), press('down'), ctrl('p'), ctrl('n')],
	},
	{
		name: 'return clears the line',
		steps: [...typeAll('hello'), press('return', '\r'), ...typeAll('x')],
	},
	{
		name: 'return discards the undo history',
		steps: [...typeAll('hello'), press('return', '\r'), undo],
	},
	{
		name: 'a pasted string with newlines submits each line',
		steps: [{ s: 'one\r\ntwo\rthree\nfour', key: {} }],
	},
	{
		name: 'a write with no key inserts',
		steps: [{ s: 'inserted', key: {} }],
	},
	{
		name: 'ctrl d at the end of a line does nothing',
		steps: [...typeAll('ab'), ctrl('d')],
	},
	{
		name: 'delete at the end does nothing',
		steps: [...typeAll('ab'), press('delete'), press('delete')],
	},
	{
		name: 'backspace on an empty line does nothing',
		steps: [press('backspace'), press('backspace')],
	},
	{
		name: 'word motion on an empty line does nothing',
		steps: [meta('b'), meta('f'), ctrl('w'), meta('d')],
	},
	{
		name: 'a long edit session',
		steps: [
			...typeAll('the quick brown fox'),
			ctrl('a'),
			meta('f'),
			meta('f'),
			ctrl('k'),
			...typeAll('lazy dog'),
			meta('b'),
			ctrl('w'),
			undo,
			ctrl('e'),
			ctrl('u'),
			ctrl('y'),
		],
	},
];

// --- harvest --------------------------------------------------------------------------------

/** Node crashes on an unknown key name only if it matches a branch; unknown names simply fall
 *  through, which is exactly what we want to record. */
function replay(steps) {
	const input = new PassThrough();
	const output = new PassThrough();
	output.columns = 80;
	output.rows = 24;
	output.resume();

	const rl = createInterface({
		input,
		output,
		terminal: true,
		prompt: '',
		tabSize: 2,
		escapeCodeTimeout: 50,
	});

	const recorded = [];
	for (const { s, key } of steps) {
		rl.write(s ?? null, key);
		recorded.push({
			s: s == null ? null : a(s),
			key: {
				name: key.name ?? null,
				ctrl: key.ctrl === true,
				meta: key.meta === true,
				shift: key.shift === true,
				sequence: key.sequence == null ? null : a(key.sequence),
			},
			line: a(rl.line),
			cursor: rl.cursor,
		});
	}
	rl.close();
	return recorded;
}

const fixture = {
	source: 'node:readline',
	node: process.versions.node,
	generatedBy: 'scripts/harvest-line-editor.mjs',
	scenarios: scenarios.map(({ name, steps }) => ({ name, steps: replay(steps) })),
};

writeFileSync(OUT, `${JSON.stringify(fixture, null, '\t')}\n`);

const keypresses = fixture.scenarios.reduce((n, s) => n + s.steps.length, 0);
console.log(`${fixture.scenarios.length} scenarios, ${keypresses} keypresses -> ${OUT}`);
