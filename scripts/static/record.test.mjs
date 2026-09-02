// Drives ./cases.mjs against clack and writes down what it writes. Run by
// scripts/harvest-static.mjs, never on its own — it needs RECORDER_OUT.
//
// The weakest oracle of the three Recorders, and the one that needs the least. A static renderer
// takes an argument and writes a string: there is no sequencing to get wrong, no keypress to
// mis-deliver, and nothing to declare in advance except that the call did not throw. What is
// recorded is the whole of what clack does.
//
// The terminal's width is written down but decides nothing here — `log`, `intro`, `outro` and
// `cancel` never read it. It is recorded because the *emulator* needs it: a message longer than the
// terminal is left for the terminal to break, and where it breaks is exactly what those cases are
// for.

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { styleText } from 'node:util';
import { expect, test } from 'vitest';
import * as prompts from 'clack:prompts';
import { MockWritable } from 'clack:test-utils';
import { cases } from './cases.mjs';

const OUT = process.env.RECORDER_OUT;
if (!OUT) throw new Error('RECORDER_OUT is not set; run scripts/harvest-static.mjs');
mkdirSync(OUT, { recursive: true });

// `note`'s `format` is a function, and a Fixture holds JSON. A case names one of these instead, and
// the port maps the same name to a Line-returning formatter of its own — the two upstream's own
// tests use, plus the pair of them together.
const formatters = {
	stars: (line) => `* ${line} *`,
	red: (line) => styleText('red', line),
	'red-stars': (line) => styleText('red', `* ${styleText('cyan', line)} *`),
};

/** The renderer each `kind` names, called the way upstream's own examples call it. */
const renderers = {
	note: (message, opts, title) => prompts.note(message, title, opts),
	box: (message, opts, title) => prompts.box(message, title, opts),
	log: (message, opts) => prompts.log.message(message, opts),
	'log.info': (message, opts) => prompts.log.info(message, opts),
	'log.success': (message, opts) => prompts.log.success(message, opts),
	'log.step': (message, opts) => prompts.log.step(message, opts),
	'log.warn': (message, opts) => prompts.log.warn(message, opts),
	'log.error': (message, opts) => prompts.log.error(message, opts),
	intro: (message, opts) => prompts.intro(message, opts),
	outro: (message, opts) => prompts.outro(message, opts),
	cancel: (message, opts) => prompts.cancel(message, opts),
};

for (const [index, testCase] of cases.entries()) {
	test(testCase.name, () => {
		const { name, kind = 'log', message, title = '', options = {}, columns = 80, rows = 20 } = testCase;

		const render = renderers[kind];
		if (!render) throw new Error(`no renderer for kind "${kind}"`);

		const output = new MockWritable();
		output.columns = columns;
		output.rows = rows;

		// Both `format` (a note's, per row) and `formatBorder` (a box's, per border character) are
		// functions, so a case names one of the formatters above instead of carrying it.
		const { format, formatBorder, ...rest } = options;
		for (const named of [format, formatBorder]) {
			if (named && !formatters[named]) throw new Error(`no formatter named "${named}"`);
		}

		render(
			message,
			{
				...rest,
				...(format ? { format: formatters[format] } : {}),
				...(formatBorder ? { formatBorder: formatters[formatBorder] } : {}),
				output,
			},
			title
		);

		const bytes = output.buffer.join('');
		// A renderer that wrote nothing at all is not a recording of anything.
		expect(bytes.length).toBeGreaterThan(0);

		writeFileSync(
			join(OUT, `${String(index).padStart(3, '0')}.json`),
			JSON.stringify({ name, kind, message, title, options, columns, rows, bytes }, null, '\t')
		);
	});
}
