import { describe, expect, it } from 'vitest';
import { formatSql, sqlLanguageFor } from './format';

describe('sql format helpers', () => {
	it('maps engines to formatter languages', () => {
		expect(sqlLanguageFor('pgsql')).toBe('postgresql');
		expect(sqlLanguageFor('mysql')).toBe('mysql');
		expect(sqlLanguageFor('oracle')).toBe('plsql');
		expect(sqlLanguageFor('mssql')).toBe('tsql');
		expect(sqlLanguageFor(null)).toBe('sql');
	});

	it('formats SQL with dialect-aware keyword casing', () => {
		const formatted = formatSql('select id,name from users where id=1', 'postgres');
		expect(formatted).toContain('SELECT');
		expect(formatted).toContain('FROM');
		expect(formatted).toContain('WHERE');
		expect(formatted.split('\n').length).toBeGreaterThan(1);
	});

	it('leaves blank SQL unchanged', () => {
		expect(formatSql('   ', 'mysql')).toBe('   ');
		expect(formatSql('', 'mysql')).toBe('');
	});
});
