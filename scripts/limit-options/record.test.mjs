// Runs ./cases.mjs through the real `limitOptions` and writes down what came back. Run by
// scripts/harvest-limit-options.mjs, never on its own — it needs RECORDER_OUT.
//
// This is a corpus rather than a recording, in the sense ADR-0008 draws: `limitOptions` is a pure
// function, so there is no Prompt to instrument and no sequence to preserve. It is called the way
// `select` calls it and its return value is written down.
//
// # What the output stream is for
//
// `limitOptions` reads the terminal off the stream it is handed, through `getColumns`/`getRows`,
// and those only look for a `columns` and a `rows` property. So an object literal with the two
// numbers on it is the whole of the double — no MockWritable, because nothing here writes.
//
// # Escapes
//
// A Frame carries no escapes (ADR-0011), so the fixture cannot either. Every style in ./cases.mjs
// is deliberately plain, which leaves exactly one styled thing in the output: the `...` overflow
// row, which `limitOptions` produces itself as `styleText('dim', '...')`. Each line is written down
// as its code points plus an `overflow` flag, and a line that carries an escape and is *not* the
// overflow row fails the case rather than being stripped — that would be the corpus quietly
// dropping something the port would then never be asked about.

import { styleText } from 'node:util';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from 'vitest';
import { limitOptions } from 'clack:limit-options';
import { cases } from './cases.mjs';

const OUT = process.env.RECORDER_OUT;
if (!OUT) throw new Error('RECORDER_OUT is not set; run scripts/harvest-limit-options.mjs');
mkdirSync(OUT, { recursive: true });

const OVERFLOW = styleText('dim', '...');

/** The style callbacks ./cases.mjs names. None of them styles, so the fixture stays escape-free. */
const STYLES = {
	plain: (option) => option,
	marker: (option, active) => (active ? `> ${option}` : `  ${option}`),
	wide: (option) => `-- ${option} --`,
};

const ESCAPE = /[\u001b\u009b]/;

test('the harvest is running with colour on', () => {
	// Without it the overflow row is three plain dots and indistinguishable from an option, and
	// every window case would record something subtly wrong rather than failing.
	expect(OVERFLOW).not.toBe('...');
});

for (const scenario of cases) {
	test(scenario.name, () => {
		const style = STYLES[scenario.style];
		expect(style, `unknown style: ${scenario.style}`).toBeTypeOf('function');

		const options = scenario.options.map((codepoints) => String.fromCodePoint(...codepoints));

		const lines = limitOptions({
			output: { columns: scenario.columns, rows: scenario.rows },
			options,
			cursor: scenario.cursor,
			style,
			// `undefined` rather than the value, so a null in the case takes upstream's own default
			// instead of a number this file chose to stand in for it.
			maxItems: scenario.maxItems ?? undefined,
			columnPadding: scenario.columnPadding ?? undefined,
			rowPadding: scenario.rowPadding ?? undefined,
		});

		const recorded = lines.map((line) => {
			const overflow = line === OVERFLOW;
			const text = overflow ? '...' : line;
			expect(ESCAPE.test(text), `styled line the corpus cannot carry: ${JSON.stringify(line)}`).toBe(
				false
			);
			return { codepoints: [...text].map((ch) => ch.codePointAt(0)), overflow };
		});

		writeFileSync(
			join(OUT, `${createId(scenario.name)}.json`),
			JSON.stringify({ ...scenario, lines: recorded })
		);
	});
}

/** A filename that cannot collide and cannot contain a separator. */
function createId(name) {
	return name.replace(/[^a-z0-9]+/gi, '-').toLowerCase();
}
