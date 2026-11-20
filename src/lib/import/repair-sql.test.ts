import { describe, expect, it } from 'vitest';
import type { ParseSample } from '$lib/api/types';
import {
	columnLengthHints,
	ensureDropBeforeCreate,
	repairImportSqlLocally,
	schemaSqlFromPreview,
	stripSqlFence,
	widenStringColumns
} from './repair-sql';

const createSql = `CREATE TABLE "Test"."faostat_data_en_11_20_2026" (
  "domain_code" VARCHAR(32) NOT NULL,
  "domain" VARCHAR(32) NOT NULL,
  "value" NUMERIC(18,6) NOT NULL
);`;

describe('stripSqlFence', () => {
	it('unwraps a sql fence', () => {
		expect(stripSqlFence('```sql\nCREATE TABLE t (a TEXT);\n```')).toBe(
			'CREATE TABLE t (a TEXT);'
		);
	});
});

describe('schemaSqlFromPreview', () => {
	it('drops the load-plan comment block', () => {
		const preview = `${createSql}\n\n-- Import load plan\n-- Source: a.csv`;
		expect(schemaSqlFromPreview(preview)).toBe(createSql);
	});
});

describe('widenStringColumns', () => {
	it('replaces varchar and character varying with TEXT', () => {
		const sql = 'CREATE TABLE t ("a" VARCHAR(32), "b" character varying(64), "c" INT);';
		expect(widenStringColumns(sql)).toBe(
			'CREATE TABLE t ("a" TEXT, "b" TEXT, "c" INT);'
		);
	});
});

describe('ensureDropBeforeCreate', () => {
	it('prepends drop for a qualified quoted name', () => {
		expect(ensureDropBeforeCreate(createSql)).toContain(
			'DROP TABLE IF EXISTS "Test"."faostat_data_en_11_20_2026";'
		);
	});

	it('does not add a second drop', () => {
		const sql = 'DROP TABLE IF EXISTS t;\nCREATE TABLE t (a TEXT);';
		expect(ensureDropBeforeCreate(sql)).toBe(sql);
	});
});

describe('repairImportSqlLocally', () => {
	it('widens varchar and drops the existing table on 22001', () => {
		const fixed = repairImportSqlLocally(
			createSql,
			'ERROR: value too long for type character varying(32) (22001)'
		);
		expect(fixed?.sql).toContain('TEXT');
		expect(fixed?.sql).not.toMatch(/VARCHAR\s*\(/i);
		expect(fixed?.sql.startsWith('DROP TABLE IF EXISTS')).toBe(true);
		expect(fixed?.sql).toContain('NUMERIC(18,6)');
	});

	it('leaves unrelated errors alone', () => {
		expect(repairImportSqlLocally(createSql, 'permission denied')).toBeNull();
	});
});

describe('columnLengthHints', () => {
	it('reports the longest sample value per column', () => {
		const sample: ParseSample = {
			headers: ['name'],
			sampleRows: [['ab'], ['abcd']],
			totalRows: 2,
			columnCount: 1,
			columnStats: [],
			suggestedTable: 't'
		};
		expect(columnLengthHints(sample)).toContain('sampleMaxChars=4');
	});
});
