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
	oracleConnect: string;
};

const URL_SCHEME = /^(postgresql|postgres|pgsql|mysql|mariadb|mssql|sqlserver|oracle):\/\//i;
/** DBeaver / JDBC thin: jdbc:oracle:thin:@host:port:SID or @//host:port/service */
const JDBC_ORACLE = /^jdbc:oracle:(?:thin|oci):(?:([^@]*)@)?(.+)$/i;
/** Easy Connect / JDBC SID form without a scheme: host:port:SID */
const EZCONNECT_SID = /^([^:/?#]+):(\d{1,5}):([^:/?#]+)$/;
/** Easy Connect service form without a scheme: host:port/service or //host:port/service */
const EZCONNECT_SERVICE = /^(?:\/\/)?([^:/?#]+)(?::(\d{1,5}))?\/([^/?#]+)$/;

export const CONNECTION_URL_PLACEHOLDERS = {
	mysql: 'mysql://root:password@127.0.0.1:3306/database',
	postgres: 'postgresql://postgres:password@127.0.0.1:5432/postgres',
	mssql: 'mssql://sa:password@127.0.0.1:1433/master',
	oracle: 'jdbc:oracle:thin:@127.0.0.1:1521:ORCL'
} as const;

export function connectionUrlPlaceholder(engine: string | undefined | null): string {
	return CONNECTION_URL_PLACEHOLDERS[normalizeEngine(engine)];
}

export function looksLikeConnectionUrl(value: string): boolean {
	const trimmed = value.trim();
	return (
		URL_SCHEME.test(trimmed) ||
		JDBC_ORACLE.test(trimmed) ||
		EZCONNECT_SID.test(trimmed) ||
		EZCONNECT_SERVICE.test(trimmed)
	);
}

export function parseConnectionUrl(input: string): ParsedConnectionUrl | null {
	const trimmed = input.trim();
	if (!trimmed) return null;

	const jdbc = parseJdbcOracleUrl(trimmed);
	if (jdbc) return jdbc;

	const ez = parseOracleEzConnect(trimmed);
	if (ez) return ez;

	if (!URL_SCHEME.test(trimmed)) return null;

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
	let oracleConnect = '';
	let resolvedDatabase = database;
	if (engine === 'oracle') {
		if (resolvedDatabase.toLowerCase().startsWith('sid:')) {
			resolvedDatabase = resolvedDatabase.slice(4);
			oracleConnect = 'sid';
		}
		const connect = firstSearchParam(url, ['connect', 'connectMode', 'connect-mode']).toLowerCase();
		if (connect === 'sid' || connect === 'service') oracleConnect = connect;
	}

	return {
		engine,
		host,
		port,
		username,
		password,
		database: resolvedDatabase,
		sslCa,
		sslCert,
		sslKey,
		sslVerify,
		oracleVersion: oracleVersion === 'auto' ? '' : oracleVersion,
		oracleConnect
	};
}

/**
 * Parse JDBC Oracle thin/OCI URLs the way DBeaver stores them.
 *
 * SID (colon):    jdbc:oracle:thin:@host:port:SID
 * Service (slash): jdbc:oracle:thin:@//host:port/service
 *                  jdbc:oracle:thin:@host:port/service
 * With user/pass: jdbc:oracle:thin:user/password@host:port:SID
 */
function parseJdbcOracleUrl(input: string): ParsedConnectionUrl | null {
	const match = input.match(JDBC_ORACLE);
	if (!match) return null;

	const userInfo = match[1] ?? '';
	let target = (match[2] ?? '').trim();
	if (!target) return null;

	// Drop query/hash from JDBC URLs (rare, but DBeaver may append props).
	const q = target.search(/[?#]/);
	let query = '';
	if (q >= 0) {
		query = target.slice(q + 1);
		target = target.slice(0, q);
	}

	if (target.startsWith('(')) {
		// Full TNS descriptor — not expanded into form fields.
		return null;
	}

	let username = '';
	let password = '';
	if (userInfo) {
		const slash = userInfo.indexOf('/');
		if (slash >= 0) {
			username = decodeUrlComponent(userInfo.slice(0, slash));
			password = decodeUrlComponent(userInfo.slice(slash + 1));
		} else {
			username = decodeUrlComponent(userInfo);
		}
	}

	const endpoint = parseOracleEndpoint(target);
	if (!endpoint) return null;

	const preset = ENGINE_PRESETS.oracle;
	const params = new URLSearchParams(query.startsWith('?') ? query.slice(1) : query);
	const oracleVersion = normalizeOracleVersion(
		params.get('oracleVersion') || params.get('oracle-version') || params.get('oracleversion') || ''
	);

	return {
		engine: 'oracle',
		host: endpoint.host,
		port: endpoint.port,
		username: username || preset.username,
		password,
		database: endpoint.database,
		sslCa: '',
		sslCert: '',
		sslKey: '',
		sslVerify: false,
		oracleVersion: oracleVersion === 'auto' ? '' : oracleVersion,
		oracleConnect: endpoint.oracleConnect
	};
}

function parseOracleEzConnect(input: string): ParsedConnectionUrl | null {
	if (URL_SCHEME.test(input) || JDBC_ORACLE.test(input)) return null;

	const sid = input.match(EZCONNECT_SID);
	if (sid) {
		const port = Number(sid[2]);
		if (!Number.isFinite(port) || port <= 0 || port > 65535) return null;
		const preset = ENGINE_PRESETS.oracle;
		return {
			engine: 'oracle',
			host: sid[1],
			port,
			username: preset.username,
			password: '',
			database: sid[3],
			sslCa: '',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: '',
			oracleConnect: 'sid'
		};
	}

	const service = input.match(EZCONNECT_SERVICE);
	if (service) {
		const port = service[2] ? Number(service[2]) : ENGINE_PRESETS.oracle.port;
		if (!Number.isFinite(port) || port <= 0 || port > 65535) return null;
		const preset = ENGINE_PRESETS.oracle;
		return {
			engine: 'oracle',
			host: service[1],
			port,
			username: preset.username,
			password: '',
			database: service[3],
			sslCa: '',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: '',
			oracleConnect: 'service'
		};
	}

	return null;
}

/** Parse the part after `@` in a JDBC thin URL, or a bare EZConnect target. */
function parseOracleEndpoint(
	target: string
): { host: string; port: number; database: string; oracleConnect: string } | null {
	let value = target.trim();
	if (value.startsWith('@')) value = value.slice(1).trim();

	// Service name forms: //host[:port]/service  or  host:port/service  or  host/service
	const serviceSlash = value.match(/^(?:\/\/)?([^:/?#]+)(?::(\d{1,5}))?\/([^/?#]+)$/);
	if (serviceSlash) {
		const port = serviceSlash[2] ? Number(serviceSlash[2]) : ENGINE_PRESETS.oracle.port;
		if (!Number.isFinite(port) || port <= 0 || port > 65535) return null;
		return {
			host: serviceSlash[1],
			port,
			database: serviceSlash[3],
			oracleConnect: 'service'
		};
	}

	// SID form used by DBeaver: host:port:SID
	const sid = value.match(/^([^:/?#]+):(\d{1,5}):([^:/?#]+)$/);
	if (sid) {
		const port = Number(sid[2]);
		if (!Number.isFinite(port) || port <= 0 || port > 65535) return null;
		return {
			host: sid[1],
			port,
			database: sid[3],
			oracleConnect: 'sid'
		};
	}

	// host:port only
	const hostPort = value.match(/^([^:/?#]+):(\d{1,5})$/);
	if (hostPort) {
		const port = Number(hostPort[2]);
		if (!Number.isFinite(port) || port <= 0 || port > 65535) return null;
		return {
			host: hostPort[1],
			port,
			database: ENGINE_PRESETS.oracle.database,
			oracleConnect: 'service'
		};
	}

	if (/^[^:/?#]+$/.test(value)) {
		return {
			host: value,
			port: ENGINE_PRESETS.oracle.port,
			database: ENGINE_PRESETS.oracle.database,
			oracleConnect: 'service'
		};
	}

	return null;
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
