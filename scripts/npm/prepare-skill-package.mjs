#!/usr/bin/env node

/**
 * Prepares the skill package for publishing.
 * Copies SKILL.md and REFERENCE.md from skills/lspee/ into npm/skill/.
 *
 * Usage: node scripts/npm/prepare-skill-package.mjs [--version 0.1.0]
 */

import { cpSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

const { values } = parseArgs({
	options: {
		version: { type: "string" },
	},
	strict: true,
});

const ROOT = join(import.meta.dirname, "..", "..");
const SKILLS_SRC = join(ROOT, "skills", "lspee");
const SKILL_PKG = join(ROOT, "npm", "skill");

// Copy skill files from source of truth.
for (const file of ["SKILL.md", "REFERENCE.md"]) {
	cpSync(join(SKILLS_SRC, file), join(SKILL_PKG, file));
	console.log(`Copied ${file} -> npm/skill/${file}`);
}

// Copy license.
cpSync(join(ROOT, "LICENSE-MIT"), join(SKILL_PKG, "LICENSE"));

// Update version if provided.
if (values.version) {
	const pkgPath = join(SKILL_PKG, "package.json");
	const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));
	pkg.version = values.version;
	writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");
	console.log(`Updated npm/skill/package.json version to ${values.version}`);
}

console.log("Skill package prepared.");
