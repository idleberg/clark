// Harvests clackatui-core/tests/fixtures/wrap.json from the real fast-wrap-ansi.
//
// The Rust port in clackatui-core/src/wrap.rs is checked against the fixture, never against a live
// Node process, for the reason docs/adr/0008 gives: prior-art/ is not committed, so CI has no
// JavaScript to compare with. This script is how the fixture is refreshed when the pinned clack
// version moves.
//
// Run it from the repository root:
//
//     node scripts/harvest-wrap.mjs
//
// Options are clack's and are not varied: `@clack/core`'s render calls
// wrapAnsi(frame, columns, { hard: true, trim: false }), and no Prompt can reach any other
// configuration.
//
// Every string is written as an array of code points rather than a literal, for the reason
// harvest-width.mjs gives: a decomposed sequence written literally gets silently precomposed
// somewhere between the editor and the filesystem, and then the fixture is not what it claims.
//
// The corpus is free of ANSI escapes on purpose. Upstream closes and reopens the open SGR code
// across a break, which is the one place it inserts bytes rather than only breaking; a Frame holds
// no escapes at all (docs/adr/0011), so the port covers the escape-free half and says so.

import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { readFileSync, writeFileSync } from 'node:fs';

const HERE = new URL('.', import.meta.url);
const ANCHOR = new URL('../prior-art/clack/packages/core/package.json', HERE);
const FIXTURE = new URL('../clackatui-core/tests/fixtures/wrap.json', HERE);

const require = createRequire(ANCHOR);
const entry = require.resolve('fast-wrap-ansi');
const { wrapAnsi } = await import(pathToFileURL(entry));
const pkgPath = entry.replace(/lib[/\\]main\.js$/, 'package.json');
const { version } = JSON.parse(readFileSync(pkgPath, 'utf8'));

const OPTIONS = { hard: true, trim: false };

/** ASCII and the shapes clack draws are safe to write literally: nothing normalises them. */
const a = (s) => [...s].map((c) => c.codePointAt(0));

const BAR = 0x2502; // │
const CORNER = 0x2514; // └
const STEP = 0x25c6; // ◆

const CORPUS = [
	// --- nothing to wrap ---------------------------------------------------------------------
	['empty', [], 10],
	['one short word', a('hello'), 10],
	['exactly the width', a('hello'), 5],
	['one column over', a('hello'), 4],

	// --- words and the spaces between them -----------------------------------------------------
	['two words that fit', a('ab cd'), 10],
	['two words that do not', a('ab cd'), 4],
	['break falls on the space', a('abc de'), 3],
	['double space', a('a  b'), 3],
	['double space at the margin', a('ab  cd'), 4],
	['leading space', a(' ab'), 3],
	['trailing space', a('ab '), 3],
	['only spaces', a('   '), 2],
	['run of spaces mid line', a('a    b'), 3],
	['many short words', a('a b c d e f g h'), 5],
	['word exactly filling a row', a('abcd ef'), 4],

	// --- words longer than a row -----------------------------------------------------------------
	['long word alone', a('abcdefghij'), 4],
	['long word after a short one', a('ab abcdefghij'), 4],
	['long word after a full row', a('abcd abcdefghij'), 4],
	['two long words', a('abcdefg hijklmn'), 4],
	['long word, one column', a('abc'), 1],
	['long word, two columns', a('abcde'), 2],
	['word exactly twice the width', a('abcdefgh'), 4],
	['word one over twice the width', a('abcdefghi'), 4],

	// --- widths that are not one ------------------------------------------------------------------
	['cjk that fits', [0x4f60, 0x597d], 4],
	['cjk over the margin', [0x4f60, 0x597d], 3],
	['cjk pair per row', [0x4f60, 0x597d, 0x4f60], 2],
	['cjk against an odd margin', [...a('a'), 0x4f60], 2],
	['emoji sequence broken mid sequence', [0x1f468, 0x200d, 0x1f469, 0x200d, 0x1f467], 2],
	['emoji sequence that fits', [0x1f600, ...a(' ok')], 8],
	['combining mark at the margin', [0x0061, 0x0062, 0x0301, 0x0063], 2],
	['word of nothing but marks', [0x0061, 0x0020, 0x0301, 0x0301], 1],
	['tab', [...a('a'), 0x0009, ...a('b')], 4],
	['tab alone', [0x0009], 4],
	['conjoining jamo', [0x1100, 0x1161, 0x11a8], 4],

	// --- no columns at all ------------------------------------------------------------------------
	// `limitOptions` wraps each option to `columns - columnPadding`, and `select` passes 13 for the
	// padding (ADR-0019), so a terminal 13 columns or narrower reaches this. Upstream gets there by
	// dividing by zero and comparing two infinities; what comes out is one code point per row, with
	// an empty row in front of the first. Negative and zero are the same case — the arithmetic that
	// separates them is a comparison of two values that are equal either way — and both are recorded
	// so that the port can say so rather than assume it.
	['no columns', a('Item 1'), 0],
	['no columns, one character', a('a'), 0],
	['no columns, empty', [], 0],
	['no columns, a wide character', [0x4f60, 0x597d], 0],
	['no columns, a combining mark', [0x0061, 0x0301, 0x0062], 0],
	['fewer than no columns', a('Item 1'), -2],
	['fewer than no columns, a wide character', [0x4f60, 0x597d], -2],
	['one column', a('Item 1'), 1],

	// --- line endings -------------------------------------------------------------------------
	['newline', a('ab\ncd'), 4],
	['newline with wrapping either side', a('abcdef\nghijkl'), 4],
	['crlf', a('ab\r\ncd'), 4],
	['lone carriage return', a('ab\rcd'), 4],
	['trailing newline', a('ab\n'), 4],
	['blank line between', a('a\n\nb'), 4],

	// --- the shapes clack actually wraps ----------------------------------------------------------
	['step and message', [STEP, ...a('  What is your name?')], 80],
	['step and message, narrow', [STEP, ...a('  What is your name?')], 12],
	['bar and value', [BAR, ...a('  Jan')], 80],
	['bar and value, narrow', [BAR, ...a('  a rather long answer')], 10],
	['bar alone', [BAR], 4],
	['corner and error', [CORNER, ...a('  Value is too short')], 10],
	['whole opening frame', a('│\n◆  What is your name?\n│  █\n└\n'), 14],
];

const seen = new Set();
const cases = CORPUS.map(([name, cp, columns]) => {
	if (seen.has(name)) throw new Error(`duplicate case name: ${name}`);
	seen.add(name);
	const input = String.fromCodePoint(...cp);
	if (input.includes('\u001b') || input.includes('\u009b')) {
		throw new Error(`case ${name} contains an escape, which the port does not cover`);
	}
	return {
		name,
		codepoints: cp,
		columns,
		rows: wrapAnsi(input, columns, OPTIONS).split('\n'),
	};
});

const fixture = {
	source: 'fast-wrap-ansi',
	version,
	options: OPTIONS,
	node: process.versions.node,
	unicode: process.versions.unicode,
	generatedBy: 'scripts/harvest-wrap.mjs',
	cases,
};

writeFileSync(FIXTURE, `${JSON.stringify(fixture, null, '\t')}\n`);
console.log(`wrote ${cases.length} cases from fast-wrap-ansi@${version} (Unicode ${process.versions.unicode})`);
