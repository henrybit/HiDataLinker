# HiDataLinker

[English](./README.md) | [中文](./README.zh-CN.md)

**Free MIT desktop client for MySQL and PostgreSQL.**

A Navicat-style desktop database client: connect, browse, query, Dump, and migrate, with credentials that stay on your machine. It also connects to MariaDB over the MySQL protocol, using the same connection type as MySQL, for everyday schema browsing and querying. MariaDB is not documented or tested as a separate engine, so MariaDB-specific features may not be fully covered. Listed on the [MariaDB Ecosystem Hub](https://ecohub.mariadb.org/database-management/hidatalinker).

**Windows · macOS · Linux** · **MySQL · PostgreSQL** · MariaDB via the MySQL protocol · UI in Chinese / English

## Download

Official installers are on [GitHub Releases](https://github.com/henrybit/HiDataLinker/releases):

- macOS: `.dmg` (Apple Silicon / Intel)
- Windows: `.msi`
- Linux: `.deb` and AppImage

See [FEATURES.md](./FEATURES.md) for capabilities by version. Check the [website docs](https://hidatalinker.onesolo.app/docs) for engine limits.

---

## Vision

Traditional database tools help you *see* tables and columns; HiDataLinker is a free MIT workstation for everyday MySQL and PostgreSQL work — connect, browse, query, Dump, and migrate, with credentials that stay on your machine.

Optional relationship analysis and agent bridges are extras, not the product promise. Analysis can use LLM provider keys you configure; do not treat billed token packs or agent tasks as a required, fully productized core feature until the docs say otherwise.

Not another fuller Navicat — **a free daily client for MySQL and PostgreSQL, with optional analysis and agent tooling labeled as such**.

---

## How to use

1. **Download & install** — Pick Windows, macOS, or Linux. Ready to use after install; no license required.
2. **Create a connection** — Choose MySQL or PostgreSQL (MariaDB uses the same connection type as MySQL), fill in host details or paste a connection URL, test, then save.
3. **Browse & operate** — Expand databases / schemas in the object tree, open tables for data and structure, query / format / Explain in the SQL editor.
4. **Export & migrate** — Dump to `.sql` for backup or environment moves, or use Migration Center to move same-engine databases to a target connection.
5. **Turn on optional analysis when needed** — Run Relationship Network Analysis: pick a scope, optionally upload business docs (Markdown / Word / PDF); the LLM fills in comments and infers implicit links; export results as Markdown. Agent tasks are optional.

Everyday query and table work is free under MIT. Optional analysis uses LLM provider keys you configure; do not treat billed token packs or agent tasks as a required, fully productized core feature until the docs say otherwise.

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
- Platforms & locales: native Windows, macOS, and Linux desktop apps, Chinese / English UI
- MariaDB: same MySQL connection type, for everyday browsing and querying; MariaDB-specific features may not be fully covered

### Optional analysis & agent bridges

- **Relationship analysis** — relationship network analysis, missing-comment inference, and business grounding from uploaded docs, using LLM provider keys you configure
- **Optional agent bridges** — local agent / MCP-style flows are optional and off by default; not required for browse, query, Dump, or migrate

Connections and data stay on your machine; secrets stay local and are sent only to the model service you choose.

---

## vs traditional DB admin tools

| | Traditional tools (e.g. Navicat, DBeaver) | HiDataLinker |
|---|------------------------------------------|--------------|
| **Pricing** | Seat perpetual / subscription, or free builds with cut features | Free MIT daily client; optional analysis uses your own LLM keys |
| **Focus** | GUI breadth, sync, enterprise plugins | Daily GUI for MySQL and PostgreSQL |
| **AI** | Often add-on or missing | Optional relationship analysis with your own LLM keys; not required for daily SQL |
| **Agents** | Rarely present | Optional local MCP; not required for browse, query, Dump, or migrate |
| **Platforms** | Varies by vendor | Windows, macOS, and Linux |
| **Getting started** | Often pay first or pick a complex edition | Download and go; no license wall |

In short:

- Navicat sells a **professional GUI license**
- DBeaver sells **engine breadth and enterprise editions**
- HiDataLinker is a **free MIT daily client**; optional analysis and agent tooling stay labeled as such

Fuller positioning and competitor notes: [PRODUCT_POSITIONING.md](./PRODUCT_POSITIONING.md).

---

## Who it's for

- Individuals and small teams who want a free MIT client for everyday MySQL and PostgreSQL work
- People who also need to browse and query MariaDB over the MySQL protocol
- Mixed Windows, macOS, and Linux engineering & ops teams who want one tool with a consistent experience
- Teams who want optional relationship analysis with their own LLM keys, without making it required for daily SQL

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

Stars, issues, and suggestions are welcome.
