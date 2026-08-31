// Vitest config for the hand-authored Recorder. Driven by scripts/harvest-authored.mjs.
//
// Unlike scripts/recorder/vitest.config.mjs this does not run upstream's suite, so `root` is this
// directory rather than the clack package, and the clack entry points are reached by alias. Vite
// resolves a module's own bare imports relative to the file that makes them, so `@clack/core` and
// its dependencies still come out of the checkout's `node_modules` and nothing is written into it.

import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

const pkg = process.env.RECORDER_PROMPTS_PKG;
if (!pkg) throw new Error('RECORDER_PROMPTS_PKG is not set; run scripts/harvest-authored.mjs');
const core = process.env.RECORDER_CORE_PKG;
if (!core) throw new Error('RECORDER_CORE_PKG is not set; run scripts/harvest-authored.mjs');

export default {
	root: here,
	resolve: {
		alias: [
			{ find: 'clack:prompts', replacement: join(pkg, 'src', 'index.ts') },
			{ find: 'clack:test-utils', replacement: join(pkg, 'test', 'test-utils.ts') },
			// `packages/prompts/src` imports `@clack/core` by name; outside the workspace there is
			// no node_modules link to follow, so it is pointed at the source directly.
			{ find: '@clack/core', replacement: join(core, 'src', 'index.ts') },
		],
	},
	test: {
		include: ['record.test.mjs'],
		// picocolors is what clack styles with, and it emits nothing when it thinks it is not
		// talking to a terminal. Without this every recorded Frame would be plain text.
		env: { FORCE_COLOR: '1' },
		// The cases take turns with `process.stdout.columns`, so they cannot be run in parallel and
		// cannot be spread across processes.
		pool: 'forks',
		poolOptions: { forks: { singleFork: true } },
		fileParallelism: false,
	},
};
