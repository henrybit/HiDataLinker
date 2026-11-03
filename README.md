# HiDataLinker

[English](./README.md) | [中文](./README.zh-CN.md)

**Free desktop DB client. AI that actually understands your schema.**

A free desktop database client — everyday management with zero friction; pay as you go when you need schema understanding, documentation, or agent tasks.

**Windows · macOS** · **MySQL · PostgreSQL · SQL Server · Oracle** · UI in Chinese / English

## Download

Official installers are on [GitHub Releases](https://github.com/henrybit/db-gui/releases):

- macOS: `.dmg` (Apple Silicon / Intel)
- Windows: `.msi`

See [FEATURES.md](./FEATURES.md) for capabilities by version.

---

## Vision

Traditional database tools help you *see* tables and columns; HiDataLinker helps you *understand* the database.

We believe connecting, querying, editing data, and migrating should be free and pleasant; high-value capabilities like relationship inference, business comments, and agent workflows should be usage-based — **work first, pay for intelligence when you need it**.

Not another fuller Navicat — **a free daily client plus a real tier for schema understanding and agents**.

---

## How to use

1. **Download & install** — Pick Windows or macOS. Ready to use after install; no license required.
2. **Create a connection** — Choose an engine, fill in host details (or paste a connection URL), test, then save.
3. **Browse & operate** — Expand databases / schemas in the object tree, open tables for data and structure, query / format / Explain in the SQL editor.
4. **Export & migrate** — Dump to `.sql` for backup or environment moves, or use Migration Center to move same-engine databases to a target connection.
5. **Turn on intelligence when needed** — Run Relationship Network Analysis: pick a scope, optionally upload business docs (Markdown / Word / PDF); the LLM fills in comments and infers implicit links; export results as Markdown. Agent tasks are billed per run.

Everyday query and table edits are free; charges apply only when you call an LLM or agent.

---

## Capabilities

### Core management (free)

- Connection management: save / edit / test / connect; optional password entry without persisting to disk
- Object browser: databases / schemas, tables, views, indexes, triggers, functions and stored procedures
- Data operations: paged preview, grid / form views, insert / edit / delete rows
- Structure & DDL: column-level edits, view create-table statements
- SQL workbench: execute, format, Explain, run with shortcuts
- DB ops: create / drop databases or schemas, Dump export, run SQL files
- Same-engine migration: visual source → target migration with progress and history
- Platforms & locales: native Windows / macOS desktop apps, Chinese / English UI

### Intelligence (usage-based)

- **LLM access (token billing)** — relationship network analysis, missing-comment inference, business grounding from uploaded docs
- **Agent access (per-task billing)** — task-based analysis and similar flows; extensible toward ops / docs agents later

Connections and data stay on your machine; secrets stay local and are sent only to the model service you choose.

---

## vs traditional DB admin tools

| | Traditional tools (e.g. Navicat, DBeaver) | HiDataLinker |
|---|------------------------------------------|--------------|
| **Pricing** | Seat perpetual / subscription, or free builds with cut features | Client free; intelligence billed by token / task |
| **Focus** | GUI breadth, sync, enterprise plugins | Daily GUI + **understanding schema** |
| **AI** | Often add-on or missing | First-class: relationship nets, comments, doc-enhanced analysis |
| **Agents** | Rarely present | Task-based access; pay for what you use |
| **Platforms** | Varies by vendor | Windows + macOS, aligned experience |
| **Getting started** | Often pay first or pick a complex edition | Download and go; no license wall |

In short:

- Navicat sells a **professional GUI license**
- DBeaver sells **engine breadth and enterprise editions**
- HiDataLinker sells a **free entry + AI / Agent usage**

Fuller positioning and competitor notes: [PRODUCT_POSITIONING.md](./PRODUCT_POSITIONING.md).

---

## Who it's for

- Individuals and small teams who want a free alternative for everyday query and edits
- SMBs with legacy databases that lack docs and clear relationships, and want AI to produce relationship maps and explanations quickly
- Mixed Windows / Mac engineering & ops teams who want one tool with consistent dual-platform experience

---

## Development notes (agent sidecar)

The built-in data-analysis agent’s LangGraph orchestration runs in a Node child process `agents-runtime` (hosted by Rust over stdio NDJSON). Tools and LLM secrets remain on the Rust side only.

```bash
pnpm agents-runtime:build   # emits agents-runtime/dist/main.mjs
pnpm tauri:dev              # beforeDevCommand builds the runtime first
```

Release builds embed a platform Node via Tauri `externalBin` (`pnpm fetch:bundled-node` → `src-tauri/binaries/node-<triple>`). Dev machines can use the system Node; override with `HIDATALINKER_NODE`. Launching from Finder on macOS often has a PATH without Homebrew/nvm, so packaged builds must ship the bundled Node.

Chat messages persist per session under `~/.hidatalinker/chats/<agentId>/<sessionId>/` (`meta.json`, `transcript.jsonl`, `checkpoint.json`). Legacy `agents/<agentId>/sessions/` is migrated automatically on startup.

Connection schema metadata is cached under `~/.hidatalinker/db/` (`index.json` + `schemas/<connectionId>/`). On app start and during agent chats, schemas older than 24 hours are refreshed; failed connections are marked, and caches for connections that fail continuously for over 7 days are cleared.

Runtime logs default to `~/.hidatalinker/runtime/YYYY-MM-DD.log` (toggle in Settings → General); on startup, logs older than 7 days are cleaned up.

---

Stars, issues, and suggestions are welcome. Let’s turn a capable free client into an intelligent workbench that actually understands your databases.
