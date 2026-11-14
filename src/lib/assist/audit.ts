import { isMssql } from '$lib/engine';
import { maskSecrets } from './context';
import { isReadOnlySql } from './statement';

/** A draft the user wrote into an editor. */
export interface AppliedDraft {
	id: string;
	sql: string;
}

export function newDraftId(): string {
	return crypto.randomUUID().replace(/-/g, '').slice(0, 8);
}

/** Audit detail for a draft: its id plus a masked, one-line excerpt of the question. */
export function draftDetail(id: string, question: string): string {
	const excerpt = maskSecrets(question).replace(/\s+/g, ' ').trim().slice(0, 120);
	return excerpt ? `${id} · ${excerpt}` : id;
}

function squash(sql: string): string {
	return sql.replace(/\s+/g, ' ').replace(/;\s*$/, '').trim().toLowerCase();
}

/** The most recent applied draft still present in `sql`, if any. */
export function findAppliedDraft(sql: string, drafts: AppliedDraft[]): AppliedDraft | null {
	const text = squash(sql);
	if (!text) return null;
	for (let i = drafts.length - 1; i >= 0; i -= 1) {
		const draft = squash(drafts[i].sql);
		if (draft && (text.includes(draft) || draft.includes(text))) return drafts[i];
	}
	return null;
}

/** Audit detail for an editor run that contains a draft. */
export function runDetail(draft: AppliedDraft | null): string | null {
	return draft ? `assist:${draft.id}` : null;
}

/**
 * Whether running drafted SQL needs the stronger confirmation: writes and DDL always, and on SQL
 * Server also Explain, because its plan comes from executing the statement.
 */
export function needsDraftConfirm(
	sql: string,
	engine: string | null | undefined,
	mode: 'run' | 'explain',
	draft: AppliedDraft | null
): boolean {
	if (!draft || isReadOnlySql(sql)) return false;
	return mode === 'run' || isMssql(engine);
}
