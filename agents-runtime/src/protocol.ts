export type Envelope = {
	id: string;
	type: string;
	payload?: unknown;
};

export type RunStartPayload = {
	runId: string;
	agentId: string;
	sessionId: string;
	query: string;
	systemMd: string;
	llm: {
		provider: 'openai' | 'anthropic';
		model: string;
		baseUrl: string;
	};
};

export type ToolInvokePayload = {
	runId: string;
	name: string;
	args: Record<string, unknown>;
};

export type LlmCompletePayload = {
	runId: string;
	provider: string;
	model: string;
	baseUrl: string;
	system: string;
	human: string;
	schema: Record<string, unknown>;
};

export type ToolResultPayload = {
	ok: boolean;
	result?: unknown;
	error?: string | null;
};

export type LlmResultPayload = {
	ok: boolean;
	result?: unknown;
	error?: string | null;
};
