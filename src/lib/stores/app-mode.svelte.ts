import type { MessageKey } from '$lib/i18n/en';

export type AppMode = 'database' | 'agent';
export type AgentNav = 'chat' | 'sources' | 'insights' | 'history';
export type AgentSessionId = 'home' | 'analysis-revenue' | 'write-orders';
export type WritePlanStatus = 'pending' | 'confirmed' | 'canceled' | 'dry-run';

const PREFS_KEY = 'hidatalinker-agent-prefs';

export type AgentBehaviorPrefs = {
	autoConnect: boolean;
	autoLearnSemantics: boolean;
	crossDbJoins: boolean;
	maskSensitive: boolean;
	writeConfirm: boolean;
	sourceAutoConnect: Record<string, boolean>;
};

function defaultPrefs(): AgentBehaviorPrefs {
	return {
		autoConnect: true,
		autoLearnSemantics: true,
		crossDbJoins: true,
		maskSensitive: true,
		writeConfirm: true,
		sourceAutoConnect: {}
	};
}

function readPrefs(): AgentBehaviorPrefs {
	if (typeof localStorage === 'undefined') return defaultPrefs();
	try {
		const raw = localStorage.getItem(PREFS_KEY);
		if (!raw) return defaultPrefs();
		return { ...defaultPrefs(), ...JSON.parse(raw) };
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
		if (mode === 'agent' && this.agentNav === 'chat' && this.sessionId === 'home') {
			this.statusMessage = null;
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

	submitComposer() {
		const text = this.composerDraft.trim();
		if (!text) return;
		const lower = text.toLowerCase();
		if (
			lower.includes('标记') ||
			lower.includes('mark') ||
			lower.includes('退款') ||
			lower.includes('refund') ||
			lower.includes('写入') ||
			lower.includes('update')
		) {
			this.openSession('write-orders', text);
			return;
		}
		this.openSession('analysis-revenue', text);
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
