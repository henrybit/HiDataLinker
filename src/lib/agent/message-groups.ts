import type { ChatMessage } from '$lib/stores/agent-chat.svelte';

export type ProcessGroupItem = {
	type: 'process';
	id: string;
	messages: ChatMessage[];
	live: boolean;
};

export type MessageGroupItem = {
	type: 'message';
	id: string;
	message: ChatMessage;
};

export type ChatRenderItem = ProcessGroupItem | MessageGroupItem;

function isProcessMessage(message: ChatMessage): boolean {
	return message.kind === 'progress' || message.kind === 'tool';
}

function isFinalMessage(message: ChatMessage): boolean {
	return message.kind === 'markdown' || message.kind === 'error';
}

/**
 * Group intermediate progress/tool messages so completed turns can collapse them.
 * The active (still-running) process group stays expanded.
 */
export function buildChatRenderItems(
	messages: ChatMessage[],
	running: boolean
): ChatRenderItem[] {
	let lastUserIndex = -1;
	for (let i = messages.length - 1; i >= 0; i -= 1) {
		if (messages[i]?.role === 'user') {
			lastUserIndex = i;
			break;
		}
	}

	const lastTurnHasFinal = messages
		.slice(lastUserIndex + 1)
		.some(isFinalMessage);

	const items: ChatRenderItem[] = [];
	let processBuf: ChatMessage[] = [];

	const flushProcess = (inLastTurn: boolean) => {
		if (processBuf.length === 0) return;
		const live = running && inLastTurn && !lastTurnHasFinal;
		items.push({
			type: 'process',
			id: `process-${processBuf[0]?.id ?? items.length}`,
			messages: processBuf,
			live
		});
		processBuf = [];
	};

	for (let i = 0; i < messages.length; i += 1) {
		const message = messages[i];
		if (!message) continue;
		const inLastTurn = i > lastUserIndex;
		if (isProcessMessage(message)) {
			processBuf.push(message);
			continue;
		}
		flushProcess(inLastTurn);
		items.push({ type: 'message', id: message.id, message });
	}
	flushProcess(true);

	return items;
}
