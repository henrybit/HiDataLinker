import * as readline from 'node:readline';
import { emitRunDone, emitRunEvent, handleHostReply } from './host.js';
import { runDataAnalyst } from './data-analyst/workflow.js';
import type { Envelope, RunStartPayload } from './protocol.js';

const MAX_CONCURRENT = Number(process.env.HIDATALINKER_AGENT_MAX_RUNS ?? 3);

type ActiveRun = {
	controller: AbortController;
	promise: Promise<void>;
};

const runs = new Map<string, ActiveRun>();

function logErr(message: string) {
	process.stderr.write(`[agents-runtime] ${message}\n`);
}

async function startRun(payload: RunStartPayload) {
	const { runId } = payload;
	if (runs.has(runId)) {
		emitRunDone(runId, '', 'run already active');
		return;
	}
	if (runs.size >= MAX_CONCURRENT) {
		emitRunDone(runId, '', `concurrency limit (${MAX_CONCURRENT}) reached`);
		return;
	}

	const controller = new AbortController();
	const promise = (async () => {
		try {
			const result = await runDataAnalyst({
				runId,
				userQuery: payload.query,
				systemMd: payload.systemMd,
				locale: payload.locale,
				llm: payload.llm,
				signal: controller.signal,
				onEvent: (event) => emitRunEvent(runId, event)
			});
			if (controller.signal.aborted) {
				emitRunDone(runId, result.markdown || '', 'cancelled');
			} else {
				emitRunDone(runId, result.markdown, result.error);
			}
		} catch (caught) {
			const message = caught instanceof Error ? caught.message : String(caught);
			emitRunDone(runId, '', message);
		} finally {
			runs.delete(runId);
		}
	})();

	runs.set(runId, { controller, promise });
}

function cancelRun(runId: string) {
	const active = runs.get(runId);
	if (!active) return;
	active.controller.abort();
}

function handleMessage(msg: Envelope) {
	switch (msg.type) {
		case 'run.start':
			void startRun(msg.payload as RunStartPayload);
			break;
		case 'run.cancel': {
			const runId = (msg.payload as { runId?: string } | undefined)?.runId;
			if (runId) cancelRun(runId);
			break;
		}
		case 'tool.result':
		case 'llm.result':
			handleHostReply(msg);
			break;
		case 'ping':
			process.stdout.write(
				`${JSON.stringify({ id: msg.id, type: 'pong', payload: { ok: true } })}\n`
			);
			break;
		default:
			logErr(`unknown message type: ${msg.type}`);
	}
}

function main() {
	process.stdout.write(
		`${JSON.stringify({ id: 'boot', type: 'runtime.ready', payload: { maxConcurrent: MAX_CONCURRENT } })}\n`
	);

	const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
	rl.on('line', (line) => {
		const trimmed = line.trim();
		if (!trimmed) return;
		try {
			const msg = JSON.parse(trimmed) as Envelope;
			handleMessage(msg);
		} catch (caught) {
			logErr(`bad json: ${caught instanceof Error ? caught.message : String(caught)}`);
		}
	});
	rl.on('close', () => {
		for (const active of runs.values()) {
			active.controller.abort();
		}
		process.exit(0);
	});
}

main();
