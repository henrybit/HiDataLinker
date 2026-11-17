# Feature-1.2.0

## English

> Version: `1.2.0` (in development, not released)  
> Relative to: minor update on `1.1.0`  
> Window: 2026-10-24 ~ 2026-11-11  
> Scope: new capabilities and behavior changes only. Full baseline: [Feature-1.0.0.md](./Feature-1.0.0.md). Previous: [Feature-1.1.0.md](./Feature-1.1.0.md).

### Theme

Alongside the SQL workbench, add **Agent Mode**: analyze connected databases in natural language, with attachment import, charts, and Insights. Ship **SQL execution audit** so every statement run by humans or agents is traceable.

### Timeline

| Date | Feature | Notes |
|------|---------|-------|
| 2026-11-11 | SQL execution audit log | All session SQL written daily; filterable in Settings |
| 2026-11-10 | Agent sidebar collapse / expand | Icon rail; state persisted |
| 2026-11-10 | Chat chart export PNG | One-click image export |
| 2026-11-10 | Insights agent | Recommend analysis scenarios from connection metadata |
| 2026-11-05 | Higher context-window ceiling | Configurable up to 100M tokens |
| 2026-11-04 | Query execution log panel | Collapsible / resizable under the result grid |
| 2026-11-04 | Agent authorization mode | Read-only / full access; can run import scripts |
| 2026-11-04 | Import flow & process-message collapse | Wait for import completion; auto-collapse process messages |
| 2026-11-04 | CSV / Excel attachment import | Parse chat attachments; async import into MySQL / PostgreSQL |
| 2026-11-04 | Multimodal config & structured-output tolerance | Markdown model replies no longer hard-fail |
| 2026-11-03 | Chat context, skills, model config | History compression, context usage, slash skills, model switch |
| 2026-11-03 | Product manual & bilingual README | Added `PRODUCT_MANUAL.md`; README defaults to English |
| 2026-11-02 | Local schema cache & runtime logs | 24h schema refresh; daily logs kept 7 days |
| 2026-11-02 | Agent output follows UI language | Chinese / English UI → matching replies |
| 2026-11-02 | Agent orchestration in Node sidecar | Orchestration leaves the WebView; keys stay on Rust |
| 2026-11-02 | Top-bar “More” menu | Secondary actions collapsed |
| 2026-10-31 | Agent Mode | Toggle Database / Agent in the top bar |
| 2026-10-24 ~ 10-26 | Automated packaging | macOS DMG (signed + notarized), Windows MSI, Linux deb / AppImage |

### New capabilities

#### Agent Mode (2026-10-31)

- Toggle **Database mode / Agent mode** next to the title. Agent Mode reuses existing connections and MCP state.
- Agent Mode includes chat, analysis, data sources, Insights, and history.

#### Agent runtime (2026-11-02)

- Built-in data-analyst agent moves from the WebView into a Rust-hosted Node sidecar (stdio), concurrent-capable.
- Tool calls and LLM keys stay on the Rust side; the UI only streams progress.
- Plans, notes, and final reports follow the app language (Chinese / English).

#### Schema cache & runtime logs (2026-11-02)

- Connection schemas cache locally and refresh on a 24-hour TTL at startup and in chat.
- Runtime logs write daily and keep 7 days; Settings uses tabbed pages.

#### Chat experience (2026-11-03)

- Long chats compress history; the input shows context usage.
- Type `/` for skills; optional skill allowlist.
- Switch LLM mid-chat; models labeled text / multimodal.
- Sessions persist across restarts; charts render inline.
- Installer ships a bundled Node runtime.

#### CSV / Excel import in chat (2026-11-04)

- Attach CSV / Excel in chat and import into MySQL / PostgreSQL.
- Only the first 20 rows are sampled for structure — the full file never goes to the LLM.
- Review the import script before async background execution.
- The agent waits for the user’s import decision and job completion; existing tables are truncated then loaded.
- After a turn, progress / tool-call process messages auto-collapse.

#### Agent authorization (2026-11-04)

- Authorization next to the input: **Read-only** (default) / **Full access**.
- Full access allows write SQL and writing / running / listing import scripts under `~/.hidatalinker/temp/scripts`.

#### Model configuration (2026-11-04 ~ 11-05)

- Multimodal model configuration.
- Markdown (non-JSON) model replies are tolerated instead of hard failures.
- Context-window ceiling raised to 100M tokens for long-context models.

#### Query execution log panel (2026-11-04)

- In Database mode, the execution log docks under the result grid — collapse / expand and resize.

#### Insights agent (2026-11-10)

- Background pass over connection metadata builds a short business profile per source, then recommends up to 10 analysis scenarios.
- Each insight has severity (high / medium / low) and category (business, quality, performance, governance, integration), plus connections and tables.
- One-click “Analyze in chat”, mark handled, or re-analyze; runs in the background.

#### Chart export PNG (2026-11-10)

- Chat charts gain “Export image”: ECharts at 2× on white; AntV G2 via composed canvas.
- Desktop save dialog; browser download.

#### Agent sidebar collapse (2026-11-10)

- Collapse to a 56px icon rail (tooltips on hover; connection status dots).
- Collapse state is remembered.

#### SQL execution audit (2026-11-11)

- All session SQL is logged: editor runs, table browse / count, create / drop DB, export, migrate, import, agent tool calls.
- Daily files under `~/.hidatalinker/audit/YYYY-MM-DD.jsonl` with time, OS user, connection, database, source, full SQL, result, rows, duration, error.
- Password literals redacted; statements over 256KB truncated and flagged.
- Settings → **SQL Audit**: on/off, retention (default 90 days), filters by date / connection / source / status / keyword; failures in red.

### UX polish

- Secondary top-bar actions move into “More” (2026-11-02).
- Mode switch moves next to the title so it does not cover the menu (2026-10-31).

### Release & install (2026-10-24 ~ 2026-10-26)

- Version tags trigger GitHub Actions Release builds: macOS DMG, Windows MSI, Linux deb / AppImage.
- macOS packages are Developer ID–signed and notarized so Gatekeeper no longer flags “damaged” downloads from other hosts.

### Docs (2026-11-03)

- Added product manual `PRODUCT_MANUAL.md`.
- README defaults to English; Chinese in `README.zh-CN.md`.

---

## 中文

> 版本：`1.2.0`（开发中，未发布）  
> 相对：`1.1.0` 的小版本功能更新  
> 周期：2026-10-24 ~ 2026-11-11  
> 说明：只记录本版新增与行为变化，完整基线见 [Feature-1.0.0.md](./Feature-1.0.0.md)，上一版见 [Feature-1.1.0.md](./Feature-1.1.0.md)。

### 本版主题

在数据库工作台之外新增 **智能体模式**：用自然语言分析已连接的数据库，并支持附件导入、图表、洞察推荐。同时补齐 **SQL 执行审计**，让人和智能体执行过的每条 SQL 都能追溯。

### 更新时间线

| 日期 | 特性 | 说明 |
|------|------|------|
| 2026-11-11 | SQL 执行审计日志 | 所有会话的 SQL 按天落盘，设置中可筛选查看 |
| 2026-11-10 | 智能体侧边栏收起/展开 | 收起为图标栏，状态持久化 |
| 2026-11-10 | 对话图表导出 PNG | 图表一键导出为图片 |
| 2026-11-10 | 洞察智能体 | 根据连接元数据推荐分析场景 |
| 2026-11-05 | 上下文窗口上限提升 | 模型上下文窗口可配置到 1 亿 token |
| 2026-11-04 | 查询执行日志面板 | 结果区下方可收起、可拖拽调整高度 |
| 2026-11-04 | 智能体授权模式 | 只读 / 完全授权切换，可执行导入脚本 |
| 2026-11-04 | 导入流程与过程消息折叠 | 等待导入完成再继续，过程消息自动折叠 |
| 2026-11-04 | CSV / Excel 附件导入 | 对话附件解析并异步导入 MySQL / PostgreSQL |
| 2026-11-04 | 多模态配置与结构化输出容错 | 模型返回 Markdown 时不再直接失败 |
| 2026-11-03 | 对话上下文、技能与模型配置 | 历史压缩、上下文用量、斜杠技能、模型切换 |
| 2026-11-03 | 产品说明书、中英文 README | 新增 `PRODUCT_MANUAL.md`，README 默认英文 |
| 2026-11-02 | Schema 本地缓存与运行日志 | 24 小时刷新 schema，日志按天保留 7 天 |
| 2026-11-02 | 智能体输出跟随界面语言 | 中英文界面对应中英文回复 |
| 2026-11-02 | 智能体编排迁到 Node 子进程 | 编排离开 WebView，密钥留在 Rust 侧 |
| 2026-11-02 | 顶栏「更多」菜单 | 次要操作收起，缓解顶栏拥挤 |
| 2026-10-31 | 智能体模式 | 顶栏切换数据库 / 智能体两种模式 |
| 2026-10-24 ~ 10-26 | 自动打包发布 | macOS DMG（签名公证）、Windows MSI、Linux deb / AppImage |

### 新增能力

#### 智能体模式（2026-10-31）

- 顶栏标题旁可切换 **数据库模式 / 智能体模式**，智能体模式直接使用已有连接和 MCP 状态。
- 智能体模式包含对话、分析、数据源、洞察、历史等页面。

#### 智能体运行架构（2026-11-02）

- 内置的数据分析智能体（data-analyst）从 WebView 迁到由 Rust 托管的 Node 子进程，通过 stdio 通信，可并发运行。
- 工具调用和 LLM 密钥只在 Rust 侧，界面只订阅进度。
- 智能体的计划、备注、最终报告跟随应用界面语言（中文 / 英文）。

#### Schema 缓存与运行日志（2026-11-02）

- 连接的 schema 缓存在本地，启动和对话时按 24 小时有效期刷新，减少重复读取元数据。
- 运行日志按天写入，保留 7 天；设置对话框改为 Tab 分页管理。

#### 对话体验（2026-11-03）

- 长对话自动压缩历史，输入区显示上下文用量进度。
- 输入 `/` 选择技能，可设置技能白名单。
- 对话中可切换 LLM，并标注模型类型（文本 / 多模态）。
- 会话持久化，重开应用后可继续历史对话；回复中的图表直接渲染。
- 安装包内置 Node 运行时，用户无需另装。

#### CSV / Excel 附件导入（2026-11-04）

- 对话中可上传 CSV / Excel 附件，导入到 MySQL / PostgreSQL。
- 只取前 20 行采样推断表结构，整份文件不发给 LLM。
- 导入脚本先展示给用户检视确认，再在后台异步执行，界面不卡顿。
- 智能体会等待用户的导入决定和导入任务完成后再继续；导入已存在的表时先清空再写入。
- 一轮对话结束后，进度和工具调用等过程消息自动折叠。

#### 智能体授权模式（2026-11-04）

- 输入框前新增授权模式切换：**只读**（默认）/ **完全授权**。
- 完全授权下允许执行写入 SQL，并可编写、运行、列出导入脚本；脚本保存在 `~/.hidatalinker/temp/scripts`。

#### 模型配置（2026-11-04 ~ 11-05）

- 完善多模态模型配置。
- 模型返回 Markdown 而非结构化 JSON 时，自动容错，不再直接报错。
- 模型上下文窗口上限提升到 1 亿 token，以适配超长上下文模型。

#### 查询执行日志面板（2026-11-04）

- 数据库模式的查询编辑器中，执行日志停靠在结果区下方，可收起 / 展开，可拖拽调整高度。

#### 洞察智能体（2026-11-10）

- 后台读取已有连接的元数据，先给每个数据源生成业务画像（领域、概要、关键表），再推荐最多 10 条分析场景。
- 每条洞察带严重程度（高 / 中 / 低）和分类（业务分析、数据质量、性能、治理、集成），以及涉及的连接和表。
- 可一键「在对话中分析」、标记已处理，或点击「重新分析」刷新；后台运行，不影响其他操作。

#### 图表导出 PNG（2026-11-10）

- 对话中的图表新增「导出图片」按钮：ECharts 以 2 倍分辨率白底导出，AntV G2 合成画布后导出。
- 桌面端弹出保存对话框选择路径，浏览器环境直接下载。

#### 智能体侧边栏收起（2026-11-10）

- 侧边栏可收起为 56px 图标栏：导航只显示图标（悬停显示名称），数据源显示连接状态圆点。
- 收起状态会被记住，下次打开保持一致。

#### SQL 执行审计（2026-11-11）

- 所有连接会话的 SQL 统一记录，覆盖：查询执行、表数据浏览 / 计数、建库 / 删库、导出、迁移、导入、智能体工具调用。
- 按天写入 `~/.hidatalinker/audit/YYYY-MM-DD.jsonl`，记录时间、系统用户、连接、库、来源、完整 SQL、结果、影响 / 返回行数、耗时、错误。
- 密码字面量自动脱敏；单条 SQL 超过 256KB 截断并标记。
- 设置新增「SQL 审计」页：开关、保留天数（默认 90 天），按日期、连接、来源、状态、关键字筛选并查看详情；失败原因红色显示。

### 体验优化

- 顶栏次要操作收入「更多」菜单（2026-11-02）。
- 模式切换移到标题旁，避免遮挡顶栏菜单（2026-10-31）。

### 发布与安装（2026-10-24 ~ 2026-10-26）

- 推送版本标签后，GitHub Actions 自动构建并发布 Release：macOS DMG、Windows MSI、Linux deb / AppImage。
- macOS 安装包使用 Developer ID 签名并公证，从其他站点下载后不再被 Gatekeeper 标为“已损坏”。

### 文档（2026-11-03）

- 新增产品说明书 `PRODUCT_MANUAL.md`。
- README 默认英文，另附中文版 `README.zh-CN.md`。
