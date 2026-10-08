import { describe, expect, it } from 'vitest';
import { oracleConnectLogDetail, testRequestFromConnection } from './connection-test';
import type { ConnectionListItem } from './api/types';

function base(overrides: Partial<ConnectionListItem> = {}): ConnectionListItem {
	return {
		id: 'c1',
		name: 'Local',
		engine: 'mysql',
		host: '127.0.0.1',
		port: 3306,
		username: 'root',
		password: 'secret',
		database: 'app',
		sslCa: null,
		sslCert: null,
		sslKey: null,
		sslVerify: false,
		oracleVersion: null,
		oracleConnect: null,
		savePassword: true,
		connected: false,
		...overrides
	};
}

describe('oracleConnectLogDetail', () => {
	it('prints the target and leaves the password out', () => {
		const request = testRequestFromConnection(
			base({
				engine: 'oracle',
				host: '172.19.3.11',
				port: 7026,
				username: 'system',
				password: 's3cret-password',
				database: 'DB11G',
				oracleVersion: '11.2',
				oracleConnect: 'sid',
				sslVerify: false
			})
		);
		const line = oracleConnectLogDetail(request);
		expect(line).toBe(
			'host=172.19.3.11 port=7026 user=system database=DB11G version=11.2 connect=sid tlsVerify=false'
		);
		expect(line).not.toContain('s3cret-password');
	});
});

describe('testRequestFromConnection', () => {
	it('maps a MySQL profile', () => {
		expect(testRequestFromConnection(base())).toEqual({
			engine: 'mysql',
			host: '127.0.0.1',
			port: 3306,
			username: 'root',
			password: 'secret',
			database: 'app',
			sslCa: null,
			sslCert: null,
			sslKey: null,
			sslVerify: false,
			oracleVersion: null,
			oracleConnect: null
		});
	});

	it('keeps Oracle SID and version fields', () => {
		const request = testRequestFromConnection(
			base({
				engine: 'oracle',
				port: 1521,
				oracleVersion: '19c',
				oracleConnect: 'sid',
				sslVerify: true
			})
		);
		expect(request.oracleVersion).toBe('19c');
		expect(request.oracleConnect).toBe('sid');
		expect(request.sslVerify).toBe(true);
	});
});
