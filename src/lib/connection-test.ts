import { normalizeEngine } from '$lib/engine';
import type { ConnectionListItem, TestConnectionRequest } from '$lib/api/types';

/** Fields safe to print when an Oracle connect fails. Password is not included. */
export function oracleConnectLogDetail(target: {
	host?: string;
	port?: number;
	username?: string;
	database?: string | null;
	oracleVersion?: string | null;
	oracleConnect?: string | null;
	sslVerify?: boolean;
}): string {
	const connect = target.oracleConnect === 'sid' ? 'sid' : 'service';
	return [
		`host=${target.host ?? ''}`,
		`port=${target.port ?? ''}`,
		`user=${target.username ?? ''}`,
		`database=${target.database ?? ''}`,
		`version=${target.oracleVersion || 'auto'}`,
		`connect=${connect}`,
		`tlsVerify=${target.sslVerify === true}`
	].join(' ');
}

/** Build a one-shot `test_connection` payload from a saved profile. */
export function testRequestFromConnection(item: ConnectionListItem): TestConnectionRequest {
	const engine = normalizeEngine(item.engine);
	return {
		engine: item.engine,
		host: item.host,
		port: item.port,
		username: item.username,
		password: item.password,
		database: item.database,
		sslCa: item.sslCa,
		sslCert: item.sslCert,
		sslKey: item.sslKey,
		sslVerify: (engine === 'mssql' || engine === 'oracle') && item.sslVerify === true,
		oracleVersion: engine === 'oracle' ? item.oracleVersion || 'auto' : null,
		oracleConnect: engine === 'oracle' && item.oracleConnect === 'sid' ? 'sid' : null
	};
}
