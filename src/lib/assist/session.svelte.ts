import { createAnalysisCaller } from '$lib/analysis/llm';
import { toLlmSettings, type LlmProviderProfile } from '$lib/llm/providers';
import { loadAssistCatalog } from './catalog';
import type { AssistContext } from './context';
import { draftSql, DraftCancelled, DraftError, type DraftOutcome, type DraftPhase } from './draft';

export interface AssistFailure {
	stage: 'catalog' | 'model';
	message: string;
}

/** Drafting state for one query editor. */
export class AssistSession {
	phase = $state<DraftPhase | null>(null);
	outcome = $state<DraftOutcome | null>(null);
	failure = $state<AssistFailure | null>(null);
	question = $state('');
	/** Whether the current draft has been written into the editor. */
	applied = $state(false);
	private generation = 0;

	get busy(): boolean {
		return this.phase !== null;
	}

	async ask(input: {
		question: string;
		context: AssistContext;
		provider: LlmProviderProfile;
		locale: 'en' | 'zh';
		oracleVersion?: string | null;
		followUp?: string;
	}): Promise<DraftOutcome | null> {
		const generation = ++this.generation;
		this.question = input.question;
		this.outcome = null;
		this.failure = null;
		this.applied = false;
		this.phase = 'catalog';
		try {
			const outcome = await draftSql({
				question: input.question,
				context: input.context,
				caller: createAnalysisCaller(toLlmSettings(input.provider)),
				locale: input.locale,
				oracleVersion: input.oracleVersion,
				followUp: input.followUp,
				loadCatalog: (scope) => loadAssistCatalog(scope),
				onPhase: (phase) => {
					if (generation === this.generation) this.phase = phase;
				},
				isCancelled: () => generation !== this.generation
			});
			if (generation !== this.generation) return null;
			this.outcome = outcome;
			return outcome;
		} catch (error) {
			if (generation !== this.generation || error instanceof DraftCancelled) return null;
			this.failure =
				error instanceof DraftError
					? { stage: error.stage, message: error.message }
					: { stage: 'model', message: error instanceof Error ? error.message : String(error) };
			return null;
		} finally {
			if (generation === this.generation) this.phase = null;
		}
	}

	cancel() {
		this.generation += 1;
		this.phase = null;
	}

	dismiss() {
		this.cancel();
		this.outcome = null;
		this.failure = null;
		this.applied = false;
	}
}
