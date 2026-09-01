// The third Recorder: the static renderers. Runs scripts/static/cases.mjs against clack and writes
// clackatui-core/tests/fixtures/static.json.
//
//   node scripts/harvest-static.mjs
//
// `log`, `intro`, `outro` and `cancel` are not Prompts — no state, no keys, no second Frame — so
// they cannot be carried by a Scenario, which is a sequence of events. A case here is one call and
// one string of bytes, and the Fixture is flat.
//
// Deliberate, like the other two harvests — never CI.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const clack = join(root, 'prior-art', 'clack');
const prompts = join(clack, 'packages', 'prompts');
const core = join(clack, 'packages', 'core');
const outDir = join(root, 'clackatui-core', 'tests', 'fixtures');

/** The same tag the other two Fixtures carry. */
const TAG = '@clack/prompts@1.7.0';

// --- preconditions --------------------------------------------------------------------------

if (!existsSync(clack)) {
	console.error(`no clack checkout at ${clack}. See ADR-0008: prior-art/ is not committed.`);
	process.exit(1);
}

const git = (...args) => execFileSync('git', args, { cwd: clack, encoding: 'utf8' }).trim();

const head = git('rev-parse', 'HEAD');
const tagged = git('rev-parse', `${TAG}^{commit}`);
if (head !== tagged) {
	console.error(`prior-art/clack is not at ${TAG}.`);
	console.error(`  HEAD is ${git('describe', '--tags')} (${head.slice(0, 8)})`);
	console.error(`  run: git -C prior-art/clack checkout '${TAG}'`);
	process.exit(1);
}

// --- run ------------------------------------------------------------------------------------

const spool = mkdtempSync(join(tmpdir(), 'clackatui-static-'));

try {
	execFileSync(
		'npx',
		['vitest', 'run', '--config', join(root, 'scripts', 'static', 'vitest.config.mjs')],
		{
			cwd: prompts,
			stdio: 'inherit',
			env: {
				...process.env,
				RECORDER_OUT: spool,
				RECORDER_PROMPTS_PKG: prompts,
				RECORDER_CORE_PKG: core,
			},
		}
	);
} catch {
	console.error('\na case wrote nothing; nothing was recorded.');
	rmSync(spool, { recursive: true, force: true });
	process.exit(1);
}

const records = readdirSync(spool)
	.filter((f) => f.endsWith('.json'))
	.sort()
	.map((f) => JSON.parse(readFileSync(join(spool, f), 'utf8')));
rmSync(spool, { recursive: true, force: true });

const { cases } = await import('./static/cases.mjs');
if (records.length !== cases.length) {
	console.error(`recorded ${records.length} of ${cases.length} cases; nothing was written.`);
	process.exit(1);
}

// --- write ------------------------------------------------------------------------------------

mkdirSync(outDir, { recursive: true });

const fixture = {
	source: 'scripts/static/cases.mjs',
	tag: TAG,
	commit: head,
	describe: git('describe', '--tags'),
	node: process.versions.node,
	generatedBy: 'scripts/harvest-static.mjs',
	cases: records,
};

const out = join(outDir, 'static.json');
writeFileSync(out, `${JSON.stringify(fixture, null, '\t')}\n`);

console.log(`\n${records.length} cases -> ${out}`);
