// Drives ./cases.mjs against clack and writes down what it writes. Run by
// scripts/harvest-scripted.mjs, never on its own — it needs RECORDER_OUT.
//
// The clock is the whole difference from the other three Recorders. A spinner draws from a
// `setInterval` and prints `performance.now()` when it is asked for a timer, so the recording is
// deterministic only if both are fake. Vitest's fake timers cover both — see the `toFake` list in
// ./vitest.config.mjs — and a `tick` step is one `advanceTimersByTime(delay)`, which is exactly one
// turn of the interval.
//
// Each step's bytes are recorded separately. They concatenate to the byte stream a terminal saw, so
// nothing is lost by splitting them, and a port that goes wrong on the fourth tick says so instead
// of printing a diff of the whole run.

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { styleText } from 'node:util';
import { expect, test, vi } from 'vitest';
import * as prompts from 'clack:prompts';
import { MockWritable } from 'clack:test-utils';
import { cases } from './cases.mjs';

const OUT = process.env.RECORDER_OUT;
if (!OUT) throw new Error('RECORDER_OUT is not set; run scripts/harvest-scripted.mjs');
mkdirSync(OUT, { recursive: true });

// `styleFrame` is a function and a Fixture holds JSON, so a case names one of these — the same
// arrangement scripts/static/record.test.mjs uses for a `note`'s `format`.
const formatters = {
	red: (frame) => styleText('red', frame),
	stars: (frame) => `*${frame}*`,
};

/** The delay a spinner is given, which is also how far a `tick` advances the clock. */
const delayOf = (options) => options.delay ?? 80;

for (const [index, testCase] of cases.entries()) {
	test(testCase.name, () => {
		const { name, kind, options, columns, rows, steps, expectsNothing = false } = testCase;
		if (kind !== 'spinner' && kind !== 'progress') {
			throw new Error(`no renderer for kind "${kind}"`);
		}

		const output = new MockWritable();
		output.columns = columns;
		output.rows = rows;

		const { styleFrame, ci = false, ...rest } = options;
		if (styleFrame && !formatters[styleFrame]) {
			throw new Error(`no formatter named "${styleFrame}"`);
		}

		// `isCI()` is `process.env.CI === 'true'`, read when the spinner is constructed.
		const originalCI = process.env.CI;
		process.env.CI = ci ? 'true' : 'false';

		vi.useFakeTimers();
		const delay = delayOf(options);
		const written = [];
		let origin = 0;

		try {
			// `progress()` takes a spinner's options and three of its own, and returns the same
			// object with `advance` added — so the switch below is the same for both.
			const make = kind === 'progress' ? prompts.progress : prompts.spinner;
			const spinner = make({
				...rest,
				...(styleFrame ? { styleFrame: formatters[styleFrame] } : {}),
				output,
			});

			for (const step of steps) {
				const before = output.buffer.length;
				switch (step.op) {
					case 'start':
						origin = Date.now();
						spinner.start(step.message);
						break;
					case 'tick':
						vi.advanceTimersByTime(delay);
						break;
					case 'message':
						spinner.message(step.message);
						break;
					case 'advance':
						spinner.advance(step.step, step.message);
						break;
					case 'stop':
						spinner.stop(step.message);
						break;
					case 'cancel':
						spinner.cancel(step.message);
						break;
					case 'error':
						spinner.error(step.message);
						break;
					case 'clear':
						spinner.clear();
						break;
					default:
						throw new Error(`no such step: ${step.op}`);
				}
				written.push({
					...step,
					// The time the spinner would have read off its own clock at this step. Only the
					// timer indicator uses it, and only the port needs it written down.
					elapsed: Date.now() - origin,
					bytes: output.buffer.slice(before).join(''),
				});
			}
		} finally {
			vi.useRealTimers();
			if (originalCI === undefined) delete process.env.CI;
			else process.env.CI = originalCI;
		}

		const bytes = output.buffer.join('');
		// A script that wrote nothing is what a case that silently failed to run looks like, so the
		// one case that means it says so.
		if (expectsNothing) expect(bytes).toBe('');
		else expect(bytes.length).toBeGreaterThan(0);

		writeFileSync(
			join(OUT, `${String(index).padStart(3, '0')}.json`),
			JSON.stringify(
				{ name, kind, options, columns, rows, delay, steps: written, bytes },
				null,
				'\t'
			)
		);
	});
}
