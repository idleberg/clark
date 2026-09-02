// Harvests clackatui-core/tests/fixtures/emitter.json from `@clack/core`'s own Prompt.render.
//
// The Emitter reproduces clack's cursor arithmetic and erasure (docs/adr/0013), and this is its
// oracle. Rather than reimplementing `render` in JavaScript to compare against, the script drives
// the real `Prompt` class: a Prompt whose `render` option returns the next frame from a list, and
// whose `output` collects every write. Stepping it through a list of frames therefore records
// exactly what clack would have put on the wire, with no prompt logic in the way — the same
// separation the width and wrap harvests have (docs/adr/0008).
//
// Run it from the repository root:
//
//     node scripts/harvest-emitter.mjs
//
// `Prompt.render` is `private` in TypeScript, which is a compile-time claim only; the constructor
// binds it as an own property, so it is callable from JavaScript. Nothing else is reached into.
//
// Two upstream quirks the corpus depends on, both recorded rather than smoothed over:
//
//   - The wrap width comes from `process.stdout.columns` while the terminal height comes from
//     `getRows(this.output)`. One is global and one is the Prompt's own output, so the script sets
//     both.
//   - A first frame equal to the empty string is not written at all, and the Prompt stays in its
//     `initial` state — so the *next* frame is the one that hides the cursor.
//
// The corpus carries no ANSI escapes and no colour. Upstream's frames are picocolors output, and
// matching those bytes would be asserting picocolors' encoding rather than clack's algorithm; a
// Frame carries styling as a `Style` per span (docs/adr/0011) and the Emitter states it per cell,
// which the opening-Frame comparison in tests/scenario_replay.rs already covers. What is asserted
// here is where the cursor goes and what gets erased.
//
// Every frame is written as an array of code points rather than a literal, for the reason
// harvest-width.mjs gives: a decomposed sequence written literally gets silently precomposed
// somewhere between the editor and the filesystem, and then the fixture is not what it claims.

import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { checkout } from './upstream.mjs';

const HERE = new URL('.', import.meta.url);
// Verified rather than merely resolved: this one imports `@clack/core`'s build, so a checkout at
// the wrong tag would record a different library's cursor arithmetic without saying so.
const CORE = pathToFileURL(`${checkout({ built: true }).core}/`);
const FIXTURE = new URL('../clackatui-core/tests/fixtures/emitter.json', HERE);

const { Prompt } = await import(pathToFileURL(new URL('dist/index.mjs', CORE).pathname));
const { version } = JSON.parse(readFileSync(new URL('package.json', CORE), 'utf8'));

/** Code points, so nothing in the corpus can be recomposed on its way to disk. */
const f = (s) => [...s].map((c) => c.codePointAt(0));

const BAR = '│'; // │
const CORNER = '└'; // └
const STEP = '◆'; // ◆
const CANCEL = '■'; // ■

/** A `text` Prompt's opening frame, as clack draws it, minus the colour. */
const OPENING = `${BAR}\n${STEP}  What is your name?\n${BAR}  \n${CORNER}\n`;
const TYPED = `${BAR}\n${STEP}  What is your name?\n${BAR}  Jan\n${CORNER}\n`;
const ERRORED = `${BAR}\n${STEP}  What is your name?\n${BAR}  \n${CORNER}  Required\n`;
const CANCELLED = `${BAR}\n${CANCEL}  What is your name?\n${BAR}  \n`;

// [name, columns, rows, frames]
const CORPUS = [
	// --- the first frame -------------------------------------------------------------------------
	['one frame', 20, 10, ['a\nb\nc']],
	['one line', 20, 10, ['a']],
	['no lines at all', 20, 10, ['']],
	['an empty first frame is not a frame', 20, 10, ['', 'a\nb']],
	['a blank line is a line', 20, 10, ['\n\n']],

	// --- nothing to do ---------------------------------------------------------------------------
	['the same frame twice', 20, 10, ['a\nb', 'a\nb']],
	['the same frame three times', 20, 10, ['a\nb', 'a\nb', 'a\nb']],

	// --- exactly one line differs ----------------------------------------------------------------
	['the middle line changes', 20, 10, ['a\nb\nc', 'a\nB\nc']],
	['the first line changes', 20, 10, ['a\nb\nc', 'A\nb\nc']],
	['the last line changes', 20, 10, ['a\nb\nc', 'a\nb\nC']],
	['the only line changes', 20, 10, ['a', 'b']],
	['a line gets longer', 20, 10, ['a\nbb\nc', 'a\nbbbb\nc']],
	['a line gets shorter', 20, 10, ['a\nbbbb\nc', 'a\nb\nc']],
	['a line empties', 20, 10, ['a\nb\nc', 'a\n\nc']],
	['a change and then another', 20, 10, ['a\nb\nc', 'a\nB\nc', 'a\nB\nC']],

	// --- more than one line differs ----------------------------------------------------------------
	['two lines change', 20, 10, ['a\nb\nc', 'A\nb\nC']],
	['every line changes', 20, 10, ['a\nb\nc', 'A\nB\nC']],
	['two lines change back', 20, 10, ['a\nb\nc', 'A\nB\nc', 'a\nb\nc']],

	// --- the frame changes height ------------------------------------------------------------------
	['the frame grows by a line', 20, 10, ['a\nb', 'a\nb\nc']],
	['the frame shrinks by a line', 20, 10, ['a\nb\nc', 'a\nb']],
	['the frame grows at the top', 20, 10, ['b\nc', 'a\nb\nc']],
	['the frame shrinks to nothing', 20, 10, ['a\nb\nc', '']],
	['the frame grows from nothing', 20, 10, ['a', '', 'a\nb\nc']],

	// --- the wrap decides the rows, not the newlines -----------------------------------------------
	['a line wraps', 4, 10, ['abcdefgh\nx', 'abcdefgh\ny']],
	['a change makes a line wrap', 4, 10, ['ab\nx', 'abcdefgh\nx']],
	['a change makes a line stop wrapping', 4, 10, ['abcdefgh\nx', 'ab\nx']],
	['a word moves to the next row', 4, 10, ['ab cd\nx', 'ab ce\nx']],
	['wrapping puts the change on a later row', 4, 10, ['abcdefgh\nx', 'abcdefgi\nx']],

	// --- frames taller than the terminal -------------------------------------------------------------
	['taller than the terminal', 20, 3, ['1\n2\n3\n4\n5', '1\n2\n3\n4\nX']],
	['the change is above the terminal', 20, 2, ['1\n2\n3\n4\n5', 'X\n2\n3\n4\n5']],
	['one change above and one inside', 20, 3, ['1\n2\n3\n4\n5', 'X\n2\n3\n4\nY']],
	['a tall frame shrinks below the terminal', 20, 3, ['1\n2\n3\n4\n5', '1\nX']],
	['a short frame grows past the terminal', 20, 3, ['1\n2', '1\n2\n3\n4\n5']],
	['a terminal one row tall', 20, 1, ['1\n2\n3', '1\n2\nX']],

	// --- the shapes clack actually emits ---------------------------------------------------------
	['a text prompt opens', 80, 24, [OPENING]],
	['a text prompt is typed into', 80, 24, [OPENING, TYPED]],
	['a text prompt errors', 80, 24, [OPENING, ERRORED]],
	['a text prompt is cancelled', 80, 24, [OPENING, CANCELLED]],
	['a text prompt in a narrow terminal', 14, 24, [OPENING, TYPED]],
	['a text prompt in a very narrow terminal', 8, 24, [OPENING, TYPED, ERRORED]],
];

const names = new Set();
const cases = [];

for (const [name, columns, rows, frames] of CORPUS) {
	if (names.has(name)) throw new Error(`duplicate case name: ${name}`);
	names.add(name);

	for (const frame of frames) {
		if (frame.includes('\u001b') || frame.includes('\u009b')) {
			throw new Error(`case "${name}" has an escape in it; the corpus is escape-free`);
		}
	}

	// `render` reads the wrap width off the global and the terminal height off its own output.
	const previousColumns = process.stdout.columns;
	process.stdout.columns = columns;

	let written = '';
	const output = {
		rows,
		columns,
		write(chunk) {
			written += chunk;
			return true;
		},
	};

	let index = 0;
	const prompt = new Prompt({
		render: () => frames[index],
		input: process.stdin,
		output,
	});

	const writes = [];
	for (index = 0; index < frames.length; index++) {
		written = '';
		prompt.render();
		writes.push(written);
	}

	process.stdout.columns = previousColumns;

	cases.push({
		name,
		columns,
		rows,
		frames: frames.map(f),
		writes,
	});
}

writeFileSync(
	FIXTURE,
	`${JSON.stringify(
		{
			source: '@clack/core Prompt.render',
			version,
			node: process.version,
			generatedBy: 'scripts/harvest-emitter.mjs',
			cases,
		},
		null,
		'\t'
	)}\n`
);

console.log(`wrote ${cases.length} cases to ${FIXTURE.pathname}`);
