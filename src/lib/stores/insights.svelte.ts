import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
import type { InsightItem, InsightsReport } from '$lib/api/types';
import { getLocale, t } from '$lib/i18n/i18n.svelte';
import { selectedLlmProvider } from '$lib/llm/catalog.svelte';
import { toLlmSettings } from '$lib/llm/providers';
import { agentChat } from '$lib/stores/agent-chat.svelte';
import { appMode } from '$lib/stores/app-mode.svelte';
import { workspace } from '$lib/stores/workspace.svelte';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/** Same TTL as the schema metadata cache: older reports are regenerated on open. */
const STALE_AFTER_MS = 24 * 60 * 60 * 1000;

type InsightsRunEvent = {
	runId: string;
	event: { type: 'status'; phase: string; message: string } | { type: string; message?: string };
};

type InsightsRunDone = {
	runId: string;
	status: 'done' | 'error' | 'cancelled';
	error?: string | null;
	report?: InsightsReport | null;
	durationMs?: number;
};

export function isReportStale(report: InsightsReport | null, now = Date.now()): boolean {
	if (!report) return true;
	const at = Date.parse(report.generatedAt);
	return !Number.isFinite(at) || now - at > STALE_AFTER_MS;
}

class InsightsStore {
	report = $state<InsightsReport | null>(null);
	runId = $state<string | null>(null);
	statusMessage = $state<string | null>(null);
	error = $state<string | null>(null);
	loaded = $state(false);
	private subscribed = false;
	private unsubs: UnlistenFn[] = [];
	/** Done events can arrive before `insightsStartRun` resolves for instant failures. */
	private finishedRunIds = new Set<string>();

	get running() {
		return this.runId !== null;
	}

	/** Subscribe to run events, load the last report, and refresh it in the background if stale. */
	async init() {
		if (!isTauriRuntime()) {
			this.loaded = true;
			return;
		}
		await this.ensureSubscribed();
		if (!this.loaded) {
			try {
				this.report = await api.insightsReadLatest();
			} catch (error) {
				console.error('[insights] load', error);
			}
			this.loaded = true;
		}
		if (!this.running && isReportStale(this.report) && workspace.connections.length > 0) {
			const profile = selectedLlmProvider();
			if (profile && (profile.apiKey.trim() || profile.baseUrl.trim())) {
				void this.run();
			}
		}
	}

	private async ensureSubscribed() {
		if (this.subscribed) return;
		this.subscribed = true;
		this.unsubs.push(
			await listen<InsightsRunEvent>('insights-run-event', (event) => {
				if (event.payload.runId !== this.runId) return;
				const inner = event.payload.event;
				if (inner.type === 'status' && inner.message) this.statusMessage = inner.message;
			})
		);
		this.unsubs.push(
			await listen<InsightsRunDone>('insights-run-done', (event) => {
				this.onDone(event.payload);
			})
		);
	}

	private onDone(payload: InsightsRunDone) {
		if (this.runId && payload.runId !== this.runId) return;
		this.finishedRunIds.add(payload.runId);
		this.runId = null;
		this.statusMessage = null;
		if (payload.report) {
			this.report = payload.report;
			this.error = null;
		} else if (payload.status !== 'cancelled') {
			this.error = payload.error || t('agentMode.insights.failed');
		}
	}

	/** Returns as soon as the host accepts the run; the graph runs in the agents sidecar. */
	async run() {
		if (!isTauriRuntime()) {
			this.error = t('agentMode.live.needDesktop');
			return;
		}
		if (this.running) return;
		const profile = selectedLlmProvider();
		if (!profile || (!profile.apiKey.trim() && !profile.baseUrl.trim())) {
			this.error = t('agentMode.live.needLlm');
			return;
		}
		await this.ensureSubscribed();
		const llm = toLlmSettings(profile);
		this.error = null;
		this.statusMessage = t('agentMode.insights.starting');
		try {
			const { runId } = await api.insightsStartRun({
				locale: getLocale() === 'zh' ? 'zh' : 'en',
				llm: {
					provider: llm.provider,
					apiKey: llm.apiKey,
					model: llm.model,
					baseUrl: llm.baseUrl
				}
			});
			if (!this.finishedRunIds.has(runId)) this.runId = runId;
		} catch (error) {
			this.statusMessage = null;
			this.error = errorMessage(error);
		}
	}

	cancel() {
		const runId = this.runId;
		if (!runId) return;
		this.runId = null;
		this.statusMessage = null;
		void api.agentCancelRun(runId).catch(() => undefined);
	}

	/** Hand a recommended scenario to the data-analyst agent in a fresh conversation. */
	async analyzeInChat(item: InsightItem) {
		const known = new Set(workspace.connections.map((conn) => conn.id));
		const connectionIds = item.connectionIds.filter((id) => known.has(id));
		appMode.convertInsightToTask(item.id);
		appMode.setAgentNav('chat');
		await agentChat.newChat();
		await agentChat.submit(item.prompt, connectionIds);
	}

	dispose() {
		for (const stop of this.unsubs) stop();
		this.unsubs = [];
		this.subscribed = false;
	}
}

export const insights = new InsightsStore();
