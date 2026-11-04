import type { AnalystEvent } from '$lib/builtin-agents/data-analyst/events';
import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
import type {
	AgentSessionMeta,
	AttachmentMeta,
	ComposerAttachment,
	ImportJobProgress,
	ParseSample,
	TranscriptEntry
} from '$lib/api/types';
import { selectedLlmProvider } from '$lib/llm/catalog.svelte';
import { estimateTokensFromText } from '$lib/llm/context-window';
import { toLlmSettings } from '$lib/llm/providers';
import { getLocale, t } from '$lib/i18n/i18n.svelte';
import { appMode } from '$lib/stores/app-mode.svelte';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export const DATA_ANALYST_ID = 'data-analyst';

/** Rough allowance for system prompt + schema digest in each run. */
const CONTEXT_OVERHEAD_TOKENS = 2_500;

export type ChatMessage = {
	id: string;
	at: string;
	role: 'user' | 'assistant' | 'system';
	kind: 'text' | 'markdown' | 'progress' | 'tool' | 'error' | 'import';
	content: string;
	meta?: Record<string, unknown> | null;
};

type StagedAttachment = AttachmentMeta & { sample?: ParseSample | null };

type QueuedPrompt = {
	id: string;
	query: string;
	messageId: string;
	connectionIds: string[];
	skillIds: string[];
	attachments: ComposerAttachment[];
	sessionId: string;
};

/** Deferred agent start while the user picks import targets. */
type PendingImportRun = {
	query: string;
	connectionIds: string[];
	skillIds: string[];
	staged: StagedAttachment[];
	sessionId: string;
	importMessageIds: string[];
};

export type ImportDecisionTarget = {
	messageId: string;
	connectionId: string;
	schema: string;
	table: string;
	mode: 'create' | 'append';
	/** True when the user confirmed an import job for this target. */
	imported?: boolean;
	/** Native import job id when waiting for import-done before starting the agent. */
	jobId?: string;
	/** Optional import outcome once the native job finishes. */
	importResult?: {
		ok: boolean;
		writtenRows?: number;
		totalRows?: number;
		error?: string;
	};
};

type PendingPostImportRun = {
	query: string;
	connectionIds: string[];
	skillIds: string[];
	staged: StagedAttachment[];
	sessionId: string;
	target: ImportDecisionTarget;
	jobId: string;
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
	/** True while import plan card(s) are waiting for the user before the agent starts. */
	awaitingImportDecision = $state(false);
	/** True while a confirmed native import job must finish before the agent starts. */
	awaitingImportJob = $state(false);
	private liveProgressId: string | null = null;
	private activeRunId: string | null = null;
	private pendingImportRun: PendingImportRun | null = null;
	private pendingPostImportRun: PendingPostImportRun | null = null;
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

	isAwaitingImportDecision(sessionId: string | null | undefined = this.session?.id) {
		return (
			((this.awaitingImportDecision && !!this.pendingImportRun) ||
				(this.awaitingImportJob && !!this.pendingPostImportRun)) &&
			!!sessionId &&
			(this.pendingImportRun?.sessionId === sessionId ||
				this.pendingPostImportRun?.sessionId === sessionId)
		);
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
		this.unsubs.push(
			await listen<ImportJobProgress>('import-progress', (event) => {
				this.onImportProgress(event.payload);
			})
		);
		this.unsubs.push(
			await listen<{ jobId: string; writtenRows: number; totalRows: number; connectionId?: string; schema?: string; table?: string }>(
				'import-done',
				(event) => {
					this.onImportDone(event.payload);
				}
			)
		);
		this.unsubs.push(
			await listen<{ jobId: string; error: string }>('import-error', (event) => {
				this.onImportError(event.payload);
			})
		);
	}

	private findImportMessageId(jobId: string): string | null {
		const hit = [...this.messages]
			.reverse()
			.find((item) => item.kind === 'import' && item.meta?.jobId === jobId);
		return hit?.id ?? null;
	}

	private onImportProgress(payload: ImportJobProgress) {
		const id = this.findImportMessageId(payload.jobId);
		if (!id) return;
		this.updateImportMessage(id, {
			phase: payload.phase,
			writtenRows: payload.writtenRows,
			totalRows: payload.totalRows,
			progressMessage: payload.message,
			importing: payload.phase !== 'done' && payload.phase !== 'cancelled'
		});
	}

	private onImportDone(payload: {
		jobId: string;
		writtenRows: number;
		totalRows: number;
		connectionId?: string;
		schema?: string;
		table?: string;
	}) {
		const id = this.findImportMessageId(payload.jobId);
		if (id) {
			this.updateImportMessage(id, {
				phase: 'done',
				writtenRows: payload.writtenRows,
				totalRows: payload.totalRows,
				importing: false,
				progressMessage: t('agentMode.live.importDone', {
					rows: payload.writtenRows,
					table: payload.table ?? ''
				})
			});
		}
		void this.finishPostImportRun(payload.jobId, {
			ok: true,
			writtenRows: payload.writtenRows,
			totalRows: payload.totalRows
		});
	}

	private onImportError(payload: { jobId: string; error: string }) {
		const id = this.findImportMessageId(payload.jobId);
		if (id) {
			this.updateImportMessage(id, {
				phase: 'error',
				importing: false,
				progressMessage: payload.error,
				error: payload.error
			});
		}
		void this.finishPostImportRun(payload.jobId, {
			ok: false,
			error: payload.error
		});
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
		this.clearPendingImportDecision();
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
		// Drop deferred start when leaving the session that owns it.
		if (
			(this.pendingImportRun && this.pendingImportRun.sessionId !== sessionId) ||
			(this.pendingPostImportRun && this.pendingPostImportRun.sessionId !== sessionId)
		) {
			this.clearPendingImportDecision();
		}
		this.clearLiveProgress();
		const detail = await api.agentLoadSession(DATA_ANALYST_ID, sessionId);
		this.session = detail.meta;
		this.messages = detail.transcript.map(toChatMessage);
		this.error = null;
		this.contextNotice = null;
		this.lastPromptContextTokens = 0;
		this.running =
			this.isSessionRunning(sessionId) || this.isAwaitingImportDecision(sessionId);
		await this.refreshHistory();
	}

	async submit(
		text?: string,
		connectionIds: string[] = [],
		skillIds: string[] = [],
		attachments: ComposerAttachment[] = []
	) {
		const query = (text ?? this.draft).trim();
		if (!query && attachments.length === 0) return;
		const scopedIds = connectionIds.map((id) => id.trim()).filter(Boolean);
		const scopedSkills = skillIds.map((id) => id.trim()).filter(Boolean);
		const files = attachments.slice();

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
			(!!this.activeRunId &&
				!!this.activeRunSessionId &&
				this.activeRunSessionId !== viewingId) ||
			(!!this.pendingImportRun &&
				!!viewingId &&
				this.pendingImportRun.sessionId !== viewingId);
		const busyHere =
			this.isSessionRunning(viewingId) ||
			this.isAwaitingImportDecision(viewingId) ||
			(this.running && !!viewingId);

		const finalQuery =
			query ||
			(files.length > 0
				? t('agentMode.composer.attachDefaultQuery')
				: '');

		if (busyHere || busyElsewhere) {
			void this.enqueue(finalQuery, scopedIds, scopedSkills, files);
			return;
		}

		// Fire-and-forget: run lifecycle continues via agent-run-* events.
		void this.beginRun(finalQuery, {
			addUserMessage: true,
			connectionIds: scopedIds,
			skillIds: scopedSkills,
			attachments: files
		});
	}

	cancel() {
		const runId = this.activeRunId;
		this.clearQueue();
		this.clearPendingImportDecision();
		if (runId) {
			void api.agentCancelRun(runId).catch(() => undefined);
		}
		this.activeRunId = null;
		this.activeRunSessionId = null;
		this.running = false;
		this.clearLiveProgress();
	}

	/**
	 * Called from the import plan card after the user picks a target
	 * (and optionally starts import). Releases the gate and starts the agent.
	 * When `imported` + `jobId` are set, waits for native import-done/error first.
	 */
	async resumeAfterImportDecision(target: ImportDecisionTarget) {
		const pending = this.pendingImportRun;
		if (!pending) return;
		if (this.session?.id && this.session.id !== pending.sessionId) return;

		this.updateImportMessage(target.messageId, {
			awaitingDecision: false,
			decision: {
				connectionId: target.connectionId,
				schema: target.schema,
				table: target.table,
				mode: target.mode,
				imported: target.imported === true,
				jobId: target.jobId ?? null
			}
		});
		for (const id of pending.importMessageIds) {
			if (id === target.messageId) continue;
			this.updateImportMessage(id, { awaitingDecision: false });
		}

		this.pendingImportRun = null;
		this.awaitingImportDecision = false;

		if (target.imported && target.jobId) {
			this.pendingPostImportRun = {
				query: pending.query,
				connectionIds: pending.connectionIds,
				skillIds: pending.skillIds,
				staged: pending.staged,
				sessionId: pending.sessionId,
				target,
				jobId: target.jobId
			};
			this.awaitingImportJob = true;
			this.running = true;
			this.clearLiveProgress();
			this.liveProgressId = uid();
			this.messages = [
				...this.messages,
				{
					id: this.liveProgressId,
					at: nowIso(),
					role: 'assistant',
					kind: 'progress',
					content: t('agentMode.live.awaitingImportJob'),
					meta: { phase: 'awaiting-import-job', jobId: target.jobId }
				}
			];
			return;
		}

		await this.startAgentAfterImportDecision(pending, target);
	}

	private async startAgentAfterImportDecision(
		pending: Pick<PendingImportRun, 'query' | 'connectionIds' | 'skillIds' | 'staged'>,
		target: ImportDecisionTarget
	) {
		try {
			await this.launchAgentRun({
				query: pending.query,
				connectionIds: pending.connectionIds,
				skillIds: pending.skillIds,
				staged: pending.staged,
				importTarget: target
			});
		} catch (error) {
			const message = errorMessage(error);
			this.error = message;
			this.clearLiveProgress();
			this.running = false;
			this.activeRunId = null;
			this.activeRunSessionId = null;
			this.messages = [
				...this.messages,
				{
					id: uid(),
					at: nowIso(),
					role: 'assistant',
					kind: 'error',
					content: message
				}
			];
			if (!this.activeRunId && !this.isAwaitingImportDecision() && this.queue.length > 0) {
				void this.drainQueue();
			}
		}
	}

	private async finishPostImportRun(
		jobId: string,
		result: NonNullable<ImportDecisionTarget['importResult']>
	) {
		const pending = this.pendingPostImportRun;
		if (!pending || pending.jobId !== jobId) return;
		this.pendingPostImportRun = null;
		this.awaitingImportJob = false;
		const target: ImportDecisionTarget = {
			...pending.target,
			imported: true,
			jobId,
			importResult: result
		};
		await this.startAgentAfterImportDecision(pending, target);
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
		skillIds: string[] = [],
		attachments: ComposerAttachment[] = []
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
				...(skillIds.length > 0 ? { skillIds } : {}),
				...(attachments.length > 0
					? {
							attachments: attachments.map((item) => ({
								id: item.id,
								name: item.name,
								sourcePath: item.sourcePath
							}))
						}
					: {})
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
				attachments,
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
			attachments?: ComposerAttachment[];
		}
	) {
		this.running = true;
		const connectionIds = (options.connectionIds ?? [])
			.map((id) => id.trim())
			.filter(Boolean);
		const skillIds = (options.skillIds ?? []).map((id) => id.trim()).filter(Boolean);
		const pendingAttachments = options.attachments ?? [];
		try {
			// Connection warm-up / schema refresh runs in Rust after start_run returns.
			// Do not await FE connect loops here — they re-boot the workspace and stall typing.

			if (!this.session) {
				this.session = await api.agentCreateSession(DATA_ANALYST_ID, {
					title: query.slice(0, 80)
				});
			}

			const staged = await this.stageAndParseAttachments(pendingAttachments);

			if (options.addUserMessage) {
				const meta: Record<string, unknown> = {};
				if (connectionIds.length > 0) meta.connectionIds = connectionIds;
				if (skillIds.length > 0) meta.skillIds = skillIds;
				if (staged.length > 0) {
					meta.attachments = staged.map((item) => ({
						id: item.id,
						name: item.name,
						path: item.path,
						kind: item.kind,
						size: item.size
					}));
				}
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

			const importMessageIds =
				staged.length > 0 ? await this.pushImportPlanCards(staged) : [];

			// Import plan cards need an explicit user choice before the agent continues.
			if (importMessageIds.length > 0 && this.session) {
				this.pendingImportRun = {
					query,
					connectionIds,
					skillIds,
					staged,
					sessionId: this.session.id,
					importMessageIds
				};
				this.awaitingImportDecision = true;
				this.running = true;
				this.liveProgressId = uid();
				this.messages = [
					...this.messages,
					{
						id: this.liveProgressId,
						at: nowIso(),
						role: 'assistant',
						kind: 'progress',
						content: t('agentMode.live.awaitingImportDecision'),
						meta: { phase: 'awaiting-import' }
					}
				];
				await this.refreshHistory();
				return;
			}

			await this.launchAgentRun({
				query,
				connectionIds,
				skillIds,
				staged
			});
		} catch (error) {
			const message = errorMessage(error);
			this.error = message;
			this.clearPendingImportDecision();
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

	private async launchAgentRun(input: {
		query: string;
		connectionIds: string[];
		skillIds: string[];
		staged: StagedAttachment[];
		importTarget?: ImportDecisionTarget;
	}) {
		if (!this.session) {
			throw new Error(t('agentMode.live.needDesktop'));
		}
		const systemMd = await api.agentReadSystem(DATA_ANALYST_ID);
		const profile = selectedLlmProvider();
		if (!profile) {
			throw new Error(t('agentMode.live.needLlm'));
		}
		const llm = toLlmSettings(profile);
		this.clearLiveProgress();
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

		const enrichedQuery =
			input.staged.length > 0
				? `${input.query}\n\n${formatAttachmentContext(input.staged, input.importTarget)}`
				: input.query;

		const connectionIds = [...input.connectionIds];
		if (
			input.importTarget?.connectionId &&
			!connectionIds.includes(input.importTarget.connectionId)
		) {
			connectionIds.push(input.importTarget.connectionId);
		}

		// Returns immediately; schema warm + Node graph start continue in the background.
		const { runId } = await api.agentStartRun({
			agentId: DATA_ANALYST_ID,
			sessionId: this.session.id,
			query: enrichedQuery,
			systemMd,
			locale: getLocale(),
			connectionIds,
			skillIds: input.skillIds,
			authMode: appMode.prefs.authMode,
			attachments: input.staged.map((item) => ({
				id: item.id,
				name: item.name,
				path: item.path,
				kind: item.kind,
				size: item.size,
				sha256: item.sha256,
				sample: item.sample ?? null
			})),
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
	}

	private clearPendingImportDecision() {
		const pending = this.pendingImportRun;
		this.pendingImportRun = null;
		this.awaitingImportDecision = false;
		this.pendingPostImportRun = null;
		this.awaitingImportJob = false;
		if (!pending) return;
		for (const id of pending.importMessageIds) {
			this.updateImportMessage(id, { awaitingDecision: false });
		}
	}

	private async drainQueue() {
		if (
			this.draining ||
			this.activeRunId ||
			this.awaitingImportDecision ||
			this.awaitingImportJob
		) {
			return;
		}
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
				skillIds: next.skillIds,
				attachments: next.attachments
			});
		} finally {
			this.draining = false;
			// beginRun failure / import wait clears activeRunId — only drain when free.
			if (
				!this.activeRunId &&
				!this.awaitingImportDecision &&
				!this.awaitingImportJob &&
				this.queue.length > 0
			) {
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

	private async stageAndParseAttachments(
		files: ComposerAttachment[]
	): Promise<StagedAttachment[]> {
		if (!this.session || files.length === 0) return [];
		const staged = await api.agentStageAttachments({
			agentId: DATA_ANALYST_ID,
			sessionId: this.session.id,
			files: files.map((item) => ({
				sourcePath: item.sourcePath,
				name: item.name
			}))
		});
		const out: StagedAttachment[] = [];
		for (const item of staged) {
			try {
				const sample = await api.importParseSample({
					path: item.path,
					name: item.name
				});
				out.push({ ...item, sample });
			} catch (error) {
				out.push({ ...item, sample: null });
				const message = errorMessage(error);
				const entry: ChatMessage = {
					id: uid(),
					at: nowIso(),
					role: 'assistant',
					kind: 'error',
					content: t('agentMode.live.importParseFailed', {
						name: item.name,
						error: message
					})
				};
				this.messages = [...this.messages, entry];
			}
		}
		return out;
	}

	private async pushImportPlanCards(staged: StagedAttachment[]): Promise<string[]> {
		const ids: string[] = [];
		for (const item of staged) {
			if (!item.sample) continue;
			const entry = await this.persist({
				id: uid(),
				at: nowIso(),
				role: 'assistant',
				kind: 'import',
				content: t('agentMode.live.importPlanTitle', { name: item.name }),
				meta: {
					phase: 'plan',
					awaitingDecision: true,
					attachment: {
						id: item.id,
						name: item.name,
						path: item.path,
						kind: item.kind,
						size: item.size,
						sha256: item.sha256
					},
					sample: item.sample
				}
			});
			ids.push(entry.id);
			this.messages = [...this.messages, toChatMessage(entry)];
		}
		return ids;
	}

	updateImportMessage(messageId: string, patch: Record<string, unknown>) {
		this.messages = this.messages.map((item) => {
			if (item.id !== messageId) return item;
			return {
				...item,
				meta: {
					...(item.meta ?? {}),
					...patch
				}
			};
		});
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

function formatAttachmentContext(
	staged: StagedAttachment[],
	importTarget?: ImportDecisionTarget
): string {
	const blocks = staged.map((item) => {
		const sample = item.sample;
		if (!sample) {
			return `Attachment: ${item.name} (${item.kind}) path=${item.path} — parse failed`;
		}
		const headerLine = sample.headers.join(' | ');
		const rowLines = sample.sampleRows
			.slice(0, 20)
			.map((row) => row.join(' | '))
			.join('\n');
		return [
			`Attachment: ${item.name} (${item.kind})`,
			`Absolute path (use this path; do not scan the disk): ${item.path}`,
			`Suggested table: ${sample.suggestedTable}`,
			`Total data rows: ${sample.totalRows}; columns: ${sample.columnCount}`,
			`Headers: ${headerLine}`,
			`Sample rows (up to 20):`,
			rowLines || '(none)'
		].join('\n');
	});
	const lines = [
		'The user attached tabular file(s). Analyze the sample below (NOT the full file).',
		'Recommend either CREATE TABLE DDL (MySQL/PostgreSQL) or matching an existing table with column mapping.',
		'Never walk the whole filesystem to find the file — use the Absolute path below.'
	];
	if (importTarget) {
		lines.push(
			`User selected import target: connectionId=${importTarget.connectionId}, schema=${importTarget.schema}, table=${importTarget.table}, mode=${importTarget.mode}.`
		);
		if (importTarget.imported) {
			const result = importTarget.importResult;
			if (result?.ok) {
				lines.push(
					`Native UI import already completed successfully: writtenRows=${result.writtenRows ?? '?'}, totalRows=${result.totalRows ?? '?'}.`,
					'Do NOT DELETE/TRUNCATE/DROP this table and do NOT run write_script/run_script to re-import.',
					'Focus on SELECT validation and analysis guidance only.'
				);
			} else if (result && !result.ok) {
				lines.push(
					`Native UI import failed: ${result.error ?? 'unknown error'}.`,
					'Do NOT DELETE/TRUNCATE the table. Diagnose with SELECT / schema tools; only propose a fix, do not clear existing data unless the user explicitly asks.'
				);
			} else {
				lines.push(
					'Native UI import owns loading for this target.',
					'Do NOT DELETE/TRUNCATE/DROP the target table and do NOT re-import via scripts.'
				);
			}
		} else {
			lines.push(
				'The user asked to analyze with this target first; do not execute writes unless they later confirm import.'
			);
		}
	} else {
		lines.push(
			'Do not execute writes; the UI will let the user confirm import via the native import job.'
		);
	}
	return [...lines, '', ...blocks].join('\n');
}

export const agentChat = new AgentChatStore();
