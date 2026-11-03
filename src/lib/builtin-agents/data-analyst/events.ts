export type AnalystEvent =
	| { type: 'status'; message: string }
	| {
			type: 'context';
			compressed: boolean;
			summaryChars: number;
			recentChars: number;
			totalChars: number;
	  }
	| { type: 'plan'; steps: AnalystPlanStep[] }
	| {
			type: 'step-start';
			index: number;
			total: number;
			title: string;
	  }
	| {
			type: 'tool';
			index: number;
			tool: string;
			args: Record<string, unknown>;
			ok: boolean;
			summary: string;
			durationMs: number;
	  }
	| { type: 'step-done'; index: number; note: string }
	| { type: 'final'; markdown: string }
	| { type: 'error'; message: string };

export interface AnalystPlanStep {
	title: string;
	goal: string;
	connectionHint?: string;
	tablesHint?: string[];
	toolHint?: string;
	sqlIntent?: string;
}

export interface AnalystObservation {
	stepIndex: number;
	tool: string;
	args: Record<string, unknown>;
	ok: boolean;
	summary: string;
}
