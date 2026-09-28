import { describe, expect, it } from 'vitest';
import {
	engineLabel,
	normalizeEngine,
	qualifyIdent,
	quoteIdent,
	quoteLiteral,
	schemaNoun
} from './engine';

describe('engine helpers', () => {
	it('normalizes engine aliases', () => {
		expect(normalizeEngine('PostgreSQL')).toBe('postgres');
		expect(normalizeEngine('pgsql')).toBe('postgres');
		expect(normalizeEngine('mysql')).toBe('mysql');
		expect(normalizeEngine('sqlserver')).toBe('mssql');
		expect(normalizeEngine('Oracle')).toBe('oracle');
		expect(engineLabel('postgres')).toBe('PostgreSQL');
		expect(engineLabel('mssql')).toBe('SQL Server');
		expect(engineLabel('oracle')).toBe('Oracle');
		expect(schemaNoun('mysql')).toBe('Database');
		expect(schemaNoun('pgsql')).toBe('Schema');
		expect(schemaNoun('mssql')).toBe('Database');
		expect(schemaNoun('oracle')).toBe('Schema');
	});

	it('quotes identifiers per dialect', () => {
		expect(quoteIdent('mysql', 'a`b')).toBe('`a``b`');
		expect(quoteIdent('postgres', 'a"b')).toBe('"a""b"');
		expect(quoteIdent('mssql', 'a]b')).toBe('[a]]b]');
		expect(quoteIdent('oracle', 'a"b')).toBe('"a""b"');
		expect(qualifyIdent('postgres', 'public', 'users')).toBe('"public"."users"');
		expect(qualifyIdent('mysql', 'shop')).toBe('`shop`.');
		expect(qualifyIdent('mssql', 'Adventure', 'dbo.Users')).toBe('[Adventure].[dbo].[Users]');
		expect(qualifyIdent('oracle', 'HR', 'EMP')).toBe('"HR"."EMP"');
	});

	it('quotes SQL literals', () => {
		expect(quoteLiteral(null)).toBe('NULL');
		expect(quoteLiteral("O'Brien")).toBe("'O''Brien'");
	});
});
