#!/usr/bin/env node

/**
 * Publishes all npm packages (platform binaries + root + skill).
 *
 * Usage: node scripts/npm/publish-packages.mjs --dist ./dist [--dry-run]
 */

import { execSync } from "node:child_process";
import { readdirSync, existsSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

const { values } = parseArgs({
	options: {
		dist: { type: "string", default: "./dist" },
		"dry-run": { type: "boolean", default: false },
	},
	strict: true,
});

const DIST = values.dist;
const dryRun = values["dry-run"];

function publish(dir) {
	const pkgPath = join(dir, "package.json");
	if (!existsSync(pkgPath)) {
		console.warn(`Skipping ${dir}: no package.json`);
		return;
	}

	const args = ["publish", "--access", "public"];
	if (dryRun) args.push("--dry-run");

	try {
		execSync(`npm ${args.join(" ")}`, { cwd: dir, stdio: "inherit" });
	} catch (error) {
		if (error.message?.includes("EPUBLISHCONFLICT") || error.message?.includes("already exists")) {
			console.log(`Already published, skipping: ${dir}`);
		} else {
			throw error;
		}
	}
}

// Publish platform packages first.
const dirs = readdirSync(DIST, { withFileTypes: true })
	.filter((d) => d.isDirectory())
	.map((d) => join(DIST, d.name));

for (const dir of dirs) {
	publish(dir);
}

// Publish skill package.
const skillDir = join(import.meta.dirname, "..", "..", "npm", "skill");
if (existsSync(join(skillDir, "package.json"))) {
	publish(skillDir);
}

console.log(dryRun ? "Dry run complete." : "All packages published.");
