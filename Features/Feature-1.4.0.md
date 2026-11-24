# Feature-1.4.0

## English

> Version: `1.4.0` (in development, not released)  
> Relative to: minor update on `1.3.0`  
> Window: 2026-11-23 ~  
> Scope: new capabilities and behavior changes only. Full baseline: [Feature-1.0.0.md](./Feature-1.0.0.md). Previous: [Feature-1.3.0.md](./Feature-1.3.0.md).

### Theme

Show plaintext in the workbench without writing it back. After a query or a table/view open, selected columns can be **Base64 / hex / URL decoded** or **AES decrypted** in place. Rules stay on that tab.

### Timeline

| Date | Feature | Notes |
|------|---------|-------|
| 2026-11-23 | Decode and decrypt result fields | Per-tab display transform for query results and table data (JDS-224) |

### New capabilities

#### Decode and decrypt result fields (JDS-224)

- On a **query tab** or a **table / view data** view, open **Decode fields** and add one or more column rules. A rule stays editable until it is checked; checking it applies the transform and locks the options as read-only. Uncheck to edit again.
- Operations: **Base64 decode**, **hex decode** (optional `0x`), **URL decode**, and **AES decrypt** (ECB / CBC / GCM). Ciphertext is Base64 or hex. The key is text, hex, or Base64 and must be 16, 24, or 32 bytes after encoding. CBC uses a 16-byte IV (empty means 16 zero bytes, or the IV can be a prefix of the ciphertext). GCM uses a 12-byte IV or a 12-byte prefix, with the 16-byte tag appended to the ciphertext.
- Rules on the same column run **top to bottom**. A cell that cannot be decoded stays as stored and is counted. Null and empty cells are skipped.
- The grid shows a decoded mark on transformed columns. The decode panel height can be **dragged**; the result area keeps a minimum height.
- **Display only.** Edit and delete still use the stored cell, so plaintext is not written back. AES keys live in the tab session and are dropped when the tab, connection, or database is closed. Cell values are not sent to Assist.
- Query-result CSV export uses the decoded view.

### Positioning & boundaries

- Transforms are local and per tab. They are not saved to disk and are not a database-side decrypt.
- Keys are not derived (no hashing). A key that is not exactly 16, 24, or 32 bytes is rejected.

---

## 中文

> 版本：`1.4.0`（开发中，未发布）  
> 相对：`1.3.0` 的小版本功能更新  
> 周期：2026-11-23 ~  
> 说明：只记录本版新增与行为变化，完整基线见 [Feature-1.0.0.md](./Feature-1.0.0.md)，上一版见 [Feature-1.3.0.md](./Feature-1.3.0.md)。

### 本版主题

在工作台里直接看明文，但不写回数据库。查询执行后，或打开表 / 视图数据后，可以对指定列做 **Base64 / hex / URL 解码** 或 **AES 解密**。规则只在当前标签页生效。

### 更新时间线

| 日期 | 特性 | 说明 |
|------|------|------|
| 2026-11-23 | 结果字段解码与解密 | 查询结果和表数据按标签配置展示转换（JDS-224） |

### 新增能力

#### 结果字段解码与解密（JDS-224）

- 在 **查询标签** 或 **表 / 视图数据** 中打开「字段解码」，为列添加一条或多条规则。规则在勾选前保持可编辑；勾选后才作用到结果，选项变为只读。取消勾选可再编辑。
- 操作包括 **Base64 解码**、**hex 解码**（可带 `0x`）、**URL 解码**，以及 **AES 解密**（ECB / CBC / GCM）。密文为 Base64 或 hex。密钥可为文本、hex 或 Base64，编码后必须是 16、24 或 32 字节。CBC 的 IV 为 16 字节（留空则用 16 个 0，也可把 IV 放在密文前面）。GCM 的 IV 为 12 字节或密文前 12 字节，16 字节 tag 附在密文末尾。
- 同一列上的多条规则 **从上到下** 依次执行。无法解码的单元格保持原值并计数；空值和空字符串跳过。
- 转换过的列在表头标出。解码面板高度可 **拖拽**，结果区保留最小高度。
- **只改展示。** 编辑和删除仍使用库里的原始单元格，明文不会写回。AES 密钥只留在当前标签会话，关闭标签、连接或数据库后清除。单元格内容不会发给 Assist。
- 查询结果导出 CSV 时使用解码后的展示结果。

### 定位与边界

- 转换在本机、按标签生效，不落盘，也不是数据库侧解密。
- 密钥不做哈希派生。编码后不是 16、24 或 32 字节的密钥会被拒绝。
