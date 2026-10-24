# Feature-1.1.0

> 版本：`1.1.0`  
> 相对：`1.0.0` 的小版本功能更新  
> 说明：只记录本版新增与行为变化，完整基线见 [Feature-1.0.0.md](./Feature-1.0.0.md)。

---

## 本版主题

把正在运行的桌面客户端开放给外部智能体：其他 Agent 宿主通过可安装 Skill 和本机 MCP，操作 **本应用里已经连上的会话**，而不是自带数据库驱动。密码和连接串不离开本机。

---

## 新增能力

### 本机 MCP 服务

- 应用启动后在 `127.0.0.1` 打开网关，令牌写到 `~/.hidatalinker/agent/endpoint.json`（权限 `600`）。
- 默认关闭。关闭后调用返回 `MCP service is turned off in HiDataLinker settings`；端口保持监听，再次打开即可恢复，不必重启应用。
- 成功调用记入环形审计（最近 40 条）。

### 设置与活动条开关

- 设置对话框顶部增加 **MCP 服务** 区块：说明、本机端口、苹果风格 Switch。
- 底部活动条使用同一开关和同一状态；开启后轨道与文案为绿色「已开启」，关闭为灰色「已关闭」。
- 浏览器预览里开关禁用，提示在桌面版操作。

### 只读工具目录

外部智能体可调用：

`list_connections` · `connect` · `list_databases` · `list_tables` · `list_views` · `list_indexes` · `list_triggers` · `list_routines` · `get_columns` · `get_ddl` · `preview_table` · `query`

约束：

- 连接列表只给 id、名称、引擎、主机、端口、是否已连接，不含密码。
- `connect` 只使用已保存密码，不接受密码参数。
- `query` / `preview_table` 只允许单条只读语句（`SELECT` / `WITH` / `EXPLAIN` / `SHOW` / `DESCRIBE` 等）；写入、导出文件、`FOR UPDATE` 等会被拒绝。
- 预览行数上限 1,000。

### Skill 与 MCP 进程

- 仓库内 `skills/hidatalinker-db/`：触发条件、工具说明、安全约束、MCP 启动脚本。
- 独立二进制 `agent-mcp` 走 stdio，把工具调用转到本机网关；桌面主程序同时支持 `--agent-mcp`。
- `Cargo.toml` 指定 `default-run = "app"`，避免与 `agent-mcp` 并存时 `tauri dev` 找不到主程序。

### 会话联动

智能体执行 `connect` 后，界面对象树通过 `agent-session-changed` 刷新，人与 Agent 看到同一组会话。

---

## 本版未纳入

写操作、Dump、迁移、连接增删改仍不走 MCP。后续若增加 `mutate`，需要应用内审批。
