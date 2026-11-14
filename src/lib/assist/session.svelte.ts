import { createAnalysisCaller } from '$lib/analysis/llm';
import { toLlmSettings, type LlmProviderProfile } from '$lib/llm/providers';
import { loadAssistCatalog } from './catalog';
import type { AssistContext } from './context';
import {
	draftSql,
	DraftCancelled,
	DraftError,
	type DraftOutcome,
	type DraftPhase,
	type DraftStep
} from './draft';

export interface AssistFailure {
	stage: 'catalog' | 'model';
	message: string;
}

/** Drafting state for one query editor. */
export class AssistSession {
	phase = $state<DraftPhase | null>(null);
	outcome = $state<DraftOutcome | null>(null);
	failure = $state<AssistFailure | null>(null);
	steps = $state<DraftStep[]>([]);
	/** The last request was cancelled before it produced anything. */
	cancelled = $state(false);
	question = $state('');
	/** Tables pinned for the last request, so a retry asks the same thing. */
	pinned = $state<string[]>([]);
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
		pinned?: string[];
	}): Promise<DraftOutcome | null> {
		const generation = ++this.generation;
		this.question = input.question;
		this.pinned = input.pinned ?? [];
		this.outcome = null;
		this.failure = null;
		this.steps = [];
		this.cancelled = false;
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
				pinned: input.pinned,
				loadCatalog: (scope) => loadAssistCatalog(scope),
				onPhase: (phase) => {
					if (generation === this.generation) this.phase = phase;
				},
				onStep: (step) => {
					if (generation === this.generation) this.steps = [...this.steps, step];
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
		if (this.phase === null) return;
		this.generation += 1;
		this.phase = null;
		this.cancelled = true;
	}

	dismiss() {
		this.generation += 1;
		this.phase = null;
		this.outcome = null;
		this.failure = null;
		this.steps = [];
		this.cancelled = false;
		this.applied = false;
	}
}
