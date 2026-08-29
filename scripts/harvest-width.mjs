// Harvests clackatui-core/tests/fixtures/width.json from the real fast-string-width.
//
// The Rust port in clackatui-core/src/width.rs is checked against the fixture, never against a live
// Node process: prior-art/ is not committed, so CI has no JavaScript to compare with. This script is
// how the fixture is refreshed when the pinned clack version moves -- see docs/adr/0008.
//
// Run it from the repository root:
//
//     node scripts/harvest-width.mjs
//
// Every string is written as an array of code points, never as a literal. Decomposed sequences
// written literally into a source file get silently precomposed by the editor or the filesystem,
// which quietly changes what is being measured.

import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { readFileSync, writeFileSync } from 'node:fs';

const HERE = new URL('.', import.meta.url);
const ANCHOR = new URL('../prior-art/clack/packages/prompts/package.json', HERE);
const FIXTURE = new URL('../clackatui-core/tests/fixtures/width.json', HERE);

const require = createRequire(ANCHOR);
const { default: fastStringWidth } = await import(pathToFileURL(require.resolve('fast-string-width')));
const pkgPath = require.resolve('fast-string-width').replace(/dist[/\\]index\.js$/, 'package.json');
const { version } = JSON.parse(readFileSync(pkgPath, 'utf8'));

const ESC = 0x001b;

/** ASCII is safe to write literally: nothing normalises it. */
const a = (s) => [...s].map((c) => c.codePointAt(0));

const CORPUS = [
	// --- the trivial cases -------------------------------------------------------------------
	['empty', []],
	['ascii', a('hello')],
	['ascii punctuation', a('~!@#$%^&*()_+')],
	['space', a(' ')],
	['latin-1 supplement', [0x00e9, 0x00f1, 0x00fc]],
	['nbsp', [0x00a0]],
	['soft hyphen', [0x00ad]],
	// U+009F is C1, not latin-1 printable: the latin block starts at U+00A0.
	['c1 boundary', [0x009f, 0x00a0]],

	// --- tabs and controls -------------------------------------------------------------------
	['tab', [0x0009]],
	['three tabs', [0x0009, 0x0009, 0x0009]],
	['tab between letters', a('a\tb')],
	['bel', [0x0007]],
	['newline', [0x000a]],
	['control run', [0x0001, 0x0002, 0x0003]],
	['lone escape', [0x001b]],
	['del', [0x007f]],

	// --- ANSI --------------------------------------------------------------------------------
	['sgr red', [...a('\u001b[31m'), ...a('red'), ...a('\u001b[0m')]],
	['sgr 256', [...a('\u001b[38;5;196m'), ...a('x')]],
	['sgr truecolor', [...a('\u001b[38;2;255;0;0m'), ...a('x')]],
	['sgr reset only', a('\u001b[m')],
	['cursor up', a('\u001b[2A')],
	['csi ending in a digit', a('\u001b[12345')],
	['csi private mode', a('\u001b[?25l')],
	['c1 csi introducer', [0x009b, ...a('31m'), ...a('x')]],
	['osc8 bel terminated', [...a('\u001b]8;;https://example.com'), 0x0007, ...a('link')]],
	['osc8 st terminated', [...a('\u001b]8;;https://example.com'), 0x001b, 0x005c, ...a('link')]],
	['osc8 unterminated', a('\u001b]8;;https://example.com')],
	['osc8 with params', [...a('\u001b]8;id=1;https://example.com'), 0x0007]],

	// --- CJKT --------------------------------------------------------------------------------
	['han', [0x4f60, 0x597d]],
	['hiragana', [0x3042, 0x3044]],
	['katakana', [0x30ab, 0x30bf]],
	['hangul syllable', [0xac01]],
	['conjoining jamo', [0x1100, 0x1161, 0x11a8]],
	['halfwidth katakana', [0xff76]],
	['halfwidth katakana with sound mark', [0x30ab, 0xff9e]],
	['combining sound mark', [0x3099]],
	['fullwidth latin', [0xff21, 0xff22]],
	['ideographic space', [0x3000]],
	['fullwidth yen', [0xffe5]],
	['cjk ext b', [0x20000]],
	['tangut', [0x17000]],
	['mixed han and ascii', [...a('a'), 0x4f60, ...a('b')]],

	// --- the wide-but-not-CJKT ranges ---------------------------------------------------------
	['hourglass', [0x231b]],
	['angle bracket', [0x2329]],
	['ideographic comma', [0x3001]],
	['bopomofo', [0x3105]],
	['compatibility form', [0xfe10]],
	['squared katakana', [0x1f200]],
	['circled ideograph', [0x1f240]],

	// --- emoji -------------------------------------------------------------------------------
	['emoji presentation', [0x1f600]],
	['heart with vs16', [0x2764, 0xfe0f]],
	['heart without vs16', [0x2764]],
	['heart with vs15', [0x2764, 0xfe0e]],
	['keycap', [0x0031, 0xfe0f, 0x20e3]],
	['keycap hash', [0x0023, 0xfe0f, 0x20e3]],
	['bare digit', [0x0031]],
	['skin tone', [0x1f44d, 0x1f3fd]],
	['modifier base without modifier', [0x1f44d]],
	['lone modifier', [0x1f3fd]],
	['flag', [0x1f1e9, 0x1f1ea]],
	['three regional indicators', [0x1f1e9, 0x1f1ea, 0x1f1eb]],
	['lone regional indicator', [0x1f1e9]],
	['scotland tag sequence', [0x1f3f4, 0xe0067, 0xe0062, 0xe0073, 0xe0063, 0xe0074, 0xe007f]],
	['black flag alone', [0x1f3f4]],
	['zwj profession', [0x1f469, 0x200d, 0x1f4bb]],
	['zwj family', [0x1f468, 0x200d, 0x1f469, 0x200d, 0x1f467, 0x200d, 0x1f466]],
	['zwj with skin tones', [0x1f469, 0x1f3fb, 0x200d, 0x1f91d, 0x200d, 0x1f468, 0x1f3ff]],
	['trailing zwj', [0x1f469, 0x200d]],
	['zwj into ascii', [0x1f469, 0x200d, ...a('a')]],
	['lone zwj', [0x200d]],
	['two emoji', [0x1f600, 0x1f601]],
	['emoji then ascii', [0x1f600, ...a('ok')]],

	// --- marks -------------------------------------------------------------------------------
	['e with combining acute', [0x0065, 0x0301]],
	['stacked marks', [0x0041, 0x0300, 0x0301, 0x0302]],
	['lone combining acute', [0x0301]],
	['thai with sara am', [0x0e01, 0x0e33]],
	['devanagari', [0x0915, 0x094d, 0x0937]],
	['hangul with mark', [0xac01, 0x0301]],

	// --- shapes clack actually renders ---------------------------------------------------------
	['prompt line', a('\u001b[36m?\u001b[0m What is your name?')],
	['bar with cjk value', [...a('\u001b[90m|\u001b[0m  '), 0x4f60, 0x597d]],
	['bar with emoji value', [...a('|  '), 0x1f680, ...a(' deploy')]],
	['mixed everything', [...a('a\t'), 0x4f60, 0x1f469, 0x200d, 0x1f4bb, 0x0065, 0x0301, ...a('\u001b[0m'), ...a('z')]],
];

const seen = new Set();
const cases = CORPUS.map(([name, cp]) => {
	if (seen.has(name)) throw new Error(`duplicate case name: ${name}`);
	seen.add(name);
	return { name, codepoints: cp, width: fastStringWidth(String.fromCodePoint(...cp)) };
});

const fixture = {
	source: 'fast-string-width',
	version,
	node: process.versions.node,
	unicode: process.versions.unicode,
	generatedBy: 'scripts/harvest-width.mjs',
	cases,
};

writeFileSync(FIXTURE, `${JSON.stringify(fixture, null, '\t')}\n`);
console.log(`wrote ${cases.length} cases from fast-string-width@${version} (Unicode ${process.versions.unicode})`);
