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
import { expect, test } from 'vitest';
import * as prompts from 'clack:prompts';
import { MockWritable } from 'clack:test-utils';
import { cases } from './cases.mjs';

const OUT = process.env.RECORDER_OUT;
if (!OUT) throw new Error('RECORDER_OUT is not set; run scripts/harvest-static.mjs');
mkdirSync(OUT, { recursive: true });

/** The renderer each `kind` names, called the way upstream's own examples call it. */
const renderers = {
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
		const { name, kind = 'log', message, options = {}, columns = 80, rows = 20 } = testCase;

		const render = renderers[kind];
		if (!render) throw new Error(`no renderer for kind "${kind}"`);

		const output = new MockWritable();
		output.columns = columns;
		output.rows = rows;

		render(message, { ...options, output });

		const bytes = output.buffer.join('');
		// A renderer that wrote nothing at all is not a recording of anything.
		expect(bytes.length).toBeGreaterThan(0);

		writeFileSync(
			join(OUT, `${String(index).padStart(3, '0')}.json`),
			JSON.stringify({ name, kind, message, options, columns, rows, bytes }, null, '\t')
		);
	});
}
