import { normalizeEngine, quoteLiteral } from '$lib/engine';

export interface CatalogSql {
	objects: string;
	columns: string;
	foreignKeys: string;
	views: string;
}

export function catalogSql(engine: string, schema: string): CatalogSql {
	const literal = quoteLiteral(schema);
	const kind = normalizeEngine(engine);
	if (kind === 'postgres') return postgresSql(literal);
	if (kind === 'mssql') return mssqlSql();
	if (kind === 'oracle') return oracleSql(literal);
	return mysqlSql(literal);
}

function mysqlSql(schema: string): CatalogSql {
	return {
		objects: `
SELECT TABLE_NAME AS object_name,
       CASE WHEN TABLE_TYPE = 'VIEW' THEN 'view' ELSE 'table' END AS object_kind,
       TABLE_COMMENT AS comment
FROM information_schema.TABLES
WHERE TABLE_SCHEMA = ${schema}
  AND TABLE_TYPE IN ('BASE TABLE', 'VIEW')
ORDER BY TABLE_NAME`.trim(),
		columns: `
SELECT TABLE_NAME AS object_name,
       COLUMN_NAME AS column_name,
       COLUMN_TYPE AS column_type,
       COLUMN_KEY AS column_key,
       COLUMN_COMMENT AS comment,
       ORDINAL_POSITION AS ordinal_position
FROM information_schema.COLUMNS
WHERE TABLE_SCHEMA = ${schema}
ORDER BY TABLE_NAME, ORDINAL_POSITION`.trim(),
		foreignKeys: `
SELECT k.CONSTRAINT_NAME AS constraint_name,
       k.TABLE_NAME AS from_table,
       k.COLUMN_NAME AS from_column,
       k.REFERENCED_TABLE_SCHEMA AS to_schema,
       k.REFERENCED_TABLE_NAME AS to_table,
       k.REFERENCED_COLUMN_NAME AS to_column,
       k.ORDINAL_POSITION AS ordinal_position
FROM information_schema.KEY_COLUMN_USAGE k
WHERE k.TABLE_SCHEMA = ${schema}
  AND k.REFERENCED_TABLE_NAME IS NOT NULL
ORDER BY k.CONSTRAINT_NAME, k.ORDINAL_POSITION`.trim(),
		views: `
SELECT TABLE_NAME AS object_name,
       VIEW_DEFINITION AS definition
FROM information_schema.VIEWS
WHERE TABLE_SCHEMA = ${schema}`.trim()
	};
}

function postgresSql(schema: string): CatalogSql {
	return {
		objects: `
SELECT c.relname AS object_name,
       CASE WHEN c.relkind = 'r' THEN 'table' ELSE 'view' END AS object_kind,
       obj_description(c.oid, 'pg_class') AS comment
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = ${schema}
  AND c.relkind IN ('r', 'v', 'm')
ORDER BY c.relname`.trim(),
		columns: `
SELECT c.relname AS object_name,
       a.attname AS column_name,
       format_type(a.atttypid, a.atttypmod) AS column_type,
       CASE
         WHEN EXISTS (
           SELECT 1 FROM pg_index i
           WHERE i.indrelid = c.oid AND i.indisprimary AND a.attnum = ANY (i.indkey)
         ) THEN 'PRI'
         WHEN EXISTS (
           SELECT 1 FROM pg_index i
           WHERE i.indrelid = c.oid AND i.indisunique AND a.attnum = ANY (i.indkey)
         ) THEN 'UNI'
         ELSE ''
       END AS column_key,
       col_description(c.oid, a.attnum) AS comment,
       a.attnum AS ordinal_position
FROM pg_attribute a
JOIN pg_class c ON c.oid = a.attrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = ${schema}
  AND c.relkind IN ('r', 'v', 'm')
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY c.relname, a.attnum`.trim(),
		foreignKeys: `
SELECT con.conname AS constraint_name,
       src.relname AS from_table,
       sa.attname AS from_column,
       dst_ns.nspname AS to_schema,
       dst.relname AS to_table,
       da.attname AS to_column,
       cols.ordinality AS ordinal_position
FROM pg_constraint con
JOIN pg_class src ON src.oid = con.conrelid
JOIN pg_namespace n ON n.oid = src.relnamespace
JOIN pg_class dst ON dst.oid = con.confrelid
JOIN pg_namespace dst_ns ON dst_ns.oid = dst.relnamespace
JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY AS cols(src_att, dst_att, ordinality)
  ON true
JOIN pg_attribute sa ON sa.attrelid = src.oid AND sa.attnum = cols.src_att
JOIN pg_attribute da ON da.attrelid = dst.oid AND da.attnum = cols.dst_att
WHERE con.contype = 'f'
  AND n.nspname = ${schema}
ORDER BY con.conname, cols.ordinality`.trim(),
		views: `
SELECT c.relname AS object_name,
       pg_get_viewdef(c.oid, true) AS definition
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = ${schema}
  AND c.relkind IN ('v', 'm')`.trim()
	};
}

function mssqlSql(): CatalogSql {
	const userSchemas = `s.name NOT IN ('sys', 'INFORMATION_SCHEMA')`;
	return {
		objects: `
SELECT s.name + '.' + o.name AS object_name,
       CASE WHEN o.type = 'V' THEN 'view' ELSE 'table' END AS object_kind,
       CAST(ep.value AS nvarchar(4000)) AS comment
FROM sys.objects o
JOIN sys.schemas s ON s.schema_id = o.schema_id
LEFT JOIN sys.extended_properties ep
  ON ep.major_id = o.object_id AND ep.minor_id = 0 AND ep.name = N'MS_Description'
WHERE o.type IN ('U', 'V')
  AND ${userSchemas}
ORDER BY s.name, o.name`.trim(),
		columns: `
SELECT s.name + '.' + o.name AS object_name,
       c.name AS column_name,
       ty.name AS column_type,
       CASE
         WHEN pk.column_id IS NOT NULL THEN 'PRI'
         WHEN uq.column_id IS NOT NULL THEN 'UNI'
         ELSE ''
       END AS column_key,
       CAST(ep.value AS nvarchar(4000)) AS comment,
       c.column_id AS ordinal_position
FROM sys.columns c
JOIN sys.objects o ON o.object_id = c.object_id
JOIN sys.schemas s ON s.schema_id = o.schema_id
JOIN sys.types ty ON ty.user_type_id = c.user_type_id
LEFT JOIN sys.extended_properties ep
  ON ep.major_id = c.object_id AND ep.minor_id = c.column_id AND ep.name = N'MS_Description'
LEFT JOIN (
  SELECT ic.object_id, ic.column_id
  FROM sys.indexes i
  JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id
  WHERE i.is_primary_key = 1
) pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id
LEFT JOIN (
  SELECT ic.object_id, ic.column_id
  FROM sys.indexes i
  JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id
  WHERE i.is_unique = 1 AND i.is_primary_key = 0
) uq ON uq.object_id = c.object_id AND uq.column_id = c.column_id
WHERE o.type IN ('U', 'V')
  AND ${userSchemas}
ORDER BY s.name, o.name, c.column_id`.trim(),
		foreignKeys: `
SELECT fk.name AS constraint_name,
       SCHEMA_NAME(parent.schema_id) + '.' + parent.name AS from_table,
       pc.name AS from_column,
       SCHEMA_NAME(referenced.schema_id) + '.' + referenced.name AS to_table,
       rc.name AS to_column,
       fkc.constraint_column_id AS ordinal_position
FROM sys.foreign_keys fk
JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id
JOIN sys.tables parent ON parent.object_id = fk.parent_object_id
JOIN sys.columns pc ON pc.object_id = parent.object_id AND pc.column_id = fkc.parent_column_id
JOIN sys.tables referenced ON referenced.object_id = fk.referenced_object_id
JOIN sys.columns rc ON rc.object_id = referenced.object_id AND rc.column_id = fkc.referenced_column_id
ORDER BY fk.name, fkc.constraint_column_id`.trim(),
		views: `
SELECT s.name + '.' + v.name AS object_name,
       m.definition AS definition
FROM sys.views v
JOIN sys.schemas s ON s.schema_id = v.schema_id
JOIN sys.sql_modules m ON m.object_id = v.object_id
WHERE ${userSchemas}
ORDER BY s.name, v.name`.trim()
	};
}

function oracleSql(owner: string): CatalogSql {
	return {
		objects: `
SELECT table_name AS object_name,
       CASE WHEN table_type = 'VIEW' THEN 'view' ELSE 'table' END AS object_kind,
       comments AS comment
FROM all_tab_comments
WHERE owner = ${owner}
  AND table_type IN ('TABLE', 'VIEW')
ORDER BY table_name`.trim(),
		columns: `
SELECT c.table_name AS object_name,
       c.column_name AS column_name,
       c.data_type AS column_type,
       CASE
         WHEN pk.column_name IS NOT NULL THEN 'PRI'
         WHEN uk.column_name IS NOT NULL THEN 'UNI'
         ELSE ''
       END AS column_key,
       cc.comments AS comment,
       c.column_id AS ordinal_position
FROM all_tab_columns c
LEFT JOIN all_col_comments cc
  ON cc.owner = c.owner AND cc.table_name = c.table_name AND cc.column_name = c.column_name
LEFT JOIN (
  SELECT cols.table_name, cols.column_name
  FROM all_constraints cons
  JOIN all_cons_columns cols
    ON cols.owner = cons.owner AND cols.constraint_name = cons.constraint_name
  WHERE cons.owner = ${owner} AND cons.constraint_type = 'P'
) pk ON pk.table_name = c.table_name AND pk.column_name = c.column_name
LEFT JOIN (
  SELECT cols.table_name, cols.column_name
  FROM all_constraints cons
  JOIN all_cons_columns cols
    ON cols.owner = cons.owner AND cols.constraint_name = cons.constraint_name
  WHERE cons.owner = ${owner} AND cons.constraint_type = 'U'
) uk ON uk.table_name = c.table_name AND uk.column_name = c.column_name
WHERE c.owner = ${owner}
ORDER BY c.table_name, c.column_id`.trim(),
		foreignKeys: `
SELECT c.constraint_name AS constraint_name,
       c.table_name AS from_table,
       cc.column_name AS from_column,
       r.owner AS to_schema,
       r.table_name AS to_table,
       rc.column_name AS to_column,
       cc.position AS ordinal_position
FROM all_constraints c
JOIN all_cons_columns cc
  ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name
JOIN all_constraints r
  ON r.owner = c.r_owner AND r.constraint_name = c.r_constraint_name
JOIN all_cons_columns rc
  ON rc.owner = r.owner AND rc.constraint_name = r.constraint_name AND rc.position = cc.position
WHERE c.owner = ${owner}
  AND c.constraint_type = 'R'
ORDER BY c.constraint_name, cc.position`.trim(),
		views: `
SELECT view_name AS object_name,
       text AS definition
FROM all_views
WHERE owner = ${owner}`.trim()
	};
}
