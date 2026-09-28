import { describe, expect, it } from 'vitest';
import {
	connectionUrlPlaceholder,
	looksLikeConnectionUrl,
	parseConnectionUrl
} from './connection-url';

describe('connection url', () => {
	it('uses engine-specific placeholders', () => {
		expect(connectionUrlPlaceholder('mysql')).toBe('mysql://root:password@127.0.0.1:3306/database');
		expect(connectionUrlPlaceholder('pgsql')).toBe(
			'postgresql://postgres:password@127.0.0.1:5432/postgres'
		);
		expect(connectionUrlPlaceholder('mssql')).toBe('mssql://sa:password@127.0.0.1:1433/master');
		expect(connectionUrlPlaceholder('oracle')).toBe(
			'oracle://system:password@127.0.0.1:1521/FREEPDB1'
		);
	});

	it('detects supported schemes', () => {
		expect(looksLikeConnectionUrl('postgresql://u:p@h:5432/db')).toBe(true);
		expect(looksLikeConnectionUrl('postgres://h/db')).toBe(true);
		expect(looksLikeConnectionUrl('pgsql://h/db')).toBe(true);
		expect(looksLikeConnectionUrl('mysql://root@127.0.0.1:3306/app')).toBe(true);
		expect(looksLikeConnectionUrl('mariadb://root@127.0.0.1/app')).toBe(true);
		expect(looksLikeConnectionUrl('mssql://sa@127.0.0.1:1433/master')).toBe(true);
		expect(looksLikeConnectionUrl('oracle://system@127.0.0.1:1521/FREEPDB1')).toBe(true);
		expect(looksLikeConnectionUrl('127.0.0.1')).toBe(false);
	});

	it('parses mysql and mariadb urls', () => {
		expect(parseConnectionUrl('mysql://app:s3cret@db.example.com:3306/shop')).toEqual({
			engine: 'mysql',
			host: 'db.example.com',
			port: 3306,
			username: 'app',
			password: 's3cret',
			database: 'shop',
			sslCa: '',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: ''
		});
		expect(parseConnectionUrl('mariadb://root@127.0.0.1/app')).toEqual({
			engine: 'mysql',
			host: '127.0.0.1',
			port: 3306,
			username: 'root',
			password: '',
			database: 'app',
			sslCa: '',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: ''
		});
	});

	it('parses mysql ssl-ca query parameter', () => {
		expect(
			parseConnectionUrl(
				'mysql://3RongwYzrjiCbcf.root:secret@gateway01.example.com:4000/account?ssl-ca=/etc/ssl/cert.pem'
			)
		).toEqual({
			engine: 'mysql',
			host: 'gateway01.example.com',
			port: 4000,
			username: '3RongwYzrjiCbcf.root',
			password: 'secret',
			database: 'account',
			sslCa: '/etc/ssl/cert.pem',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: ''
		});
		expect(
			parseConnectionUrl('mysql://root@127.0.0.1:3306/app?sslca=%2Fetc%2Fssl%2Fcert.pem')?.sslCa
		).toBe('/etc/ssl/cert.pem');
	});

	it('parses postgresql ssl certificate query parameters', () => {
		expect(
			parseConnectionUrl(
				'postgresql://postgres:secret@db.example.com:5432/postgres?sslrootcert=/etc/ssl/root.crt&sslcert=/etc/ssl/client.crt&sslkey=/etc/ssl/client.key'
			)
		).toEqual({
			engine: 'postgres',
			host: 'db.example.com',
			port: 5432,
			username: 'postgres',
			password: 'secret',
			database: 'postgres',
			sslCa: '/etc/ssl/root.crt',
			sslCert: '/etc/ssl/client.crt',
			sslKey: '/etc/ssl/client.key',
			sslVerify: false,
			oracleVersion: ''
		});
	});

	it('parses supabase-style postgresql urls', () => {
		const parsed = parseConnectionUrl(
			'postgresql://postgres:[YOUR-PASSWORD]@spb-bp1gvnmm7m9oat33.supabase.opentrust.net:5432/postgres'
		);
		expect(parsed).toEqual({
			engine: 'postgres',
			host: 'spb-bp1gvnmm7m9oat33.supabase.opentrust.net',
			port: 5432,
			username: 'postgres',
			password: '[YOUR-PASSWORD]',
			database: 'postgres',
			sslCa: '',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: ''
		});
	});

	it('decodes percent-encoded credentials and database', () => {
		const parsed = parseConnectionUrl(
			'postgresql://user:p%40ss%3Aword@db.example.com:5432/my%2Fdb'
		);
		expect(parsed).toEqual({
			engine: 'postgres',
			host: 'db.example.com',
			port: 5432,
			username: 'user',
			password: 'p@ss:word',
			database: 'my/db',
			sslCa: '',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: ''
		});
	});

	it('applies engine defaults when port or database omitted', () => {
		expect(parseConnectionUrl('postgres://alice@db.example.com')).toEqual({
			engine: 'postgres',
			host: 'db.example.com',
			port: 5432,
			username: 'alice',
			password: '',
			database: 'postgres',
			sslCa: '',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: ''
		});
		expect(parseConnectionUrl('mysql://root@127.0.0.1/')).toEqual({
			engine: 'mysql',
			host: '127.0.0.1',
			port: 3306,
			username: 'root',
			password: '',
			database: '',
			sslCa: '',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: ''
		});
	});

	it('returns null for invalid urls', () => {
		expect(parseConnectionUrl('postgresql://')).toBeNull();
		expect(parseConnectionUrl('not-a-url')).toBeNull();
	});

	it('ignores an untrusted SQL Server certificate unless verification is requested', () => {
		expect(parseConnectionUrl('mssql://sa:secret@db.example.com/master')?.sslVerify).toBe(false);
		expect(
			parseConnectionUrl('mssql://sa@db.example.com/master?trustServerCertificate=true')?.sslVerify
		).toBe(false);
		expect(
			parseConnectionUrl('mssql://sa@db.example.com/master?trustServerCertificate=false')?.sslVerify
		).toBe(true);
		expect(parseConnectionUrl('mssql://sa@db.example.com/master?sslverify=1')?.sslVerify).toBe(
			true
		);
	});

	it('ignores an untrusted Oracle certificate unless verification is requested', () => {
		expect(parseConnectionUrl('oracle://system:secret@db.example.com/FREEPDB1')?.sslVerify).toBe(
			false
		);
		expect(
			parseConnectionUrl('oracle://system@db.example.com/FREEPDB1?sslverify=1')?.sslVerify
		).toBe(true);
	});

	it('reads an Oracle release from the connection url', () => {
		expect(
			parseConnectionUrl('oracle://system@db.example.com/FREEPDB1?oracleVersion=11.2')
				?.oracleVersion
		).toBe('11.2');
		expect(parseConnectionUrl('oracle://system@db.example.com/FREEPDB1')?.oracleVersion).toBe(
			''
		);
		expect(parseConnectionUrl('mysql://root@127.0.0.1/app?oracleVersion=11.2')?.oracleVersion).toBe(
			''
		);
	});
});
