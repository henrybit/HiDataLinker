# Feature-1.0.0

## English

> Version: `1.0.0`  
> Product: HiDataLinker  
> Scope: capability baseline for the first GA release. Later minors record deltas only — see other `Feature-x.y.z.md` files in this folder.

### Positioning

A free Navicat-style desktop database client. Connect, browse, query, change structure, SQL, Dump, and same-engine migrate all run locally. LLM relationship analysis is billed by token. Windows / macOS; UI language Chinese / English.

### Engines

| Engine | In this release |
|--------|-----------------|
| MySQL | Connect, object browse, query, DDL changes, Dump, migrate |
| PostgreSQL | Same; object tree expands by Schema |
| SQL Server | Connect, object browse, query, relationship analysis |
| Oracle | Connect (SID; 11g and below need the official Client installed locally), object browse, query, relationship analysis |

Connections accept pasted URLs (`mysql://`, `postgres://`, etc.). Passwords may be entered manually and not persisted. Up to 5 concurrent sessions. PostgreSQL supports SSL certs; Oracle / SQL Server can skip cert verification. SSH tunnels are out of scope for this release.

### Everyday workbench

- **Connections**: save / edit / delete / test / connect / disconnect; password prompt on auth failure.
- **Object tree**: databases / schemas, tables, views, indexes, triggers, functions & procedures; lazy load; context menus for connect, create DB, Dump, run SQL file, open data & DDL.
- **Table / view workspace**: Data, Structure, DDL tabs. Paginated preview (page size 100), grid and form views, insert / edit / delete by primary key. Columns can be added, edited, deleted.
- **Data grid**: virtual scroll beyond 80 rows; NULL markers; long-cell detail drawer; max 5,000 rows per query; default LIMIT 500 when none is written.
- **SQL editor**: highlight, format, Explain, execution log; `⌘/Ctrl+Enter` to run. Open and run SQL files (max 8 MB). Scripts survive tab switches; very large scripts spill to disk cache.
- **DB ops**: create / drop database or schema; Dump export (optional data); run SQL files.

### Migration center

Same-engine full-database migrate: pick or create source and target in the panel, check connectivity before start, per-table progress, local history for review and delete.

### Smart analysis

- Toolbar **Analyze** opens relationship analysis: physical FKs + view deps + LLM-inferred links, comments, graph, Markdown export.
- Upload Markdown / Word / PDF to feed business context.
- Configure LLM providers in Settings (OpenAI / Anthropic-compatible endpoints); keys stay on device.
- Analysis jobs write to local history.

### UI

Toolbar: New Connection, Connect, Disconnect, Create DB, New Query, Refresh, Analyze, Migrate, Settings, language toggle. Resizable object tree / work area; tabs keep panels mounted to avoid reload jank.

### Out of scope this release

- SSH tunnels, full object wizards (visual create table / index), restore wizard, CSV import / export
- Query history, saved scripts, connection groups, theme switch
- External agents via Skill / MCP (see `1.1.0`)

---

## 中文

> 版本：`1.0.0`  
> 产品：HiDataLinker  
> 说明：首个正式版本的能力基线。后续小版本只记录相对本版的增量，见同目录其他 `Feature-x.y.z.md`。

### 本版定位

免费的 Navicat 风格桌面数据库客户端。连接、浏览、查数、改结构、SQL、Dump、同引擎迁移均在本机完成；大模型关系网分析按 Token 计费。Windows / macOS 双端，界面中英切换。

### 引擎

| 引擎 | 本版能力 |
|------|----------|
| MySQL | 连接、对象浏览、查询、结构变更、Dump、迁移 |
| PostgreSQL | 同上；对象树按 Schema 展开 |
| SQL Server | 连接、对象浏览、查询、关系网分析 |
| Oracle | 连接（含 SID、11g 及以下需本机安装官方 Client）、对象浏览、查询、关系网分析 |

连接支持粘贴 URL（`mysql://`、`postgres://` 等）。密码可选手动输入、不落盘。最多 5 个并发会话。PostgreSQL 可配置 SSL 证书；Oracle / SQL Server 可跳过证书检测。SSH 隧道未纳入本版。

### 日常工作台

- **连接管理**：保存 / 编辑 / 删除 / 测试 / 连接 / 断开；认证失败时弹出密码框。
- **对象树**：库 / Schema、表、视图、索引、触发器、函数与存储过程；按需加载；右键菜单覆盖连接、建库、Dump、运行 SQL 文件、打开数据与 DDL。
- **表 / 视图工作区**：数据、结构、DDL 三个子页。数据分页预览（页大小 100），表格视图与表单视图，按主键插入 / 编辑 / 删除行。列可添加、编辑、删除。
- **数据网格**：超过 80 行虚拟滚动；NULL 标识；长单元格详情抽屉；单次查询最多返回 5,000 行，未写 LIMIT 时默认限制 500 行。
- **SQL 编辑器**：高亮、格式化、Explain、执行日志；`⌘/Ctrl+Enter` 运行。可打开并执行 SQL 文件（上限 8 MB）。查询脚本在页签切换时保留，超大脚本落到磁盘缓存。
- **库运维**：创建 / 删除库或 Schema；Dump 导出（可选含数据）；运行 SQL 文件。

### 迁移中心

同引擎整库迁移：在面板里选择或新建源、目标连接，启动前检查连通性，按表显示进度，任务写入本地历史，可回看与删除。

### 智能分析

- 工具栏「分析」打开关系网分析：物理外键 + 视图依赖 + 大模型推断隐式关联，补注释，画关系图，导出 Markdown。
- 可上传 Markdown / Word / PDF，把业务说明喂进分析。
- 设置里配置大模型 provider（OpenAI / Anthropic 兼容地址），密钥只保存在本机。
- 分析任务写入本地历史。

### 界面

工具栏：新建连接、连接、断开、创建库、新建查询、刷新、分析、数据迁移、设置、中英切换。左侧对象树与右侧工作区分栏可拖动；页签切换不卸载面板，避免重载卡顿。

### 本版未纳入

- SSH 隧道、完整对象向导（可视化建表 / 索引）、还原向导、CSV 导入导出
- 查询历史、已保存脚本、连接分组、主题切换
- 外部智能体通过 Skill / MCP 操作本客户端（见 `1.1.0`）
