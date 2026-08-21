#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { join } from "node:path";

const require = createRequire(import.meta.url);

const PLATFORMS = {
	"darwin-arm64": {
		package: "@ifi/monokit-darwin-arm64",
		binary: "monokit",
	},
	"darwin-x64": {
		package: "@ifi/monokit-darwin-x64",
		binary: "monokit",
	},
	"linux-arm64-glibc": {
		package: "@ifi/monokit-linux-arm64-gnu",
		binary: "monokit",
	},
	"linux-arm64-musl": {
		package: "@ifi/monokit-linux-arm64-musl",
		binary: "monokit",
	},
	"linux-x64-glibc": {
		package: "@ifi/monokit-linux-x64-gnu",
		binary: "monokit",
	},
	"linux-x64-musl": {
		package: "@ifi/monokit-linux-x64-musl",
		binary: "monokit",
	},
};

function detectLibc() {
	try {
		execFileSync("ldd", ["--version"], { stdio: "pipe" });
		return "glibc";
	} catch {
		return "musl";
	}
}

function resolveBinary() {
	const platform = process.platform;
	const arch = process.arch;

	let key;
	if (platform === "darwin") {
		key = `darwin-${arch}`;
	} else if (platform === "linux") {
		const libc = detectLibc();
		key = `linux-${arch}-${libc}`;
	} else {
		console.error(`Unsupported platform: ${platform}-${arch}`);
		process.exit(1);
	}

	const spec = PLATFORMS[key];
	if (!spec) {
		console.error(`No monokit binary available for ${key}`);
		process.exit(1);
	}

	try {
		const pkgPath = require.resolve(`${spec.package}/package.json`);
		const binPath = join(pkgPath, "..", "bin", spec.binary);
		if (existsSync(binPath)) {
			return binPath;
		}
	} catch {
		// Package not installed — fall through.
	}

	console.error(
		`Could not find monokit binary for ${key}.\n` +
			`Expected package: ${spec.package}\n` +
			`Try: npm install -g @ifi/monokit`,
	);
	process.exit(1);
}

const binary = resolveBinary();
const result = execFileSync(binary, process.argv.slice(2), {
	stdio: "inherit",
	env: process.env,
});
