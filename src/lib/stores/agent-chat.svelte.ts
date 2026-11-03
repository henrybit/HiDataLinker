import type { AnalystEvent } from '$lib/builtin-agents/data-analyst/events';
import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
import type { AgentSessionMeta, TranscriptEntry } from '$lib/api/types';
import { selectedLlmProvider } from '$lib/llm/catalog.svelte';
import { estimateTokensFromText } from '$lib/llm/context-window';
import { toLlmSettings } from '$lib/llm/providers';
import { getLocale, t } from '$lib/i18n/i18n.svelte';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export const DATA_ANALYST_ID = 'data-analyst';

/** Rough allowance for system prompt + schema digest in each run. */
const CONTEXT_OVERHEAD_TOKENS = 2_500;

export type ChatMessage = {
	id: string;
	at: string;
	role: 'user' | 'assistant' | 'system';
	kind: 'text' | 'markdown' | 'progress' | 'tool' | 'error';
	content: string;
	meta?: Record<string, unknown> | null;
};

type QueuedPrompt = {
	id: string;
	query: string;
	messageId: string;
	connectionIds: string[];
	skillIds: string[];
	sessionId: string;
};

type AgentRunEventPayload = {
	runId: string;
	agentId: string;
	sessionId: string;
	event: AnalystEvent;
};

type AgentRunDonePayload = {
	runId: string;
	agentId: string;
	sessionId: string;
	markdown: string;
	error?: string | null;
	status?: string;
	durationMs?: number | null;
};

function uid(): string {
	return crypto.randomUUID();
}

function nowIso(): string {
	return new Date().toISOString();
}

function toChatMessage(entry: TranscriptEntry): ChatMessage {
	return {
		id: entry.id || uid(),
		at: entry.at || nowIso(),
		role: (entry.role as ChatMessage['role']) || 'assistant',
		kind: (entry.kind as ChatMessage['kind']) || 'text',
		content: entry.content,
		meta: entry.meta ?? null
	};
}

class AgentChatStore {
	session = $state<AgentSessionMeta | null>(null);
	messages = $state<ChatMessage[]>([]);
	history = $state<AgentSessionMeta[]>([]);
	running = $state(false);
	error = $state<string | null>(null);
	draft = $state('');
	/** Bumped to clear the composer local draft after programmatic submits. */
	composerClearSignal = $state(0);
	/** Pending user prompts waiting for the active run to finish. */
	queue = $state<QueuedPrompt[]>([]);
	/** Session id that currently owns `activeRunId` (may differ from viewed session). */
	activeRunSessionId = $state<string | null>(null);
	/** Manual or auto context compression in progress. */
	compressing = $state(false);
	/** Last compression notice shown in the session header area. */
	contextNotice = $state<string | null>(null);
	/** Last prompt context size reported by the runtime (token estimate from chars). */
	lastPromptContextTokens = $state(0);
	private liveProgressId: string | null = null;
	private activeRunId: string | null = null;
	private draining = false;
	private unsubs: UnlistenFn[] = [];
	private subscribed = false;

	get queueLength() {
		return this.queue.length;
	}

	get viewedSessionId() {
		return this.session?.id ?? null;
	}

	isSessionRunning(sessionId: string | null | undefined) {
		return !!sessionId && this.activeRunSessionId === sessionId && !!this.activeRunId;
	}

	/**
	 * Approximate tokens that will feed the next turn: transcript estimate, plus
	 * overhead, preferring the last runtime-reported prompt size when larger.
	 */
	estimateContextTokens(): number {
		let fromMessages = CONTEXT_OVERHEAD_TOKENS;
		for (const message of this.messages) {
			fromMessages += estimateTokensFromText(message.content);
			if (message.meta && Object.keys(message.meta).length > 0) {
				try {
					fromMessages += estimateTokensFromText(JSON.stringify(message.meta));
				} catch {
					/* ignore */
				}
			}
		}
		return Math.max(fromMessages, this.lastPromptContextTokens);
	}

	async ensureSubscribed() {
		if (!isTauriRuntime() || this.subscribed) return;
		this.subscribed = true;
		this.unsubs.push(
			await listen<AgentRunEventPayload>('agent-run-event', (event) => {
				void this.onRunEvent(event.payload);
			})
		);
		this.unsubs.push(
			await listen<AgentRunDonePayload>('agent-run-done', (event) => {
				void this.onRunDone(event.payload);
			})
		);
	}

	dispose() {
		for (const stop of this.unsubs) stop();
		this.unsubs = [];
		this.subscribed = false;
	}

	async refreshHistory() {
		if (!isTauriRuntime()) {
			this.history = [];
			return;
		}
		try {
			this.history = await api.agentListSessions(DATA_ANALYST_ID);
		} catch (error) {
			console.error('[agent-chat] history', error);
		}
	}

	async openSession(sessionId: string) {
		if (!isTauriRuntime()) return;
		await this.ensureSubscribed();
		await this.loadSession(sessionId, { clearQueue: true });
	}

	/** Switch without cancelling a background run on another session. */
	async switchSession(sessionId: string) {
		if (!isTauriRuntime()) return;
		if (this.session?.id === sessionId) return;
		await this.ensureSubscribed();
		await this.loadSession(sessionId, { clearQueue: false });
	}

	async newChat() {
		await this.ensureSubscribed();
		// Keep background runs; only reset the viewed conversation.
		this.clearLiveProgress();
		this.session = null;
		this.messages = [];
		this.error = null;
		this.draft = '';
		this.composerClearSignal += 1;
		this.running = false;
		this.contextNotice = null;
		this.lastPromptContextTokens = 0;
		await this.refreshHistory();
	}

	async compressContext() {
		if (!isTauriRuntime() || !this.session || this.compressing || this.running) return;
		const provider = selectedLlmProvider();
		if (!provider) {
			this.error = t('agentMode.live.needLlm');
			return;
		}
		const llm = toLlmSettings(provider);
		if (!llm.apiKey?.trim()) {
			this.error = t('agentMode.live.needLlm');
			return;
		}
		this.compressing = true;
		this.error = null;
		try {
			const locale = getLocale() === 'zh' ? 'zh' : 'en';
			const result = await api.agentCompressContext({
				agentId: DATA_ANALYST_ID,
				sessionId: this.session.id,
				locale,
				llm: {
					provider: llm.provider,
					apiKey: llm.apiKey,
					model: llm.model,
					baseUrl: llm.baseUrl
				}
			});
			this.contextNotice = result.compressed
				? t('agentMode.live.contextCompressed', {
						count: result.messagesCompressed
					})
				: t('agentMode.live.contextNothingToCompress');
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.compressing = false;
		}
	}

	private async loadSession(
		sessionId: string,
		options: { clearQueue: boolean }
	) {
		if (options.clearQueue) this.clearQueue();
		this.clearLiveProgress();
		const detail = await api.agentLoadSession(DATA_ANALYST_ID, sessionId);
		this.session = detail.meta;
		this.messages = detail.transcript.map(toChatMessage);
		this.error = null;
		this.contextNotice = null;
		this.lastPromptContextTokens = 0;
		this.running = this.isSessionRunning(sessionId);
		await this.refreshHistory();
	}

	async submit(text?: string, connectionIds: string[] = [], skillIds: string[] = []) {
		const query = (text ?? this.draft).trim();
		if (!query) return;
		const scopedIds = connectionIds.map((id) => id.trim()).filter(Boolean);
		const scopedSkills = skillIds.map((id) => id.trim()).filter(Boolean);

		if (!isTauriRuntime()) {
			this.error = t('agentMode.live.needDesktop');
			return;
		}

		await this.ensureSubscribed();

		const profile = selectedLlmProvider();
		if (!profile) {
			this.error = t('agentMode.live.needLlm');
			return;
		}
		if (!profile.apiKey.trim() && !profile.baseUrl.trim()) {
			this.error = t('agentMode.live.needLlm');
			return;
		}

		this.error = null;
		this.draft = '';
		this.composerClearSignal += 1;

		const viewingId = this.session?.id ?? null;
		const busyElsewhere =
			!!this.activeRunId &&
			!!this.activeRunSessionId &&
			this.activeRunSessionId !== viewingId;
		const busyHere = this.isSessionRunning(viewingId) || (this.running && !!viewingId);

		if (busyHere || busyElsewhere) {
			void this.enqueue(query, scopedIds, scopedSkills);
			return;
		}

		// Fire-and-forget: run lifecycle continues via agent-run-* events.
		void this.beginRun(query, {
			addUserMessage: true,
			connectionIds: scopedIds,
			skillIds: scopedSkills
		});
	}

	cancel() {
		const runId = this.activeRunId;
		this.clearQueue();
		if (runId) {
			void api.agentCancelRun(runId).catch(() => undefined);
		}
		this.activeRunId = null;
		this.activeRunSessionId = null;
		this.running = false;
		this.clearLiveProgress();
	}

	private clearQueue() {
		if (this.queue.length === 0) return;
		const queuedIds = new Set(this.queue.map((item) => item.messageId));
		this.queue = [];
		this.messages = this.messages.filter((item) => !queuedIds.has(item.id));
	}

	private async enqueue(
		query: string,
		connectionIds: string[] = [],
		skillIds: string[] = []
	) {
		if (!this.session) {
			this.session = await api.agentCreateSession(DATA_ANALYST_ID, {
				title: query.slice(0, 80)
			});
			await this.refreshHistory();
		}
		const sessionId = this.session.id;
		const messageId = uid();
		const entry = await this.persist({
			id: messageId,
			at: nowIso(),
			role: 'user',
			kind: 'text',
			content: query,
			meta: {
				queued: true,
				...(connectionIds.length > 0 ? { connectionIds } : {}),
				...(skillIds.length > 0 ? { skillIds } : {})
			}
		});
		this.messages = [...this.messages, toChatMessage(entry)];
		this.queue = [
			...this.queue,
			{
				id: uid(),
				query,
				messageId: entry.id || messageId,
				connectionIds,
				skillIds,
				sessionId
			}
		];
	}

	private markDequeued(messageId: string) {
		this.messages = this.messages.map((item) =>
			item.id === messageId
				? { ...item, meta: { ...(item.meta ?? {}), queued: false } }
				: item
		);
	}

	private async beginRun(
		query: string,
		options: {
			addUserMessage: boolean;
			messageId?: string;
			connectionIds?: string[];
			skillIds?: string[];
		}
	) {
		this.running = true;
		const connectionIds = (options.connectionIds ?? [])
			.map((id) => id.trim())
			.filter(Boolean);
		const skillIds = (options.skillIds ?? []).map((id) => id.trim()).filter(Boolean);
		try {
			// Connection warm-up / schema refresh runs in Rust after start_run returns.
			// Do not await FE connect loops here — they re-boot the workspace and stall typing.

			if (!this.session) {
				this.session = await api.agentCreateSession(DATA_ANALYST_ID, {
					title: query.slice(0, 80)
				});
			}

			if (options.addUserMessage) {
				const meta: Record<string, unknown> = {};
				if (connectionIds.length > 0) meta.connectionIds = connectionIds;
				if (skillIds.length > 0) meta.skillIds = skillIds;
				const userEntry = await this.persist({
					id: uid(),
					at: nowIso(),
					role: 'user',
					kind: 'text',
					content: query,
					meta: Object.keys(meta).length > 0 ? meta : null
				});
				this.messages = [...this.messages, toChatMessage(userEntry)];
			} else if (options.messageId) {
				this.markDequeued(options.messageId);
			}

			const systemMd = await api.agentReadSystem(DATA_ANALYST_ID);
			const profile = selectedLlmProvider();
			if (!profile) {
				throw new Error(t('agentMode.live.needLlm'));
			}
			const llm = toLlmSettings(profile);
			this.liveProgressId = uid();
			this.messages = [
				...this.messages,
				{
					id: this.liveProgressId,
					at: nowIso(),
					role: 'assistant',
					kind: 'progress',
					content: t('agentMode.live.starting'),
					meta: { phase: 'start' }
				}
			];

			// Returns immediately; schema warm + Node graph start continue in the background.
			const { runId } = await api.agentStartRun({
				agentId: DATA_ANALYST_ID,
				sessionId: this.session.id,
				query,
				systemMd,
				locale: getLocale(),
				connectionIds,
				skillIds,
				llm: {
					provider: llm.provider,
					apiKey: llm.apiKey,
					model: llm.model,
					baseUrl: llm.baseUrl
				}
			});
			this.activeRunId = runId;
			this.activeRunSessionId = this.session.id;
			this.running = this.isSessionRunning(this.session.id);
			await this.refreshHistory();
		} catch (error) {
			const message = errorMessage(error);
			this.error = message;
			this.clearLiveProgress();
			this.running = false;
			this.activeRunId = null;
			this.activeRunSessionId = null;
			const entry: ChatMessage = {
				id: uid(),
				at: nowIso(),
				role: 'assistant',
				kind: 'error',
				content: message
			};
			this.messages = [...this.messages, entry];
			if (this.session) {
				try {
					await this.persist({
						id: entry.id,
						at: entry.at,
						role: 'assistant',
						kind: 'error',
						content: message
					});
				} catch {
					/* ignore */
				}
			}
		}
	}

	private async drainQueue() {
		if (this.draining || this.activeRunId) return;
		const [next, ...rest] = this.queue;
		if (!next) return;
		this.draining = true;
		this.queue = rest;
		try {
			if (this.session?.id !== next.sessionId) {
				await this.loadSession(next.sessionId, { clearQueue: false });
			}
			await this.beginRun(next.query, {
				addUserMessage: false,
				messageId: next.messageId,
				connectionIds: next.connectionIds,
				skillIds: next.skillIds
			});
		} finally {
			this.draining = false;
			// beginRun failure clears activeRunId — keep draining remaining items.
			if (!this.activeRunId && this.queue.length > 0) {
				void this.drainQueue();
			}
		}
	}

	private clearLiveProgress() {
		if (this.liveProgressId) {
			this.messages = this.messages.filter((item) => item.id !== this.liveProgressId);
			this.liveProgressId = null;
		}
	}

	private async persist(entry: TranscriptEntry): Promise<TranscriptEntry> {
		if (!this.session) return entry;
		return api.agentAppendTranscript(DATA_ANALYST_ID, this.session.id, entry);
	}

	private async onRunEvent(payload: AgentRunEventPayload) {
		if (payload.agentId !== DATA_ANALYST_ID) return;
		if (!this.activeRunId || payload.runId !== this.activeRunId) return;
		if (this.session && payload.sessionId !== this.session.id) return;
		await this.handleEvent(payload.event);
	}

	private async onRunDone(payload: AgentRunDonePayload) {
		if (payload.agentId !== DATA_ANALYST_ID) return;
		// Ignore stale/cancelled runs that are no longer the active one.
		if (!this.activeRunId || payload.runId !== this.activeRunId) return;

		const forCurrentSession = !this.session || this.session.id === payload.sessionId;
		if (forCurrentSession) this.clearLiveProgress();
		this.activeRunId = null;
		this.activeRunSessionId = null;
		this.running = false;

		if (!forCurrentSession) {
			await this.refreshHistory();
			void this.drainQueue();
			return;
		}

		const error = payload.error?.trim() || '';
		const markdown = payload.markdown?.trim() || '';
		const durationMs =
			typeof payload.durationMs === 'number' && Number.isFinite(payload.durationMs)
				? payload.durationMs
				: null;
		const doneMeta = durationMs != null ? { durationMs } : null;
		if (error && !markdown) {
			this.error = error;
			this.messages = [
				...this.messages,
				{
					id: uid(),
					at: nowIso(),
					role: 'assistant',
					kind: 'error',
					content: error,
					meta: doneMeta
				}
			];
		} else if (markdown) {
			this.messages = [
				...this.messages,
				{
					id: uid(),
					at: nowIso(),
					role: 'assistant',
					kind: 'markdown',
					content: markdown,
					meta: doneMeta
				}
			];
		}
		await this.refreshHistory();
		void this.drainQueue();
	}

	private async handleEvent(event: AnalystEvent) {
		if (event.type === 'context') {
			// Runtime reports character counts for the assembled prompt block.
			this.lastPromptContextTokens = Math.max(
				1,
				Math.ceil(Math.max(0, event.totalChars) / 2.5) + CONTEXT_OVERHEAD_TOKENS
			);
			if (event.compressed) {
				this.contextNotice = t('agentMode.live.contextAutoCompressed', {
					summary: event.summaryChars,
					recent: event.recentChars
				});
			}
			return;
		}
		if (event.type === 'status' && this.liveProgressId) {
			this.messages = this.messages.map((item) =>
				item.id === this.liveProgressId
					? { ...item, content: event.message, meta: { phase: 'status' } }
					: item
			);
			return;
		}
		if (event.type === 'plan') {
			const content = event.steps.map((step, i) => `${i + 1}. ${step.title}`).join('\n');
			this.messages = [
				...this.messages,
				{
					id: uid(),
					at: nowIso(),
					role: 'assistant',
					kind: 'progress',
					content: `${t('agentMode.live.planTitle')}\n${content}`,
					meta: { phase: 'plan', steps: event.steps }
				}
			];
			return;
		}
		if (event.type === 'step-start') {
			this.messages = [
				...this.messages,
				{
					id: uid(),
					at: nowIso(),
					role: 'assistant',
					kind: 'progress',
					content: t('agentMode.live.stepProgress', {
						current: event.index + 1,
						total: event.total,
						title: event.title
					}),
					meta: { phase: 'step', index: event.index, total: event.total }
				}
			];
			return;
		}
		if (event.type === 'tool') {
			const sql = typeof event.args.sql === 'string' ? event.args.sql : '';
			this.messages = [
				...this.messages,
				{
					id: uid(),
					at: nowIso(),
					role: 'assistant',
					kind: 'tool',
					content: sql || event.summary,
					meta: {
						tool: event.tool,
						ok: event.ok,
						durationMs: event.durationMs,
						args: event.args,
						summary: event.summary
					}
				}
			];
			return;
		}
		if (event.type === 'step-done') {
			this.messages = [
				...this.messages,
				{
					id: uid(),
					at: nowIso(),
					role: 'assistant',
					kind: 'progress',
					content: event.note,
					meta: { phase: 'step-done', index: event.index }
				}
			];
			return;
		}
		if (event.type === 'error') {
			this.error = event.message;
		}
	}
}

export const agentChat = new AgentChatStore();
