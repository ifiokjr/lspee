#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { join } from "node:path";

const require = createRequire(import.meta.url);

const PLATFORMS = {
	"darwin-arm64": {
		package: "@ifi/lspee-darwin-arm64",
		binary: "lspee",
	},
	"darwin-x64": {
		package: "@ifi/lspee-darwin-x64",
		binary: "lspee",
	},
	"linux-arm64-glibc": {
		package: "@ifi/lspee-linux-arm64-gnu",
		binary: "lspee",
	},
	"linux-arm64-musl": {
		package: "@ifi/lspee-linux-arm64-musl",
		binary: "lspee",
	},
	"linux-x64-glibc": {
		package: "@ifi/lspee-linux-x64-gnu",
		binary: "lspee",
	},
	"linux-x64-musl": {
		package: "@ifi/lspee-linux-x64-musl",
		binary: "lspee",
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
		console.error(`No lspee binary available for ${key}`);
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
		`Could not find lspee binary for ${key}.\n` +
			`Expected package: ${spec.package}\n` +
			`Try: npm install -g @ifi/lspee`,
	);
	process.exit(1);
}

const binary = resolveBinary();
const result = execFileSync(binary, process.argv.slice(2), {
	stdio: "inherit",
	env: process.env,
});
