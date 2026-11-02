# agents-runtime

Node sidecar for HiDataLinker built-in agents (LangGraph data-analyst).

Rust (`AgentRunner`) spawns `node dist/main.mjs` and speaks **stdio NDJSON**. Tool calls and LLM completions are proxied back to Rust; **API keys never enter this process**.

## Develop

```bash
pnpm --filter @hidatalinker/agents-runtime build
# or: pnpm tauri:dev  (builds runtime via beforeDevCommand)
```

Override paths:

- `HIDATALINKER_NODE` — Node binary
- `HIDATALINKER_AGENTS_RUNTIME` — path to `main.mjs`
- `HIDATALINKER_AGENT_MAX_RUNS` — concurrency (default 3)

Release bundles `dist/main.mjs` as a Tauri resource and still requires a system/`PATH` Node for now (embedding a full Node binary is a follow-up).
