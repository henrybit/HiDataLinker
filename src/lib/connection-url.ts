import { ENGINE_PRESETS, normalizeEngine, type EngineKind } from '$lib/engine';

export type ParsedConnectionUrl = {
	engine: EngineKind;
	host: string;
	port: number;
	username: string;
	password: string;
	database: string;
	sslCa: string;
	sslCert: string;
	sslKey: string;
};

const URL_SCHEME = /^(postgresql|postgres|pgsql|mysql|mariadb):\/\//i;

export const CONNECTION_URL_PLACEHOLDERS = {
	mysql: 'mysql://root:password@127.0.0.1:3306/database',
	postgres: 'postgresql://postgres:password@127.0.0.1:5432/postgres'
} as const;

export function connectionUrlPlaceholder(engine: string | undefined | null): string {
	return CONNECTION_URL_PLACEHOLDERS[normalizeEngine(engine)];
}

export function looksLikeConnectionUrl(value: string): boolean {
	return URL_SCHEME.test(value.trim());
}

export function parseConnectionUrl(input: string): ParsedConnectionUrl | null {
	const trimmed = input.trim();
	if (!looksLikeConnectionUrl(trimmed)) return null;

	let url: URL;
	try {
		url = new URL(trimmed);
	} catch {
		return null;
	}

	const scheme = url.protocol.replace(/:$/, '').toLowerCase();
	const engine = normalizeEngine(scheme === 'mysql' || scheme === 'mariadb' ? 'mysql' : 'postgres');
	const host = url.hostname.trim();
	if (!host) return null;

	const preset = ENGINE_PRESETS[engine];
	const port = url.port ? Number(url.port) : preset.port;
	if (!Number.isFinite(port) || port <= 0 || port > 65535) return null;

	const username = decodeUrlComponent(url.username) || preset.username;
	const password = decodeUrlComponent(url.password);
	const database =
		decodeUrlComponent(url.pathname.replace(/^\/+/, '').replace(/\/+$/, '')) || preset.database;
	const sslCa = firstSearchParam(url, [
		'ssl-ca',
		'ssl_ca',
		'sslca',
		'ca',
		'sslrootcert',
		'ssl_root_cert'
	]);
	const sslCert = firstSearchParam(url, ['sslcert', 'ssl_cert', 'ssl-cert']);
	const sslKey = firstSearchParam(url, ['sslkey', 'ssl_key', 'ssl-key']);

	return { engine, host, port, username, password, database, sslCa, sslCert, sslKey };
}

function firstSearchParam(url: URL, names: string[]): string {
	for (const name of names) {
		const value = url.searchParams.get(name);
		if (value?.trim()) return value.trim();
	}
	return '';
}

function decodeUrlComponent(value: string): string {
	if (!value) return '';
	try {
		return decodeURIComponent(value);
	} catch {
		return value;
	}
}
