import type { DraftOutcome } from './draft';

/** One earlier question in this editor and what the assistant answered. */
export interface AssistTurn {
	question: string;
	kind: 'draft' | 'clarify';
	/** The drafted SQL, or what was missing for a clarification. */
	answer: string;
}

const MAX_TURNS = 3;
const MAX_ANSWER_CHARS = 1500;

export function turnOf(question: string, outcome: DraftOutcome | null): AssistTurn | null {
	if (outcome?.kind === 'draft') return { question, kind: 'draft', answer: outcome.sql };
	if (outcome?.kind === 'clarify') {
		const answer = outcome.response.missing || outcome.response.summary;
		return { question, kind: 'clarify', answer };
	}
	return null;
}

/** The last few turns, oldest first, for the "Earlier in this session" part of the prompt. */
export function followUpText(turns: AssistTurn[]): string {
	return turns
		.slice(-MAX_TURNS)
		.map((turn) => {
			const answer =
				turn.answer.length > MAX_ANSWER_CHARS
					? `${turn.answer.slice(0, MAX_ANSWER_CHARS)} …`
					: turn.answer;
			const label = turn.kind === 'draft' ? 'You drafted' : 'You asked for clarification';
			return `Q: ${turn.question}\n${label}:\n${answer}`;
		})
		.join('\n\n');
}
