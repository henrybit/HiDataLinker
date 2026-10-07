import { describe, expect, it } from 'vitest';
import { testRequestFromConnection } from './connection-test';
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
