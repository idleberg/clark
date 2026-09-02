// Where clack is, and whether it is the clack every Fixture was recorded against.
//
// The harvests all need the same four things — a checkout, at the pinned tag, sometimes with
// `@clack/core` built — and each used to ask for them itself, which meant five scripts asked and
// three did not. A Fixture recorded from the wrong commit is worse than no Fixture: it fails
// somewhere unrelated months later. So the question is asked in one place, and asked by everyone.
//
// `upstream/` is not committed (ADR-0008). `mise run upstream` creates it.

import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = join(dirname(fileURLToPath(import.meta.url)), '..');

/** The tag README.md and every Fixture name. Bumping this is what tracking clack looks like. */
export const TAG = '@clack/prompts@1.7.0';

/**
 * The clack checkout, verified. Exits rather than returning something half-true, because every
 * caller's next move is to record a Fixture from it.
 *
 * @param {{ built?: boolean }} [opts] `built` also requires `@clack/core`'s dist, which the
 *   harvests that import it need and the ones that only read source do not.
 */
export function checkout({ built = false } = {}) {
	const clack = join(root, 'upstream', 'clack');

	if (!existsSync(clack)) {
		console.error(`no clack checkout at ${clack}. See ADR-0008: upstream/ is not committed.`);
		console.error('  run: mise run upstream');
		process.exit(1);
	}

	const git = (...args) => execFileSync('git', args, { cwd: clack, encoding: 'utf8' }).trim();

	const head = git('rev-parse', 'HEAD');
	const tagged = git('rev-parse', `${TAG}^{commit}`);
	if (head !== tagged) {
		console.error(`upstream/clack is not at ${TAG}.`);
		console.error(`  HEAD is ${git('describe', '--tags')} (${head.slice(0, 8)})`);
		console.error(`  run: git -C upstream/clack checkout '${TAG}'`);
		process.exit(1);
	}

	const core = join(clack, 'packages', 'core');
	if (built && !existsSync(join(core, 'dist', 'index.mjs'))) {
		console.error('@clack/core is not built. run: mise run upstream');
		process.exit(1);
	}

	return {
		clack,
		core,
		prompts: join(clack, 'packages', 'prompts'),
		git,
		head,
		describe: git('describe', '--tags'),
		TAG,
	};
}
