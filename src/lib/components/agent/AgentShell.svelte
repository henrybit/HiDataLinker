<script lang="ts">
	import { appMode } from '$lib/stores/app-mode.svelte';
	import AgentSidebar from './AgentSidebar.svelte';
	import AgentLiveChat from './AgentLiveChat.svelte';
	import AgentAnalysisView from './AgentAnalysisView.svelte';
	import AgentWritePlanView from './AgentWritePlanView.svelte';
	import AgentContextPanel from './AgentContextPanel.svelte';
	import AgentSourcesView from './AgentSourcesView.svelte';
	import AgentInsightsView from './AgentInsightsView.svelte';
	import AgentHistoryView from './AgentHistoryView.svelte';

	const showDemoContext = $derived(
		appMode.agentNav === 'chat' &&
			(appMode.sessionId === 'analysis-revenue' || appMode.sessionId === 'write-orders')
	);
</script>

<div class="agent-shell" class:with-context={showDemoContext}>
	<AgentSidebar />
	<div class="agent-main">
		{#if appMode.agentNav === 'sources'}
			<AgentSourcesView />
		{:else if appMode.agentNav === 'insights'}
			<AgentInsightsView />
		{:else if appMode.agentNav === 'history'}
			<AgentHistoryView />
		{:else if appMode.sessionId === 'analysis-revenue'}
			<AgentAnalysisView />
		{:else if appMode.sessionId === 'write-orders'}
			<AgentWritePlanView />
		{:else}
			<AgentLiveChat />
		{/if}
	</div>
	{#if showDemoContext}
		<AgentContextPanel />
	{/if}
</div>
