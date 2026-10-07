import { normalizeEngine } from '$lib/engine';
import type { ConnectionListItem, TestConnectionRequest } from '$lib/api/types';

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
