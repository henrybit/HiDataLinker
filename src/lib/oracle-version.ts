/** Release ids match the backend protocol map in `db/oracle`. */
export const ORACLE_VERSIONS = [
	{ id: '23', label: '23ai' },
	{ id: '21', label: '21c' },
	{ id: '19', label: '19c' },
	{ id: '18', label: '18c' },
	{ id: '12.2', label: '12c R2' },
	{ id: '12.1', label: '12c R1' },
	{ id: '11.2', label: '11g R2' },
	{ id: '11.1', label: '11g R1' },
	{ id: '10.2', label: '10g R2' },
	{ id: '10.1', label: '10g R1' }
] as const;

export type OracleVersionId = (typeof ORACLE_VERSIONS)[number]['id'] | 'auto';

/** 11g and 10g connect through ODPI-C and need an Instant Client directory. */
export function needsOracleInstantClient(value: string | null | undefined): boolean {
	const id = normalizeOracleVersion(value);
	return id === '11.2' || id === '11.1' || id === '10.2' || id === '10.1';
}

export function normalizeOracleVersion(value: string | null | undefined): OracleVersionId {
	const trimmed = value?.trim() ?? '';
	if (!trimmed || trimmed.toLowerCase() === 'auto') return 'auto';
	const known = ORACLE_VERSIONS.find(
		(item) => item.id === trimmed || item.label.toLowerCase() === trimmed.toLowerCase()
	);
	return known?.id ?? 'auto';
}
