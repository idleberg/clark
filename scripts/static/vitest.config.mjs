// Vitest config for the static Recorder. Driven by scripts/harvest-static.mjs.
//
// The same arrangement as ../authored/vitest.config.mjs and for the same reason: clack is TypeScript
// source in a checkout that is not a workspace member here, so its entry points are reached by
// alias and its own bare imports still resolve out of the checkout's node_modules.

import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

const pkg = process.env.RECORDER_PROMPTS_PKG;
if (!pkg) throw new Error('RECORDER_PROMPTS_PKG is not set; run scripts/harvest-static.mjs');
const core = process.env.RECORDER_CORE_PKG;
if (!core) throw new Error('RECORDER_CORE_PKG is not set; run scripts/harvest-static.mjs');

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
		pool: 'forks',
		poolOptions: { forks: { singleFork: true } },
		fileParallelism: false,
	},
};
