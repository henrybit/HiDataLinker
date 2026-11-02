import { randomUUID } from 'node:crypto';
import type {
	Envelope,
	LlmCompletePayload,
	LlmResultPayload,
	ToolInvokePayload,
	ToolResultPayload
} from './protocol.js';

type Pending = {
	resolve: (value: unknown) => void;
	reject: (error: Error) => void;
	timer: ReturnType<typeof setTimeout>;
};

const pending = new Map<string, Pending>();
const REQUEST_TIMEOUT_MS = 120_000;

export function send(envelope: Envelope) {
	process.stdout.write(`${JSON.stringify(envelope)}\n`);
}

export function handleHostReply(msg: Envelope) {
	const wait = pending.get(msg.id);
	if (!wait) return;
	clearTimeout(wait.timer);
	pending.delete(msg.id);
	const payload = msg.payload as { ok?: boolean; error?: string | null; result?: unknown } | undefined;
	if (msg.type === 'tool.result' || msg.type === 'llm.result') {
		wait.resolve(payload ?? { ok: false, error: 'empty reply' });
		return;
	}
	wait.reject(new Error(`unexpected reply type ${msg.type}`));
}

function request<T>(type: string, payload: unknown): Promise<T> {
	const id = randomUUID();
	return new Promise<T>((resolve, reject) => {
		const timer = setTimeout(() => {
			pending.delete(id);
			reject(new Error(`host request timed out: ${type}`));
		}, REQUEST_TIMEOUT_MS);
		pending.set(id, {
			resolve: (value) => resolve(value as T),
			reject,
			timer
		});
		send({ id, type, payload });
	});
}

export async function invokeTool(
	runId: string,
	name: string,
	args: Record<string, unknown> = {}
): Promise<ToolResultPayload> {
	const payload: ToolInvokePayload = { runId, name, args };
	return request<ToolResultPayload>('tool.invoke', payload);
}

export async function completeLlm(
	payload: Omit<LlmCompletePayload, never>
): Promise<LlmResultPayload> {
	return request<LlmResultPayload>('llm.complete', payload);
}

export function emitRunEvent(runId: string, event: unknown) {
	send({ id: randomUUID(), type: 'run.event', payload: { runId, event } });
}

export function emitRunDone(runId: string, markdown: string, error?: string) {
	send({
		id: randomUUID(),
		type: error && !markdown ? 'run.error' : 'run.done',
		payload: { runId, markdown, error: error ?? null }
	});
}

export function summarizeToolResult(name: string, result: ToolResultPayload): string {
	if (!result.ok) return result.error?.trim() || 'tool failed';
	try {
		const text = JSON.stringify(result.result);
		if (text.length <= 2400) return text;
		return `${text.slice(0, 2400)}…`;
	} catch {
		return `${name} ok`;
	}
}
