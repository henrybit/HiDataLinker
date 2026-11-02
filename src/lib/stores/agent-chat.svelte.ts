import type { AnalystEvent } from '$lib/builtin-agents/data-analyst/events';
import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
import type { AgentSessionMeta, TranscriptEntry } from '$lib/api/types';
import { selectedLlmProvider } from '$lib/llm/catalog.svelte';
import { toLlmSettings } from '$lib/llm/providers';
import { t } from '$lib/i18n/i18n.svelte';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export const DATA_ANALYST_ID = 'data-analyst';

export type ChatMessage = {
	id: string;
	at: string;
	role: 'user' | 'assistant' | 'system';
	kind: 'text' | 'markdown' | 'progress' | 'tool' | 'error';
	content: string;
	meta?: Record<string, unknown> | null;
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
	private liveProgressId: string | null = null;
	private activeRunId: string | null = null;
	private unsubs: UnlistenFn[] = [];
	private subscribed = false;

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
		this.clearLiveProgress();
		const detail = await api.agentLoadSession(DATA_ANALYST_ID, sessionId);
		this.session = detail.meta;
		this.messages = detail.transcript.map(toChatMessage);
		this.error = null;
		// Keep running if this session still has an in-flight run.
		if (this.activeRunId && this.session?.id === sessionId) {
			this.running = true;
		} else if (!this.activeRunId) {
			this.running = false;
		}
	}

	async newChat() {
		await this.ensureSubscribed();
		if (this.activeRunId) {
			try {
				await api.agentCancelRun(this.activeRunId);
			} catch {
				/* ignore */
			}
			this.activeRunId = null;
		}
		this.session = null;
		this.messages = [];
		this.error = null;
		this.draft = '';
		this.clearLiveProgress();
		this.running = false;
	}

	async submit(text?: string) {
		const query = (text ?? this.draft).trim();
		if (!query || this.running) return;

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
		this.running = true;

		try {
			if (!this.session) {
				this.session = await api.agentCreateSession(DATA_ANALYST_ID, {
					title: query.slice(0, 80)
				});
			}

			const userEntry = await this.persist({
				id: uid(),
				at: nowIso(),
				role: 'user',
				kind: 'text',
				content: query
			});
			this.messages = [...this.messages, toChatMessage(userEntry)];

			const systemMd = await api.agentReadSystem(DATA_ANALYST_ID);
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

			const { runId } = await api.agentStartRun({
				agentId: DATA_ANALYST_ID,
				sessionId: this.session.id,
				query,
				systemMd,
				llm: {
					provider: llm.provider,
					apiKey: llm.apiKey,
					model: llm.model,
					baseUrl: llm.baseUrl
				}
			});
			this.activeRunId = runId;
		} catch (error) {
			const message = errorMessage(error);
			this.error = message;
			this.clearLiveProgress();
			this.running = false;
			this.activeRunId = null;
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

	cancel() {
		const runId = this.activeRunId;
		if (runId) {
			void api.agentCancelRun(runId).catch(() => undefined);
		}
		this.activeRunId = null;
		this.running = false;
		this.clearLiveProgress();
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
		if (this.activeRunId && payload.runId !== this.activeRunId) return;
		if (this.session && payload.sessionId !== this.session.id) return;
		await this.handleEvent(payload.event);
	}

	private async onRunDone(payload: AgentRunDonePayload) {
		if (payload.agentId !== DATA_ANALYST_ID) return;
		if (this.activeRunId && payload.runId !== this.activeRunId) return;

		const forCurrentSession = !this.session || this.session.id === payload.sessionId;
		this.clearLiveProgress();
		this.activeRunId = null;
		this.running = false;

		if (!forCurrentSession) {
			await this.refreshHistory();
			return;
		}

		const error = payload.error?.trim() || '';
		const markdown = payload.markdown?.trim() || '';
		if (error && !markdown) {
			this.error = error;
			this.messages = [
				...this.messages,
				{
					id: uid(),
					at: nowIso(),
					role: 'assistant',
					kind: 'error',
					content: error
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
					content: markdown
				}
			];
		}
		await this.refreshHistory();
	}

	private async handleEvent(event: AnalystEvent) {
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
