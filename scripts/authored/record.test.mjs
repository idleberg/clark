// Drives ./cases.mjs against clack and writes down what it does. Run by
// scripts/harvest-authored.mjs, never on its own — it needs RECORDER_OUT.
//
// This is a second Recorder and it is weaker than the first one by construction. The harvested
// Scenarios come with upstream's own snapshots, so a suite that still passes under instrumentation
// is evidence the recording is of clack behaving normally (ADR-0003, ADR-0010). Nothing here has
// that. What it has instead: clack is the same checkout at the same tag, the prompt is called the
// way upstream's own tests call it, and every case declares what `text()` must return — a case that
// resolves to anything else fails and nothing is written.
//
// # The width
//
// `text()` does not wrap its own message. The only wrap is in `Prompt.render()`, and it is against
// `process.stdout.columns` — the real process, not the stream the prompt was handed. The mock
// output's `columns` decides nothing; its `rows` decides the diff offset. So a case's width has to
// be set on `process.stdout`, and both numbers are written down, because a Fixture that recorded
// only the one clack ignores would be a recording of the harvesting terminal's width instead.

import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, expect, test } from 'vitest';
import * as prompts from 'clack:prompts';
// Upstream's own test doubles, aliased rather than copied so they cannot drift apart.
import { MockReadable, MockWritable } from 'clack:test-utils';
import { cases } from './cases.mjs';

const OUT = process.env.RECORDER_OUT;
if (!OUT) throw new Error('RECORDER_OUT is not set; run scripts/harvest-authored.mjs');
mkdirSync(OUT, { recursive: true });

const stdoutColumns = Object.getOwnPropertyDescriptor(process.stdout, 'columns');

afterEach(() => {
	if (stdoutColumns) Object.defineProperty(process.stdout, 'columns', stdoutColumns);
	else delete process.stdout.columns;
});

for (const scenario of cases) {
	test(scenario.name, async () => {
		const input = new MockReadable();
		const output = new MockWritable();
		output.rows = scenario.rows ?? 20;

		// The width clack actually wraps to. Assigned rather than mutated: on a non-TTY stdout
		// there is no `columns` property to mutate.
		Object.defineProperty(process.stdout, 'columns', {
			value: scenario.columns,
			configurable: true,
			writable: true,
		});

		const written = [];
		const write = output.write.bind(output);
		output.write = (chunk, ...rest) => {
			written.push(String(chunk));
			return write(chunk, ...rest);
		};

		const result = prompts.text({ ...scenario.opts, input, output });
		for (const { s, key } of scenario.keys) {
			input.emit('keypress', s, key);
		}
		const value = await result;

		// The case said where this would end up. If it did not, the keys did not mean what the case
		// thinks they mean, and the bytes below are a recording of something else.
		if (scenario.cancelled) {
			expect(prompts.isCancel(value)).toBe(true);
		} else {
			expect(value).toBe(scenario.value);
		}
		expect(written.length).toBeGreaterThan(0);

		const record = {
			name: scenario.name,
			prompts: [
				{
					kind: 'text',
					opts: scenario.opts,
					settings: { withGuide: true },
					terminal: {
						// What the prompt's own stream reports, which is what upstream's suite varies
						// and what clack reads its height from.
						columns: output.columns,
						rows: output.rows,
						// And what it wraps to.
						stdout: scenario.columns,
					},
					keys: scenario.keys.map(({ s, key }) => ({
						s: s ?? null,
						key: {
							name: key?.name ?? null,
							ctrl: key?.ctrl === true,
							meta: key?.meta === true,
							shift: key?.shift === true,
							sequence: key?.sequence ?? null,
						},
					})),
					output: written,
				},
			],
		};

		const id = createHash('sha256').update(scenario.name).digest('hex').slice(0, 16);
		writeFileSync(join(OUT, `${id}.json`), `${JSON.stringify(record)}\n`);
	});
}
