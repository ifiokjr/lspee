#!/usr/bin/env node

import { readFileSync, cpSync, mkdirSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");

const { values } = parseArgs({
	options: {
		"print-skill": { type: "boolean", default: false },
		"print-reference": { type: "boolean", default: false },
		"print-install": { type: "boolean", default: false },
		copy: { type: "string" },
		help: { type: "boolean", short: "h", default: false },
	},
	strict: true,
});

if (values.help || (!values["print-skill"] && !values["print-reference"] && !values["print-install"] && !values.copy)) {
	console.log(`lspee-skill — agent skill helper for lspee

Usage:
  lspee-skill --print-skill       Print SKILL.md to stdout
  lspee-skill --print-reference   Print REFERENCE.md to stdout
  lspee-skill --print-install     Print installation instructions
  lspee-skill --copy <dir>        Copy skill files to target directory
  lspee-skill --help              Show this help
`);
	process.exit(0);
}

if (values["print-install"]) {
	console.log(`Install the lspee CLI:
  npm install -g @ifi/lspee
  # or: cargo install lspee_cli

Install this skill package:
  npm install -g @ifi/lspee-skill
`);
	process.exit(0);
}

if (values["print-skill"]) {
	process.stdout.write(readFileSync(join(root, "SKILL.md"), "utf-8"));
	process.exit(0);
}

if (values["print-reference"]) {
	process.stdout.write(readFileSync(join(root, "REFERENCE.md"), "utf-8"));
	process.exit(0);
}

if (values.copy) {
	const target = values.copy;
	if (!existsSync(target)) {
		mkdirSync(target, { recursive: true });
	}
	for (const file of ["SKILL.md", "REFERENCE.md"]) {
		const src = join(root, file);
		const dst = join(target, file);
		cpSync(src, dst);
		console.log(`Copied ${file} -> ${dst}`);
	}
	process.exit(0);
}
