#!/usr/bin/env node

/**
 * Builds platform-specific npm packages from release binaries.
 *
 * Usage: node scripts/npm/build-packages.mjs --version 0.1.0 --artifacts ./artifacts --out ./dist
 */

import { cpSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

const { values } = parseArgs({
	options: {
		version: { type: "string" },
		artifacts: { type: "string", default: "./artifacts" },
		out: { type: "string", default: "./dist" },
	},
	strict: true,
});

if (!values.version) {
	console.error("--version is required");
	process.exit(1);
}

const VERSION = values.version;
const ARTIFACTS = values.artifacts;
const OUT = values.out;

const PLATFORMS = [
	{
		packageName: "@ifi/lspee-darwin-arm64",
		target: "aarch64-apple-darwin",
		os: "darwin",
		cpu: "arm64",
		binaryName: "lspee",
		archiveExt: "tar.gz",
	},
	{
		packageName: "@ifi/lspee-darwin-x64",
		target: "x86_64-apple-darwin",
		os: "darwin",
		cpu: "x64",
		binaryName: "lspee",
		archiveExt: "tar.gz",
	},
	{
		packageName: "@ifi/lspee-linux-arm64-gnu",
		target: "aarch64-unknown-linux-gnu",
		os: "linux",
		cpu: "arm64",
		libc: "glibc",
		binaryName: "lspee",
		archiveExt: "tar.gz",
	},
	{
		packageName: "@ifi/lspee-linux-arm64-musl",
		target: "aarch64-unknown-linux-musl",
		os: "linux",
		cpu: "arm64",
		libc: "musl",
		binaryName: "lspee",
		archiveExt: "tar.gz",
	},
	{
		packageName: "@ifi/lspee-linux-x64-gnu",
		target: "x86_64-unknown-linux-gnu",
		os: "linux",
		cpu: "x64",
		libc: "glibc",
		binaryName: "lspee",
		archiveExt: "tar.gz",
	},
	{
		packageName: "@ifi/lspee-linux-x64-musl",
		target: "x86_64-unknown-linux-musl",
		os: "linux",
		cpu: "x64",
		libc: "musl",
		binaryName: "lspee",
		archiveExt: "tar.gz",
	},
];

mkdirSync(OUT, { recursive: true });

for (const platform of PLATFORMS) {
	const pkgDir = join(OUT, platform.packageName.replace("/", "-"));
	const binDir = join(pkgDir, "bin");
	mkdirSync(binDir, { recursive: true });

	const binarySource = join(ARTIFACTS, platform.target, platform.binaryName);
	if (!existsSync(binarySource)) {
		console.warn(
			`Skipping ${platform.packageName}: binary not found at ${binarySource}`,
		);
		continue;
	}

	cpSync(binarySource, join(binDir, platform.binaryName));

	const pkg = {
		name: platform.packageName,
		version: VERSION,
		description: `Prebuilt lspee binary for ${platform.os} ${platform.cpu}${
			platform.libc ? ` (${platform.libc})` : ""
		}`,
		license: "MIT OR Apache-2.0",
		repository: {
			type: "git",
			url: "git+https://github.com/ifiokjr/lspee.git",
		},
		os: [platform.os],
		cpu: [platform.cpu],
		...(platform.libc ? { libc: [platform.libc] } : {}),
		files: ["bin", "LICENSE"],
		publishConfig: { access: "public", provenance: true },
	};

	writeFileSync(
		join(pkgDir, "package.json"),
		JSON.stringify(pkg, null, 2) + "\n",
	);
	console.log(`Built ${platform.packageName}`);
}

// Build root package
const rootDir = join(OUT, "@ifi-lspee");
const rootBinDir = join(rootDir, "bin");
mkdirSync(rootBinDir, { recursive: true });

cpSync(
	join(import.meta.dirname, "..", "..", "npm", "bin", "lspee.js"),
	join(rootBinDir, "lspee.js"),
);

const optionalDependencies = {};
for (const platform of PLATFORMS) {
	optionalDependencies[platform.packageName] = VERSION;
}

const rootPkg = {
	name: "@ifi/lspee",
	version: VERSION,
	description:
		"Local LSP multiplexer for fast, shared, per-workspace language-server access",
	license: "MIT OR Apache-2.0",
	repository: {
		type: "git",
		url: "git+https://github.com/ifiokjr/lspee.git",
	},
	bin: { lspee: "bin/lspee.js" },
	optionalDependencies,
	files: ["bin", "LICENSE", "README.md"],
	publishConfig: { access: "public", provenance: true },
	engines: { node: ">=18" },
	type: "module",
};

writeFileSync(
	join(rootDir, "package.json"),
	JSON.stringify(rootPkg, null, 2) + "\n",
);
console.log(`Built @ifi/lspee (root)`);

console.log("Done.");
