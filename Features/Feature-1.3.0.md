# Feature-1.3.0

## English

> Version: `1.3.0` (in development, not released)  
> Relative to: minor update on `1.2.0`  
> Window: 2026-11-14 ~ 2026-11-15  
> Scope: new capabilities and behavior changes only. Full baseline: [Feature-1.0.0.md](./Feature-1.0.0.md). Previous: [Feature-1.2.0.md](./Feature-1.2.0.md).

### Theme

Beyond Agent Mode, bring AI back into the **database workbench**: ask from the editor, result grid, or object tree, then draft and validate SQL against schema. Deepen the **PostgreSQL** client — switch database on the same connection, visual EXPLAIN, Schema Health, a richer object tree, and one-click PostgreSQL 19 property-graph DDL from relationship analysis.

### Timeline

| Date | Feature | Notes |
|------|---------|-------|
| 2026-11-15 | AI fix after exec error | Hand result-area errors to Assist to rewrite SQL (JDS-206) |
| 2026-11-15 | Follow-ups & export in the same editor | Continue drafting; export results (JDS-38) |
| 2026-11-15 | Execution vs credentials boundary | Draft vs execute separated; audit sources distinguished (JDS-37) |
| 2026-11-15 | Visible draft steps, table confirm, optional Explain | Plan steps, table confirm, optional Explain (JDS-36) |
| 2026-11-15 | Schema-aware SQL draft & validate | Draft from connection metadata and validate (JDS-35) |
| 2026-11-15 | Workbench context entry points | Ask from editor, result grid, object tree (JDS-34) |
| 2026-11-14 | Relationship net → PG 19 property graph | Generate property-graph DDL from relationship analysis (JDS-205) |
| 2026-11-14 | Schema Health | Wiki “Don’t Do This”–style rules with fix SQL (JDS-204) |
| 2026-11-14 | Object tree completeness | Sequences / types / extensions / policies; nested partitions; refresh MVs (JDS-203) |
| 2026-11-14 | Visual EXPLAIN | ANALYZE / BUFFERS supported (JDS-202) |
| 2026-11-14 | Server messages in exec log | Capture RAISE NOTICE and related (JDS-201) |
| 2026-11-14 | Switch database on same connection | No new connection profile required (JDS-200) |
| 2026-11-14 | DDL / Dump / migrate fidelity | Virtual generated columns, partitions, comments, sequences preserved (JDS-198) |

### New capabilities

#### Workbench SQL Assist (JDS-34 ~ JDS-38, JDS-206)

- Open Assist from the **SQL editor, result grid, or object tree** (⌘/Ctrl+K) with connection, database, selection, or result summary as context.
- **Schema-aware drafting**: generate candidate SQL from locally cached table metadata, validate, then write into the editor.
- **Visible steps**: show the draft plan and candidate-table confirmation; optionally Explain the draft.
- **Execution boundary**: draft by default; execute uses the existing query path and authorization. Audit distinguishes “Assist draft” vs “actual execute”.
- **Follow-up & export**: continue in the same panel from the last draft or result; export results.
- **Fix after error**: after a failed run, send the error and statement to Assist to rewrite.

#### PostgreSQL: switch database (JDS-200)

- On an open PostgreSQL session, switch the current database from the top bar / connection context without creating another profile.

#### PostgreSQL: server messages (JDS-201)

- `RAISE NOTICE` and related server messages appear in the query execution log next to the result area.

#### PostgreSQL: visual EXPLAIN (JDS-202)

- Render Explain as a visual plan. Optional **ANALYZE** (run then roll back) and **BUFFERS** to find scans and buffer hotspots.

#### PostgreSQL: richer object tree (JDS-203)

- Object tree adds **sequences, types, extensions, policies**.
- Nested partitions; refresh materialized views from the tree.

#### PostgreSQL: Schema Health (JDS-204)

- Scan the current database against PostgreSQL wiki “Don’t Do This”–style rules, with severity and copy-paste fix SQL.

#### PostgreSQL 19: relationship net → property graph (JDS-205)

- After relationship analysis, generate **PostgreSQL 19** property-graph statements for tables in a single PostgreSQL database.

#### DDL / Dump / migrate fidelity (JDS-198)

- Fix loss of **virtual generated columns, partition definitions, comments, and sequences** on export / migrate paths so PG structure round-trips more completely.

### Positioning & boundaries

- Assist is for “write the right SQL in the workbench.” Multi-step analysis, attachment import, and Insights stay in **Agent Mode** (see 1.2.0).
- Keys and tool calls remain on the local Rust side; Assist and editor execution share audit and credential boundaries.

---

## 中文

> 版本：`1.3.0`（开发中，未发布）  
> 相对：`1.2.0` 的小版本功能更新  
> 周期：2026-11-14 ~ 2026-11-15  
> 说明：只记录本版新增与行为变化，完整基线见 [Feature-1.0.0.md](./Feature-1.0.0.md)，上一版见 [Feature-1.2.0.md](./Feature-1.2.0.md)。

### 本版主题

在智能体模式之外，把 AI 拉回 **数据库工作台**：编辑器 / 结果网格 / 对象树可直接提问，按 Schema 起草并校验 SQL。同时把 **PostgreSQL** 做成深度客户端——同连接切库、可视化执行计划、Schema 体检、对象树补齐，以及关系网一键生成 PostgreSQL 19 属性图。

### 更新时间线

| 日期 | 特性 | 说明 |
|------|------|------|
| 2026-11-15 | 执行报错后 AI 修复 | 结果区错误可一键交给 Assist 改写 SQL（JDS-206） |
| 2026-11-15 | 同一编辑器内追问与结果导出 | 草稿可继续追问；结果可导出（JDS-38） |
| 2026-11-15 | 执行权与凭据边界 | 起草与执行分权；审计区分来源（JDS-37） |
| 2026-11-15 | 起草步骤可见、候选表确认与按需 Explain | 计划步骤、表确认、可选 Explain（JDS-36） |
| 2026-11-15 | Schema 感知 SQL 起草与校验 | 基于连接元数据起草并校验（JDS-35） |
| 2026-11-15 | 工作台上下文入口 | 编辑器、结果网格、对象树提问（JDS-34） |
| 2026-11-14 | 关系网一键生成 PG 19 属性图 | 从关系分析结果生成属性图 DDL（JDS-205） |
| 2026-11-14 | Schema 体检 | 按 wiki「Don't Do This」等规则给出修复 SQL（JDS-204） |
| 2026-11-14 | 对象树补齐 | 序列 / 类型 / 扩展 / 策略；分区嵌套；物化视图刷新（JDS-203） |
| 2026-11-14 | 可视化执行计划 | 支持 ANALYZE / BUFFERS（JDS-202） |
| 2026-11-14 | 服务器消息进执行日志 | 捕获 RAISE NOTICE 等（JDS-201） |
| 2026-11-14 | 同一连接内切换数据库 | 无需新建连接即可切库（JDS-200） |
| 2026-11-14 | DDL / Dump / 迁移保真 | 虚拟生成列、分区、注释、序列不再丢失（JDS-198） |

### 新增能力

#### 工作台 SQL Assist（JDS-34 ~ JDS-38、JDS-206）

- 在 **SQL 编辑器、结果网格、对象树** 打开助手（快捷键 ⌘/Ctrl+K），携带当前连接、库、选中对象、选区或结果摘要作为上下文。
- **Schema 感知起草**：按本地缓存的表结构生成候选 SQL，并做基础校验后再写入编辑器。
- **可见步骤**：展示起草计划、候选表确认；可按需对草稿跑 Explain。
- **执行边界**：默认只起草不执行；执行走现有查询通道与授权；审计来源区分「助手起草」与「实际执行」。
- **追问与导出**：同一面板可基于上次草稿或结果继续提问；结果支持导出。
- **报错修复**：执行失败后可把错误与语句交给助手改写，减少往返改 SQL 的成本。

#### PostgreSQL：同连接切库（JDS-200）

- 已连接的 PostgreSQL 会话可在顶栏 / 连接上下文中切换当前数据库，不必另建连接配置。

#### PostgreSQL：服务器消息（JDS-201）

- `RAISE NOTICE` 等服务器消息写入查询执行日志面板，与结果区一并可见。

#### PostgreSQL：可视化执行计划（JDS-202）

- Explain 结果以可视化计划展示；可选 **ANALYZE**（实际执行后回滚）与 **BUFFERS** 信息，便于定位扫描与缓冲热点。

#### PostgreSQL：对象树补齐（JDS-203）

- 对象树新增 **序列、类型、扩展、策略**。
- 分区表支持嵌套展示；物化视图支持刷新操作。

#### PostgreSQL：Schema 体检（JDS-204）

- 按 PostgreSQL wiki「Don't Do This」等规则扫描当前库，列出问题严重程度，并给出可复制的修复 SQL。

#### PostgreSQL 19：关系网 → 属性图（JDS-205）

- 关系网分析完成后，可一键生成面向 **PostgreSQL 19** 的属性图（property graph）相关语句；仅包含同一库内的 PostgreSQL 表。

#### DDL / Dump / 迁移保真（JDS-198）

- 修复导出与迁移路径中丢失 **虚拟生成列、分区定义、注释、序列** 的问题，使 PG 结构往返更完整。

### 定位与边界

- Assist 面向「在工作台里写对 SQL」；复杂多步分析、附件导入、洞察推荐仍走 **智能体模式**（见 1.2.0）。
- 密钥与工具调用仍在本机 Rust 侧；Assist 与编辑器执行共用审计与凭据边界。
