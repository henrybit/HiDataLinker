#!/usr/bin/env node
/**
 * Download a platform Node binary for Tauri `bundle.externalBin`.
 * Output: src-tauri/binaries/node-<target-triple>[.exe]
 *
 * Usage:
 *   node scripts/fetch-bundled-node.mjs
 *   node scripts/fetch-bundled-node.mjs --force
 */
import { createWriteStream, existsSync, mkdirSync, chmodSync, unlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { pipeline } from 'node:stream/promises';
import { createGunzip } from 'node:zlib';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { Readable } from 'node:stream';

const NODE_VERSION = process.env.HIDATALINKER_NODE_VERSION || '22.14.0';
const force = process.argv.includes('--force');

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(__dirname, '..');
const outDir = join(repoRoot, 'src-tauri', 'binaries');

function detectTarget() {
	const platform = process.platform;
	const arch = process.arch;
	if (platform === 'darwin' && arch === 'arm64') {
		return {
			triple: 'aarch64-apple-darwin',
			nodeArch: 'darwin-arm64',
			archive: 'tar.gz',
			binName: 'node'
		};
	}
	if (platform === 'darwin' && arch === 'x64') {
		return {
			triple: 'x86_64-apple-darwin',
			nodeArch: 'darwin-x64',
			archive: 'tar.gz',
			binName: 'node'
		};
	}
	if (platform === 'linux' && arch === 'arm64') {
		return {
			triple: 'aarch64-unknown-linux-gnu',
			nodeArch: 'linux-arm64',
			archive: 'tar.gz',
			binName: 'node'
		};
	}
	if (platform === 'linux' && arch === 'x64') {
		return {
			triple: 'x86_64-unknown-linux-gnu',
			nodeArch: 'linux-x64',
			archive: 'tar.gz',
			binName: 'node'
		};
	}
	if (platform === 'win32' && arch === 'arm64') {
		return {
			triple: 'aarch64-pc-windows-msvc',
			nodeArch: 'win-arm64',
			archive: 'zip',
			binName: 'node.exe'
		};
	}
	if (platform === 'win32' && arch === 'x64') {
		return {
			triple: 'x86_64-pc-windows-msvc',
			nodeArch: 'win-x64',
			archive: 'zip',
			binName: 'node.exe'
		};
	}
	throw new Error(`Unsupported platform for bundled node: ${platform}/${arch}`);
}

async function download(url, dest) {
	const res = await fetch(url);
	if (!res.ok || !res.body) {
		throw new Error(`Download failed ${res.status} ${url}`);
	}
	await pipeline(Readable.fromWeb(res.body), createWriteStream(dest));
}

function extractNodeBinary(archivePath, archiveKind, nodeArch, binName, outPath) {
	const extractRoot = join(tmpdir(), `hidatalinker-node-${NODE_VERSION}-${nodeArch}`);
	mkdirSync(extractRoot, { recursive: true });
	if (archiveKind === 'tar.gz') {
		execFileSync('tar', ['-xzf', archivePath, '-C', extractRoot], { stdio: 'inherit' });
		const extracted = join(
			extractRoot,
			`node-v${NODE_VERSION}-${nodeArch}`,
			'bin',
			binName
		);
		if (!existsSync(extracted)) {
			throw new Error(`Extracted node binary missing: ${extracted}`);
		}
		execFileSync('cp', [extracted, outPath], { stdio: 'inherit' });
		chmodSync(outPath, 0o755);
		return;
	}
	// Windows zip — use PowerShell Expand-Archive when available, else tar.
	const destDir = join(extractRoot, 'unzipped');
	mkdirSync(destDir, { recursive: true });
	try {
		execFileSync(
			'powershell.exe',
			['-NoProfile', '-Command', `Expand-Archive -Force -Path '${archivePath}' -DestinationPath '${destDir}'`],
			{ stdio: 'inherit' }
		);
	} catch {
		execFileSync('tar', ['-xf', archivePath, '-C', destDir], { stdio: 'inherit' });
	}
	const extracted = join(destDir, `node-v${NODE_VERSION}-${nodeArch}`, binName);
	if (!existsSync(extracted)) {
		throw new Error(`Extracted node binary missing: ${extracted}`);
	}
	execFileSync('cmd.exe', ['/c', 'copy', '/Y', extracted, outPath], { stdio: 'inherit' });
}

async function main() {
	const target = detectTarget();
	mkdirSync(outDir, { recursive: true });
	const outName = target.binName.endsWith('.exe')
		? `node-${target.triple}.exe`
		: `node-${target.triple}`;
	const outPath = join(outDir, outName);

	if (existsSync(outPath) && !force) {
		console.log(`[fetch-bundled-node] exists: ${outPath}`);
		return;
	}

	const file = `node-v${NODE_VERSION}-${target.nodeArch}.${target.archive}`;
	const url = `https://nodejs.org/dist/v${NODE_VERSION}/${file}`;
	const archivePath = join(tmpdir(), file);
	console.log(`[fetch-bundled-node] downloading ${url}`);
	await download(url, archivePath);
	try {
		if (existsSync(outPath)) unlinkSync(outPath);
		extractNodeBinary(archivePath, target.archive, target.nodeArch, target.binName, outPath);
		console.log(`[fetch-bundled-node] wrote ${outPath}`);
	} finally {
		try {
			unlinkSync(archivePath);
		} catch {
			/* ignore */
		}
	}
}

main().catch((err) => {
	console.error('[fetch-bundled-node]', err);
	process.exit(1);
});
