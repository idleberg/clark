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

/** `start`, ticks, `stop` — a `spinner` or the `progress` bar that is one. */
const runSpinner = ({ kind, options, output, steps, delay, written }) => {
	const { styleFrame, ...rest } = options;
	if (styleFrame && !formatters[styleFrame]) {
		throw new Error(`no formatter named "${styleFrame}"`);
	}
	// `progress()` takes a spinner's options and three of its own, and returns the same object with
	// `advance` added — so the switch below is the same for both.
	const make = kind === 'progress' ? prompts.progress : prompts.spinner;
	const spinner = make({
		...rest,
		...(styleFrame ? { styleFrame: formatters[styleFrame] } : {}),
		output,
	});

	// `null` until a `start` step, so that a spinner which was never started records no elapsed
	// time rather than the epoch. A wall clock in a Fixture is drift on every re-record.
	let origin = null;
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
			// The time the spinner would have read off its own clock at this step. Only the timer
			// indicator uses it, and only the port needs it written down.
			elapsed: origin === null ? 0 : Date.now() - origin,
			bytes: output.buffer.slice(before).join(''),
		});
	}
};

/** `taskLog`, which has no clock: an `open`, some messages, and an ending. */
const runTaskLog = ({ options, output, steps, written }) => {
	let log;
	// The groups a script made, in the order it made them — a step names one by that index.
	const groups = [];
	const groupOf = (step) => {
		const group = groups[step.group ?? 0];
		if (!group) throw new Error(`no group ${step.group}`);
		return group;
	};

	for (const step of steps) {
		const before = output.buffer.length;
		switch (step.op) {
			// `taskLog()` writes its title from the constructor, so making one is a step.
			case 'open':
				log = prompts.taskLog({ ...options, output });
				break;
			case 'message':
				log.message(step.message, { raw: step.raw });
				break;
			case 'group':
				groups.push(log.group(step.name));
				break;
			case 'group-message':
				groupOf(step).message(step.message, { raw: step.raw });
				break;
			case 'group-success':
				groupOf(step).success(step.message);
				break;
			case 'group-error':
				groupOf(step).error(step.message);
				break;
			case 'success':
				log.success(step.message, { showLog: step.showLog });
				break;
			case 'error':
				log.error(step.message, { showLog: step.showLog });
				break;
			default:
				throw new Error(`no such step: ${step.op}`);
		}
		written.push({ ...step, elapsed: 0, bytes: output.buffer.slice(before).join('') });
	}
};

for (const [index, testCase] of cases.entries()) {
	test(testCase.name, () => {
		const { name, kind, options, columns, rows, steps, expectsNothing = false } = testCase;
		if (kind !== 'spinner' && kind !== 'progress' && kind !== 'task-log') {
			throw new Error(`no renderer for kind "${kind}"`);
		}

		const output = new MockWritable();
		output.columns = columns;
		output.rows = rows;

		// `guide` is the *global* `settings.withGuide`, not the per-call option. A task log never
		// passes its own `withGuide` to the `log.message` calls it makes, so the global is the only
		// thing that turns its bars off — see the port's module docs.
		const { ci = false, tty = !ci, guide = true, ...rest } = options;
		// `isTTY(output)` is a property, and half of what a task log calls `isTTY` — the other half
		// is `isCI()`. A spinner reads neither.
		output.isTTY = tty;

		// `isCI()` is `process.env.CI === 'true'`, read when the renderer is constructed.
		const originalCI = process.env.CI;
		process.env.CI = ci ? 'true' : 'false';

		vi.useFakeTimers();
		const delay = delayOf(options);
		const written = [];

		prompts.updateSettings({ withGuide: guide });

		try {
			const run = kind === 'task-log' ? runTaskLog : runSpinner;
			run({ kind, options: rest, output, steps, delay, written });
		} finally {
			prompts.updateSettings({ withGuide: true });
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
