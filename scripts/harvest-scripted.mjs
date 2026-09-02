// The fourth Recorder: the renderers that are driven by calls. Runs scripts/scripted/cases.mjs
// against clack and writes clackatui-core/tests/fixtures/scripted.json.
//
//   node scripts/harvest-scripted.mjs
//
// A `spinner` is neither a Prompt nor a static renderer. It reads no key, so a Scenario cannot carry
// it; it draws more than once, so one call and one string cannot either. A case here is a *script* —
// `start`, some ticks of a fake clock, a `message`, a `stop` — and what is recorded is the bytes
// each of those steps wrote.
//
// Deliberate, like the other three harvests — never CI.

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

const spool = mkdtempSync(join(tmpdir(), 'clackatui-scripted-'));

try {
	execFileSync(
		'npx',
		['vitest', 'run', '--config', join(root, 'scripts', 'scripted', 'vitest.config.mjs')],
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

const { cases } = await import('./scripted/cases.mjs');
if (records.length !== cases.length) {
	console.error(`recorded ${records.length} of ${cases.length} cases; nothing was written.`);
	process.exit(1);
}

// --- write ------------------------------------------------------------------------------------

mkdirSync(outDir, { recursive: true });

const fixture = {
	source: 'scripts/scripted/cases.mjs',
	tag: TAG,
	commit: head,
	describe: git('describe', '--tags'),
	node: process.versions.node,
	generatedBy: 'scripts/harvest-scripted.mjs',
	cases: records,
};

const out = join(outDir, 'scripted.json');
writeFileSync(out, `${JSON.stringify(fixture, null, '\t')}\n`);

console.log(`\n${records.length} cases -> ${out}`);
