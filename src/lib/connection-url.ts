import { ENGINE_PRESETS, normalizeEngine, type EngineKind } from '$lib/engine';
import { normalizeOracleVersion } from '$lib/oracle-version';

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
	sslVerify: boolean;
	oracleVersion: string;
};

const URL_SCHEME = /^(postgresql|postgres|pgsql|mysql|mariadb|mssql|sqlserver|oracle):\/\//i;

export const CONNECTION_URL_PLACEHOLDERS = {
	mysql: 'mysql://root:password@127.0.0.1:3306/database',
	postgres: 'postgresql://postgres:password@127.0.0.1:5432/postgres',
	mssql: 'mssql://sa:password@127.0.0.1:1433/master',
	oracle: 'oracle://system:password@127.0.0.1:1521/FREEPDB1'
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
	const engine = normalizeEngine(scheme);
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
	const sslVerify = parseSslVerify(url);
	const oracleVersion =
		engine === 'oracle'
			? normalizeOracleVersion(
					firstSearchParam(url, ['oracleVersion', 'oracle-version', 'oracleversion'])
				)
			: 'auto';

	return {
		engine,
		host,
		port,
		username,
		password,
		database,
		sslCa,
		sslCert,
		sslKey,
		sslVerify,
		oracleVersion: oracleVersion === 'auto' ? '' : oracleVersion
	};
}

function firstSearchParam(url: URL, names: string[]): string {
	for (const name of names) {
		const value = url.searchParams.get(name);
		if (value?.trim()) return value.trim();
	}
	return '';
}

/** True only when the URL explicitly asks to validate the SQL Server certificate. */
function parseSslVerify(url: URL): boolean {
	const verify = firstSearchParam(url, [
		'sslverify',
		'ssl-verify',
		'verifyServerCertificate',
		'verifyservercertificate'
	]);
	if (verify) return truthy(verify);
	const trust = firstSearchParam(url, [
		'trustServerCertificate',
		'trustservercertificate',
		'TrustServerCertificate'
	]);
	if (trust) return !truthy(trust);
	return false;
}

function truthy(value: string): boolean {
	const normalized = value.trim().toLowerCase();
	return normalized === '1' || normalized === 'true' || normalized === 'yes';
}

function decodeUrlComponent(value: string): string {
	if (!value) return '';
	try {
		return decodeURIComponent(value);
	} catch {
		return value;
	}
}
