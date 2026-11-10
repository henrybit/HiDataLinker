import { isChartEngine, type ChartEngine } from '$lib/agent/chart-spec';
import type { MessageKey } from '$lib/i18n/en';
import { workspace } from '$lib/stores/workspace.svelte';

export type AppMode = 'database' | 'agent';
export type AgentNav = 'chat' | 'sources' | 'insights' | 'history';
export type AgentSessionId = 'home' | 'analysis-revenue' | 'write-orders';
export type WritePlanStatus = 'pending' | 'confirmed' | 'canceled' | 'dry-run';
/** Agent tool authorization: read-only SQL vs full DML/DDL. */
export type AgentAuthMode = 'readonly' | 'full';

const PREFS_KEY = 'hidatalinker-agent-prefs';

export type AgentBehaviorPrefs = {
	autoConnect: boolean;
	autoLearnSemantics: boolean;
	crossDbJoins: boolean;
	maskSensitive: boolean;
	writeConfirm: boolean;
	/** Composer authorization mode for agent SQL tools. */
	authMode: AgentAuthMode;
	/** Chat chart renderer: ECharts or Ant Design Charts (AntV G2). */
	chartEngine: ChartEngine;
	/** Conversation session rail expanded (true) or collapsed (false). */
	sessionRailExpanded: boolean;
	/** Left workspace sidebar expanded (true) or collapsed to icons (false). */
	sidebarExpanded: boolean;
	sourceAutoConnect: Record<string, boolean>;
};

export function isAgentAuthMode(value: unknown): value is AgentAuthMode {
	return value === 'readonly' || value === 'full';
}

function defaultPrefs(): AgentBehaviorPrefs {
	return {
		autoConnect: true,
		autoLearnSemantics: true,
		crossDbJoins: true,
		maskSensitive: true,
		writeConfirm: true,
		authMode: 'readonly',
		chartEngine: 'echarts',
		sessionRailExpanded: true,
		sidebarExpanded: true,
		sourceAutoConnect: {}
	};
}

function readPrefs(): AgentBehaviorPrefs {
	if (typeof localStorage === 'undefined') return defaultPrefs();
	try {
		const raw = localStorage.getItem(PREFS_KEY);
		if (!raw) return defaultPrefs();
		const parsed = { ...defaultPrefs(), ...JSON.parse(raw) } as AgentBehaviorPrefs;
		if (!isChartEngine(parsed.chartEngine)) parsed.chartEngine = 'echarts';
		if (!isAgentAuthMode(parsed.authMode)) parsed.authMode = 'readonly';
		return parsed;
	} catch {
		return defaultPrefs();
	}
}

function writePrefs(prefs: AgentBehaviorPrefs) {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
	} catch {
		/* ignore */
	}
}

class AppModeStore {
	mode = $state<AppMode>('database');
	agentNav = $state<AgentNav>('chat');
	sessionId = $state<AgentSessionId>('home');
	composerDraft = $state('');
	writePlanStatus = $state<WritePlanStatus>('pending');
	statusMessage = $state<MessageKey | null>(null);
	prefs = $state<AgentBehaviorPrefs>(readPrefs());
	/** Insight ids converted to local tasks */
	taskedInsightIds = $state<Set<string>>(new Set());
	processedInsightIds = $state<Set<string>>(new Set());

	setMode(mode: AppMode) {
		this.mode = mode;
		if (mode === 'agent') {
			if (this.agentNav === 'chat' && this.sessionId === 'home') {
				this.statusMessage = null;
			}
			void workspace.warmAgentConnections({
				autoConnect: this.prefs.autoConnect,
				sourceAutoConnect: this.prefs.sourceAutoConnect
			});
		}
	}

	setAgentNav(nav: AgentNav) {
		this.agentNav = nav;
		if (nav !== 'chat') {
			/* keep session for return */
		} else if (this.sessionId !== 'home') {
			/* stay on session when returning to chat */
		}
	}

	openHome() {
		this.agentNav = 'chat';
		this.sessionId = 'home';
		this.composerDraft = '';
		this.writePlanStatus = 'pending';
		this.statusMessage = null;
	}

	openSession(id: Exclude<AgentSessionId, 'home'>, seed?: string) {
		this.agentNav = 'chat';
		this.sessionId = id;
		if (seed) this.composerDraft = seed;
		if (id === 'write-orders') this.writePlanStatus = 'pending';
		this.statusMessage = null;
	}

	submitComposer(text?: string) {
		const query = (text ?? this.composerDraft).trim();
		if (!query) return;
		const lower = query.toLowerCase();
		if (
			lower.includes('标记') ||
			lower.includes('mark') ||
			lower.includes('退款') ||
			lower.includes('refund') ||
			lower.includes('写入') ||
			lower.includes('update')
		) {
			this.openSession('write-orders', query);
			return;
		}
		this.openSession('analysis-revenue', query);
	}

	applySuggestion(session: Exclude<AgentSessionId, 'home'>, label: string) {
		this.openSession(session, label);
	}

	setWritePlanStatus(status: WritePlanStatus, messageKey?: MessageKey) {
		this.writePlanStatus = status;
		this.statusMessage = messageKey ?? null;
	}

	patchPrefs(patch: Partial<AgentBehaviorPrefs>) {
		this.prefs = { ...this.prefs, ...patch };
		writePrefs(this.prefs);
	}

	setSourceAutoConnect(id: string, enabled: boolean) {
		this.prefs = {
			...this.prefs,
			sourceAutoConnect: { ...this.prefs.sourceAutoConnect, [id]: enabled }
		};
		writePrefs(this.prefs);
	}

	convertInsightToTask(id: string) {
		if (this.taskedInsightIds.has(id)) return;
		const nextTasked = new Set(this.taskedInsightIds);
		nextTasked.add(id);
		this.taskedInsightIds = nextTasked;
		const nextProcessed = new Set(this.processedInsightIds);
		nextProcessed.add(id);
		this.processedInsightIds = nextProcessed;
	}
}

export const appMode = new AppModeStore();
