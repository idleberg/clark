// Vitest setup file for the Scenario Recorder. Loaded by scripts/recorder/vitest.config.mjs into
// every test file of clack's own suite; see ADR-0003 and ADR-0010.
//
// Everything here observes. Nothing changes what upstream's tests do, so the suite still passes its
// own snapshots while it is being recorded — which is the only evidence we have that the recording
// is of clack behaving normally, rather than of clack with a recorder in the way.
//
// This file owns the record; ./prompts-shim.mjs is what calls into it, from inside the prompt
// functions of `@clack/prompts`, where a test's options are still visible.

import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, beforeEach, expect } from 'vitest';

const OUT = process.env.RECORDER_OUT;
if (!OUT) throw new Error('RECORDER_OUT is not set; run scripts/harvest-scenarios.mjs');
mkdirSync(OUT, { recursive: true });

/** The record being built, or null between tests. */
let current = null;

/** Hooks to take back off the test's streams when the test ends. */
let undo = [];

/** Options as they can be written down. A callback becomes the fact that there was one, because a
 *  Scenario cannot carry a JavaScript function across into Rust; the streams are dropped, being the
 *  test harness rather than the Prompt. */
function plain(opts) {
	const out = {};
	for (const [k, v] of Object.entries(opts ?? {})) {
		if (v === undefined || k === 'input' || k === 'output' || k === 'signal') continue;
		if (typeof v === 'function') out[k] = { callback: true };
		else if (typeof v === 'object' && v !== null && v.constructor === Object) out[k] = plain(v);
		else if (typeof v === 'object' && v !== null && !Array.isArray(v)) out[k] = { opaque: true };
		else out[k] = v;
	}
	return out;
}

/**
 * Start recording one prompt run.
 *
 * The keypresses and the output are taken from the streams the test passed in, not from a hook on
 * `Prompt.prototype`. Under pnpm the same `@clack/core` resolves through two paths — the symlink
 * this file sees and the real one `packages/prompts/src` sees — so a patched class is not
 * necessarily the class that runs. The streams have no such ambiguity: they are the objects the
 * test created and clack is about to be handed.
 */
export function begin(kind, opts, settings) {
	if (!current) return;

	const input = opts?.input;
	const output = opts?.output;

	const run = {
		kind,
		opts: plain(opts),
		// `settings` is a module-level object upstream, so it is read now rather than when the
		// Scenario is written out: tests change it and change it back.
		settings: { withGuide: settings.withGuide },
		terminal: { columns: output?.columns ?? null, rows: output?.rows ?? null },
		keys: [],
		output: [],
	};
	current.prompts.push(run);

	if (input) {
		const emit = input.emit.bind(input);
		input.emit = (event, ...rest) => {
			if (event === 'keypress') {
				const [s, key] = rest;
				run.keys.push({
					s: s === undefined ? null : s,
					key: {
						name: key?.name ?? null,
						ctrl: key?.ctrl === true,
						meta: key?.meta === true,
						shift: key?.shift === true,
						sequence: key?.sequence ?? null,
					},
				});
			}
			return emit(event, ...rest);
		};
		undo.push(() => {
			input.emit = emit;
		});
	}

	if (output) {
		const write = output.write.bind(output);
		output.write = (chunk, ...rest) => {
			run.output.push(String(chunk));
			return write(chunk, ...rest);
		};
		undo.push(() => {
			output.write = write;
		});
	}
}

beforeEach(() => {
	const state = expect.getState();
	current = {
		test: state.currentTestName ?? '(unnamed)',
		file: state.testPath ?? null,
		prompts: [],
	};
	undo = [];
});

afterEach(() => {
	for (const off of undo) off();
	undo = [];

	const record = current;
	current = null;
	if (!record || record.prompts.length === 0) return;

	// One file per test rather than one appended file: vitest may fan tests out across processes,
	// and concurrent appends lose records. scripts/harvest-scenarios.mjs collates.
	const id = createHash('sha256').update(`${record.file} ${record.test}`).digest('hex').slice(0, 16);
	writeFileSync(join(OUT, `${id}.json`), `${JSON.stringify(record)}\n`);
});
