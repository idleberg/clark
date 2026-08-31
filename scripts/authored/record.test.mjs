// Drives ./cases.mjs against clack and writes down what it does. Run by
// scripts/harvest-authored.mjs, never on its own — it needs RECORDER_OUT.
//
// This is a second Recorder and it is weaker than the first one by construction. The harvested
// Scenarios come with upstream's own snapshots, so a suite that still passes under instrumentation
// is evidence the recording is of clack behaving normally (ADR-0003, ADR-0010). Nothing here has
// that. What it has instead: clack is the same checkout at the same tag, the prompt is called the
// way upstream's own tests call it, and every case declares what it must return — a case that
// resolves to anything else fails and nothing is written.
//
// # The two widths
//
// `text()` and `password()` do not wrap their own messages. Their only wrap is in `Prompt.render()`,
// and it is against `process.stdout.columns` — the real process, not the stream the prompt was
// handed. The mock output's `columns` decides nothing there; its `rows` decides the diff offset. So
// a case's width has to be set on `process.stdout`, and both numbers are written down, because a
// Fixture that recorded only the one clack ignores would be a recording of the harvesting
// terminal's width instead.
//
// `confirm()` is the exception: `wrapTextWithPrefix` reads `getColumns(opts.output)`, so the
// stream's number decides where its message breaks and the global decides where the Frame around it
// breaks. In a real program they are the same terminal, and they are set to the same number here.

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

/** A keypress in the shape both Fixtures record it. */
const recorded = ({ s, key }) => ({
	s: s ?? null,
	key: {
		name: key?.name ?? null,
		ctrl: key?.ctrl === true,
		meta: key?.meta === true,
		shift: key?.shift === true,
		sequence: key?.sequence ?? null,
	},
});

const stdoutColumns = Object.getOwnPropertyDescriptor(process.stdout, 'columns');

afterEach(() => {
	if (stdoutColumns) Object.defineProperty(process.stdout, 'columns', stdoutColumns);
	else delete process.stdout.columns;
});

for (const scenario of cases) {
	test(scenario.name, async () => {
		const input = new MockReadable();
		const output = new MockWritable();
		// In a real program the stream and the process are the same terminal, so they are the same
		// number here too — even though `text` reads only the height off the stream.
		output.columns = scenario.columns;
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

		const kind = scenario.kind ?? 'text';
		const result = prompts[kind]({ ...scenario.opts, input, output });

		// The events, in order, and where in the byte stream each one landed. A key needs no
		// position — every chunk after it and before the next event is its doing — but a resize does:
		// the terminal the bytes are being written into changes at that point, so anything replaying
		// the stream has to change with it. `at` is how many chunks had been written when the resize
		// was delivered.
		const events = [];
		for (const event of scenario.events) {
			if (event.kind === 'resize') {
				const rows = event.rows ?? output.rows;
				Object.defineProperty(process.stdout, 'columns', {
					value: event.columns,
					configurable: true,
					writable: true,
				});
				output.columns = event.columns;
				output.rows = rows;

				events.push({ kind: 'resize', columns: event.columns, rows, at: written.length });
				// `Prompt.prompt` subscribes `this.render` to this, which is the whole mechanism.
				output.emit('resize');
			} else {
				events.push({ kind: 'key', ...recorded(event) });
				input.emit('keypress', event.s, event.key);
			}
		}

		const value = await result;

		// The case said where this would end up. If it did not, the events did not mean what the case
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
					kind,
					opts: scenario.opts,
					settings: { withGuide: true },
					// The terminal the Prompt opened in. A Scenario that resizes says the rest in its
					// events; this is where it started.
					terminal: {
						// What the prompt's own stream reports, which is what upstream's suite varies
						// and what clack reads its height from.
						columns: scenario.columns,
						rows: scenario.rows ?? 20,
						// And what it wraps to.
						stdout: scenario.columns,
					},
					events,
					// The key events on their own, in the shape the harvested Fixture uses, for
					// everything that drives a bare Prompt and has no notion of a terminal at all.
					keys: scenario.events.filter((e) => e.kind === 'key').map(recorded),
					output: written,
				},
			],
		};

		const id = createHash('sha256').update(scenario.name).digest('hex').slice(0, 16);
		writeFileSync(join(OUT, `${id}.json`), `${JSON.stringify(record)}\n`);
	});
}
