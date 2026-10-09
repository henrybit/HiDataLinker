import { describe, expect, it } from 'vitest';
import { catalogSql } from './catalog-sql';

describe('catalogSql', () => {
	it('reads MySQL foreign keys from information_schema', () => {
		const sql = catalogSql('mysql', "a'b");
		expect(sql.foreignKeys).toContain('information_schema.KEY_COLUMN_USAGE');
		expect(sql.foreignKeys).toContain("'a''b'");
		expect(sql.objects).toContain('BASE TABLE');
	});

	it('reads PostgreSQL foreign keys from pg_constraint', () => {
		const sql = catalogSql('postgresql', 'public');
		expect(sql.foreignKeys).toContain('pg_constraint');
		expect(sql.foreignKeys).toContain("'public'");
		expect(sql.views).toContain('pg_get_viewdef');
	});

	it('reads SQL Server foreign keys after the session database is selected', () => {
		const sql = catalogSql('sqlserver', 'Adventure');
		expect(sql.foreignKeys).toContain('sys.foreign_keys');
		expect(sql.objects).not.toContain('Adventure');
		expect(sql.columns).toContain('MS_Description');
		expect(sql.views).toContain('ORDER BY s.name, v.name');
	});

	it('reads Oracle foreign keys from all_constraints', () => {
		const sql = catalogSql('oracle', 'HR');
		expect(sql.foreignKeys).toContain("c.constraint_type = 'R'");
		expect(sql.foreignKeys).toContain("'HR'");
		expect(sql.views).toContain('text_vc');
		expect(sql.objects).toContain('AS "comment"');
		expect(sql.columns).toContain('AS "comment"');
		expect(sql.objects).not.toMatch(/\bAS comment\b/);
	});

	it('keeps Oracle 11g view text out of the LONG subquery wrapper', () => {
		const sql = catalogSql('oracle', 'HR', '11.2');
		expect(sql.views).toContain('text AS definition');
		expect(sql.views).toContain('ROWNUM > 0');
		expect(sql.views).not.toContain('text_vc');
	});
});
