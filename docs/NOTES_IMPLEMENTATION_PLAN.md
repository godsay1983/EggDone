# EggDone 桌面端便签实现方案

## 1. 文档目标

本文档定义 EggDone 桌面端便签功能的产品边界、数据模型、同步协议、UI 入口和工程改动。配套开发顺序见 [NOTES_ROADMAP.md](NOTES_ROADMAP.md)。鸿蒙端必须遵守相同的便签同步协议，具体实现见 `D:\Develop\EggDoneHarmony\docs\HARMONY_NOTES_IMPLEMENTATION_PLAN.md`。

## 2. 产品目标

便签用于记录不需要完成状态、到期时间和提醒的文字信息。它与 Todo 平级，但保持 EggDone 轻量、离线和托盘常驻的定位。

第一版提供：

- 可选标题和纯文本正文。
- 新建、编辑、置顶、换色、搜索和删除。
- 本地自动保存。
- 使用现有 S3 / MinIO 配置跨设备同步。
- 与鸿蒙端互通。
- 亮色、暗色主题适配。

第一版不提供：

- 富文本和 Markdown。图片与附件作为后续独立扩展，见 [NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md](NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md)。
- 标签、文件夹和便签分享。
- Todo 与便签互相转换或关联。
- 多人协作、逐字段合并和版本历史。
- 端到端加密。便签内容与当前 Todo 一样，以 JSON 明文保存在用户配置的对象存储中。

## 3. 核心设计决策

### 3.1 便签使用独立数据模型

不把便签伪装成 Todo。Todo 的完成、提醒、重复和四象限语义不适用于便签，复用 Todo 会增加大量空字段和条件分支。

桌面端新增 `notes` 表、Note API、Note Store 和 Note UI。Todo 现有查询、排序和同步行为保持不变。

### 3.2 便签使用独立 S3 对象

当前 Todo 同步文档严格使用 `format_version = 1`。直接在 `todos.json` 中加入 `notes` 会产生两类风险：旧客户端可能拒绝新格式，也可能在重新上传时丢弃未知字段。

便签使用独立对象：

```text
eggdone/todos.json
eggdone/notes.json
```

自定义 Todo Object Key 时，两端使用同一推导规则：

1. 文件名为 `todos.json` 时，替换为 `notes.json`。
2. 其他带扩展名的文件，在扩展名前加入 `.notes`，如 `backup.json` 变为 `backup.notes.json`。
3. 无扩展名时，追加 `.notes.json`。

这样不需要新增一套 S3 凭据，也不会破坏旧版本的 Todo 同步。

### 3.3 整条便签参与冲突决胜

便签采用与 Todo 一致的最终一致性策略：

1. `updated_at` 较新的记录胜出。
2. 时间相同时，删除记录优先，避免便签复活。
3. 再比较 `updated_by`。
4. 最后比较规范化内容，保证两端得出相同结果。

第一版不做正文逐段合并。两台设备同时编辑同一便签时，整条较新的记录胜出。

## 4. 跨端同步协议

### 4.1 文档结构

```json
{
  "format_version": 1,
  "device_id": "00000000-0000-4000-8000-000000000000",
  "generated_at": 0,
  "notes": [
    {
      "uuid": "00000000-0000-4000-8000-000000000000",
      "title": "可选标题",
      "content": "便签正文",
      "color": "yellow",
      "pinned": false,
      "created_at": 0,
      "updated_at": 0,
      "deleted_at": null,
      "updated_by": "00000000-0000-4000-8000-000000000000"
    }
  ]
}
```

### 4.2 校验规则

- `format_version` 必须为 `1`。
- `device_id`、`uuid`、`updated_by` 必须是 UUID。
- `title` 最多 100 个 Unicode 字符。
- `content` 最多 20,000 个 Unicode 字符。
- `title` 和 `content` 不能同时为空。
- `color` 只能是 `default`、`yellow`、`pink`、`green`、`blue`。
- 时间戳不能为负数。
- 同一文档不能包含重复 UUID。

### 4.3 排序和决胜

列表展示排序不写入同步协议：

```text
pinned DESC, updated_at DESC, uuid ASC
```

冲突决胜顺序：

```text
updated_at
deleted_at 是否存在
updated_by
pinned
color
title
content
created_at
```

### 4.4 传输流程

- 复用现有 ETag、`If-Match`、冲突重试和单任务互斥锁。
- Todo 和 Note 在一次同步任务中顺序执行，但分别维护 ETag 和 dirty 状态。
- 远端不存在 `notes.json` 时按空文档处理，首次同步后创建。
- Todo 成功而 Note 失败时，首页仍显示 `待同步` 或 `同步失败`；状态详情明确指出便签未完成。
- 只有 Todo 和 Note 都成功后，整体状态才变为 `已同步`。
- 前台恢复、60 秒轻量检查、手动同步和下拉刷新都检查两个对象。

## 5. 本地数据模型

桌面端数据库从 migration 13 升到 14。

```sql
CREATE TABLE notes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uuid TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL DEFAULT '',
  content TEXT NOT NULL DEFAULT '',
  color TEXT NOT NULL DEFAULT 'default',
  pinned INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER,
  updated_by TEXT NOT NULL
);

CREATE INDEX idx_notes_active_order
  ON notes(deleted_at, pinned DESC, updated_at DESC);

CREATE INDEX idx_notes_updated_at
  ON notes(updated_at);
```

删除采用软删除。同步文档包含活动记录和墓碑；普通列表只查询 `deleted_at IS NULL`。

## 6. 桌面端工程设计

### 6.1 Rust 层

建议新增：

```text
src-tauri/src/notes.rs
src-tauri/src/note_sync.rs
```

职责：

- `db.rs`：注册 migration 14。
- `notes.rs`：定义 Note、列表、创建、更新、置顶、换色和软删除。
- `note_sync.rs`：构建、校验、合并和落库 NoteSyncDocument。
- `commands.rs`：只暴露 Tauri command，不堆放完整 Note SQL。
- `s3_sync.rs` / `sync.rs`：由现有同步编排器调用 Note 同步，不把 Note 塞入 Todo SyncDocument。
- `data_exchange.rs`：全量备份增加 `notes`，旧备份缺少该字段时使用空数组。

建议命令：

```text
list_notes
create_note
update_note
set_note_pinned
set_note_color
delete_note
prepare_note_sync_document
merge_note_sync_document
```

### 6.2 Svelte 层

建议新增：

```text
src/lib/api/noteApi.ts
src/lib/stores/noteStore.ts
src/lib/components/NoteList.svelte
src/lib/components/NoteCard.svelte
src/lib/components/NoteEditor.svelte
src/lib/noteTypes.ts
```

`TodoPanel.svelte` 只负责切换任务和便签内容，不直接执行 Note SQL 或同步合并。

## 7. 桌面 UI 方案

### 7.1 入口

在现有 `全部 / 今天 / 四象限 / 日历` 视图切换器末尾增加 `便签`。不在拥挤的 Header 中再增加图标。

进入便签模式后：

- 原快速新增任务区域替换为 `新建便签` 紧凑入口，占用相同高度。
- 隐藏任务分组、批量操作和任务筛选。
- 搜索入口搜索便签标题与正文。
- 支持 `Ctrl+Shift+N` 新建便签，`Ctrl+F` 搜索当前便签列表。

### 7.2 列表

桌面托盘面板较窄，采用单列卡片，不使用瀑布流。

卡片显示：

- 标题；无标题时显示正文第一行。
- 最多四行正文摘要。
- 更新时间。
- 置顶图标和主题化颜色边框。
- 一个“更多”入口，包含置顶、换色和删除。

点击卡片后，在当前面板内容区打开编辑层。编辑层保留应用 Header，避免新建额外 Tauri 窗口。

### 7.3 编辑和自动保存

- 标题可选，正文为多行文本框。
- 输入停止 600 毫秒后保存到 SQLite。
- `Ctrl+S`、失焦、返回和关闭编辑层时立即刷新未保存内容。
- 空白草稿不入库；已有便签被清空时要求删除或继续编辑。
- 本地保存成功后标记 Note dirty，再交给现有自动同步调度器合并上传。
- 编辑器显示 `正在保存`、`已保存到本地`、`待同步`、`已同步` 和失败信息。

### 7.4 视觉

- 卡片复用现有圆角和阴影尺度。
- 颜色使用语义令牌，不把原始十六进制颜色写入数据库。
- 暗色主题使用低饱和背景和清晰正文，不简单反转亮色颜色。
- 正文与背景对比度至少为 4.5:1。

## 8. 状态与错误处理

- Note Store 独立维护 `items`、`loading`、`saving` 和 `error`。
- 切换任务/便签模式不重复初始化数据库。
- 删除后提供短时撤销；撤销本质上清除本地墓碑并更新同步元数据。
- S3 未启用时便签仍完整离线可用。
- 同步失败不阻止继续编辑，本地保存状态和云同步状态分开显示。
- 搜索只在内存中的活动便签执行，不查询已删除墓碑。

## 9. 测试要求

### 单元测试

- migration 14 对新库和 migration 13 旧库都成功。
- Note CRUD、置顶、颜色、搜索和软删除。
- 协议校验、稳定排序和跨端决胜样例。
- `notes.json` Object Key 推导规则。
- 远端不存在、ETag 冲突和冲突重试。
- Todo 成功、Note 失败时整体同步状态不误报 `已同步`。

### 手动回归

- 窄托盘面板中五个视图入口不截断。
- 新建、快速切换、编辑自动保存和删除撤销。
- 亮色、暗色主题下五种卡片颜色可读。
- 桌面创建后鸿蒙端可见，反向编辑后桌面更新。
- 离线编辑后恢复网络可以同步。
- 旧版本桌面端继续同步 Todo，不影响新版本便签对象。

## 10. 完成标准

- Todo 现有行为和 `todos.json` 格式不变。
- 两端使用完全一致的 Note JSON 字段、校验和冲突决胜顺序。
- 便签离线可用，S3 同步失败不丢本地内容。
- 任务和便签同步都成功后才显示 `已同步`。
- `pnpm check`、`pnpm build`、`cargo fmt -- --check` 和 `cargo check` 通过。
- Windows 下完成真实托盘面板回归；macOS、Linux 不引入平台专属路径。
