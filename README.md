# HiDataLinker

**Free desktop DB client. AI that actually understands your schema.**

免费的桌面数据库客户端 —— 日常管理零门槛；需要读懂库结构、补文档、跑智能任务时，再按量付费。

**Windows · macOS** ｜ **MySQL · PostgreSQL · SQL Server · Oracle** ｜ 中 / 英界面

## 下载

正式安装包在 [GitHub Releases](https://github.com/henrybit/db-gui/releases)：

- macOS：`.dmg`（Apple Silicon / Intel）
- Windows：`.msi`

各版本能力见 [FEATURES.md](./FEATURES.md)。

---

## 愿景

传统数据库工具帮你「看见」表和字段；HiDataLinker 还要帮你「读懂」库。

我们相信：连接、查询、改数、迁移应当免费且好用；而关系推断、业务注释、智能体代办这类高价值能力，应按使用量付费 —— **先干活，智能才付钱**。

不是再做一个更全的 Navicat，而是：**免费日常客户端 + 真正做库理解与智能体的那一档。**

---

## 如何使用

1. **下载安装** — 选择 Windows 或 macOS 版本，安装后即可使用，无需许可证。
2. **新建连接** — 选择引擎，填写主机信息（也可粘贴连接 URL），测试通过后保存。
3. **浏览与操作** — 在对象树中展开库 / Schema，打开表查看数据与结构，用 SQL 编辑器查询、格式化、Explain。
4. **导出与迁移** — 需要备份或换环境时，Dump 为 `.sql`，或在迁移中心把同引擎库迁到目标连接。
5. **需要智能时再打开** — 运行「关系网分析」：选范围、可选上传业务文档（Markdown / Word / PDF），由大模型补注释、推断隐式关联，结果可导出 Markdown；智能体任务按次计费。

日常查库改表完全免费；只有调用大模型或智能体时才产生费用。

---

## 支持的能力

### 基础管理（免费）

- 连接管理：保存 / 编辑 / 测试 / 连接，密码可选手动输入不落盘
- 对象浏览：库 / Schema、表、视图、索引、触发器、函数与存储过程
- 数据操作：分页预览、表格 / 表单视图、行插入 / 编辑 / 删除
- 结构与 DDL：列级修改、查看建表语句
- SQL 工作台：执行、格式化、Explain，快捷键运行
- 库运维：创建 / 删除库或 Schema，Dump 导出，运行 SQL 文件
- 同引擎迁移：源 → 目标可视化迁移，进度与历史可追溯
- 多端与多语言：Windows / macOS 原生桌面版，中英切换

### 智能增值（按量）

- **大模型接入（Token 计费）** — 关系网分析、缺失注释推断、结合文档做业务 grounding
- **智能体接入（任务计费）** — 以任务为单位完成分析等智能流程；后续可扩展运维 / 文档类 Agent

连接与数据在本机处理；密钥仅保存在本地，并只发给你选择的模型服务。

---

## 与传统数据库管理工具的区别

| | 传统工具（如 Navicat、DBeaver） | HiDataLinker |
|---|-------------------------------|--------------|
| **收费方式** | 席位买断 / 订阅，或免费版砍功能 | 客户端免费；智能按 Token / 任务计费 |
| **产品重心** | GUI 广度、同步、企业插件 | 日常 GUI + **读懂库结构** |
| **AI** | 多为附属或缺失 | 一等公民：关系网、注释、文档增强 |
| **智能体** | 基本没有 | 任务制接入，用多少付多少 |
| **平台** | 各家不同 | Windows + macOS，体验对齐 |
| **上手** | 常先付钱或先选复杂版本 | 下载即用，无授权墙 |

一句话：

- Navicat 卖的是**专业 GUI 许可证**
- DBeaver 卖的是**引擎广度与企业版**
- HiDataLinker 卖的是**免费入口 + AI / Agent 用量**

更完整的定位与竞品分析见 [PRODUCT_POSITIONING.md](./PRODUCT_POSITIONING.md)。

---

## 适合谁

- 个人与小团队：想免费替代付费客户端做日常查库改表
- 中小企业：遗留库缺文档、关系不清，希望用 AI 快速产出关系网与说明
- 混用 Windows / Mac 的研发运维：一套工具、双端一致

---

## 开发说明（智能体 sidecar）

内置数据分析 Agent 的 LangGraph 编排跑在 Node 子进程 `agents-runtime`（由 Rust 经 stdio NDJSON 托管），工具与 LLM 密钥仍只在 Rust 侧。

```bash
pnpm agents-runtime:build   # 产出 agents-runtime/dist/main.mjs
pnpm tauri:dev              # beforeDevCommand 会先构建 runtime
```

开发机与当前发行包需要本机可用的 `node`（可用 `HIDATALINKER_NODE` 指定）。后续计划用 Tauri `externalBin` 嵌入平台 Node。

---

欢迎 Star、提 Issue 与建议。一起把「能用的免费客户端」做成「能读懂你的库」的智能工作台。
