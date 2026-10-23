import { api, isTauriRuntime } from '$lib/api/tauri';
import type { AgentAuditEntry, AgentStatus } from '$lib/api/types';

export const agentGateway = $state<{
	status: AgentStatus | null;
	entries: AgentAuditEntry[];
	toggling: boolean;
}>({
	status: null,
	entries: [],
	toggling: false
});

export async function refreshAgentGateway() {
	if (!isTauriRuntime()) return;
	agentGateway.status = await api.agentStatus();
	agentGateway.entries = await api.listAgentAudit();
}

export async function setAgentGatewayEnabled(enabled: boolean) {
	agentGateway.toggling = true;
	try {
		agentGateway.status = await api.setAgentEnabled(enabled);
	} finally {
		agentGateway.toggling = false;
	}
}

export function applyAgentStatus(status: AgentStatus) {
	agentGateway.status = status;
}

export function pushAgentAudit(entry: AgentAuditEntry) {
	agentGateway.entries = [
		entry,
		...agentGateway.entries.filter((item) => item.id !== entry.id)
	].slice(0, 40);
}
