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
	/** App UI locale: drives LLM output language. */
	locale?: 'zh' | 'en';
	/** Prebuilt schema digest from local ~/.hidatalinker/db cache. */
	schemaDigest?: string;
	/** Optional @-mentioned connection ids; empty/omitted = all sources. */
	connectionIds?: string[];
	/** Optional /-selected skill ids; empty/omitted = full tool set. */
	skillIds?: string[];
	/** Authorization mode for SQL tools: readonly (default) or full. */
	authMode?: 'readonly' | 'full';
	/** Compressed prior conversation (summary + recent turns). */
	conversationContext?: string;
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
