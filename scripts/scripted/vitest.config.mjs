// Vitest config for the scripted Recorder. Driven by scripts/harvest-scripted.mjs.
//
// The same arrangement as ../static/vitest.config.mjs, plus the clock. `performance` is not in
// Vitest's default `toFake` list, and a spinner asked for a timer reads `performance.now()` — so
// without it the recording would carry whatever the machine's real clock said between two calls.

import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

const pkg = process.env.RECORDER_PROMPTS_PKG;
if (!pkg) throw new Error('RECORDER_PROMPTS_PKG is not set; run scripts/harvest-scripted.mjs');
const core = process.env.RECORDER_CORE_PKG;
if (!core) throw new Error('RECORDER_CORE_PKG is not set; run scripts/harvest-scripted.mjs');

export default {
	root: here,
	resolve: {
		alias: [
			{ find: 'clack:prompts', replacement: join(pkg, 'src', 'index.ts') },
			{ find: 'clack:test-utils', replacement: join(pkg, 'test', 'test-utils.ts') },
			{ find: '@clack/core', replacement: join(core, 'src', 'index.ts') },
		],
	},
	test: {
		include: ['record.test.mjs'],
		// picocolors emits nothing when it thinks it is not talking to a terminal, and a recording
		// with no colour in it would agree with a port that had none either.
		env: { FORCE_COLOR: '1' },
		fakeTimers: {
			toFake: [
				'setTimeout',
				'clearTimeout',
				'setInterval',
				'clearInterval',
				'setImmediate',
				'clearImmediate',
				'Date',
				'performance',
			],
		},
		pool: 'forks',
		poolOptions: { forks: { singleFork: true } },
		fileParallelism: false,
	},
};
