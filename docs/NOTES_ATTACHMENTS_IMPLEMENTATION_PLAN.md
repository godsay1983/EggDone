# EggDone 桌面端便签图片与附件实现方案

## 1. 文档目标

本文档定义 EggDone 桌面端便签图片与附件功能的产品边界、跨端协议、S3 对象布局、本地持久化、同步顺序、UI 和验证要求。配套开发顺序见 [NOTES_ATTACHMENTS_ROADMAP.md](NOTES_ATTACHMENTS_ROADMAP.md)。鸿蒙端必须使用相同的元数据字段、对象路径和冲突决胜规则，具体实现见 `D:\Develop\EggDoneHarmony\docs\HARMONY_NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md`。

本功能扩展现有便签能力，不修改 `todos.json` 和 `notes.json` 的格式。

## 2. 产品目标与边界

### 2.1 第一阶段：图片 MVP

- 从文件选择器向便签添加 JPEG、PNG 和 WebP 图片。
- 保留原始图片字节，并生成跨端可显示的 JPEG 预览图。
- 在便签编辑器中展示图片网格，支持查看、删除、重试和下载原图。
- 图片可以离线添加；网络恢复后自动上传。
- 桌面端和鸿蒙端可以互相查看、增加和删除图片。
- 单张原图上限 10 MiB，每条便签最多 9 张活动图片。
- 远端 HEIC/HEIF 图片使用同步的 JPEG 预览展示，原图允许下载；桌面端第一阶段不承诺直接解码 HEIC/HEIF 原图。

### 2.2 第二阶段：普通附件

- 支持 PDF、TXT、Markdown、DOCX、XLSX、PPTX 和 ZIP 等普通文件。
- 展示文件名、类型、大小、同步状态和下载状态。
- 支持使用系统默认程序打开和另存为。
- 单文件上限 20 MiB；第一版不支持可执行文件和脚本文件。

### 2.3 暂不包含

- 富文本正文中的任意位置图片、图文混排和图片标题。
- 视频、音频预览。
- 超过 20 MiB 文件的 S3 分片上传和断点续传。
- 多人共享、外链分享和服务端转码。
- 端到端加密。附件与现有同步数据一样存放在用户配置的 S3 / MinIO 中。

## 3. 核心设计决策

### 3.1 不在 JSON 中保存 Base64

图片和附件不得以 Base64 或字节数组写入 `notes.json`。原因：

- Base64 会增加体积，并让每次文字修改重新上传所有附件。
- 当前同步按完整 JSON 对象执行 ETag 合并，大对象会增加内存、网络和冲突成本。
- 二进制对象无法按需下载，也无法独立重试和清理。

`notes.json` 继续只保存便签文字。附件元数据和二进制文件分别存储。

### 3.2 附件使用独立元数据对象

新增 `note-attachments.json`，使用 `format_version = 1`。旧客户端继续读写 `notes.json`，不会删除或改写附件元数据。

附件二进制对象使用不可变 UUID 路径。创建后不覆盖原对象；修改附件等同于删除旧附件并创建新 UUID。

### 3.3 附件不是富文本节点

正文仍为纯文本。附件显示在正文下方的独立区域，通过 `note_uuid` 归属便签，通过 `sort_order` 排序。这样可以保持现有编辑器、字符上限和便签冲突规则不变。

### 3.4 本地写入优先

添加附件时先复制到应用数据目录并写入 SQLite，再交给同步队列。关闭窗口、离线和上传失败都不能丢失用户选择的文件。

### 3.5 保持空白便签规则兼容

现有 `notes.json` v1 要求标题和正文不能同时为空。为避免修改协议：

- 空白草稿首次添加附件时，使用第一个文件名去除扩展名后生成可见、可编辑的标题；无法得到名称时使用 `图片便签` 或 `附件便签`。
- 便签存在活动附件时，不允许标题和正文同时清空；编辑器必须提示先输入文字或删除附件。
- 不创建只有附件、但在 `notes.json` 中为空白的隐藏便签。

## 4. S3 对象布局

默认布局：

```text
eggdone/
├─ todos.json
├─ notes.json
├─ note-attachments.json
└─ note-assets/
   └─ v1/
      └─ <attachment_uuid>/
         ├─ original
         └─ preview.jpg
```

`preview.jpg` 只对图片存在。文件名不进入对象路径，避免路径注入、特殊字符兼容和文件名泄漏。

### 4.1 Object Key 推导

两端必须共享以下规则：

| Todo Object Key | 附件元数据 | 二进制前缀 |
| --- | --- | --- |
| `todos.json` | `note-attachments.json` | `note-assets/v1/` |
| `eggdone/todos.json` | `eggdone/note-attachments.json` | `eggdone/note-assets/v1/` |
| `backup.json` | `backup.note-attachments.json` | `backup.note-assets/v1/` |
| `folder/backup` | `folder/backup.note-attachments.json` | `folder/backup.note-assets/v1/` |

最终二进制 Key 由前缀和附件 UUID 推导，不写入同步元数据：

```text
<asset_prefix>/<attachment_uuid>/original
<asset_prefix>/<attachment_uuid>/preview.jpg
```

设置页只读展示附件元数据 Key 和二进制前缀，不新增用户必填配置。

## 5. 跨端附件协议

### 5.1 文档结构

```json
{
  "format_version": 1,
  "device_id": "00000000-0000-4000-8000-000000000000",
  "generated_at": 0,
  "attachments": [
    {
      "uuid": "00000000-0000-4000-8000-000000000001",
      "note_uuid": "00000000-0000-4000-8000-000000000002",
      "kind": "image",
      "display_name": "会议白板.png",
      "mime_type": "image/png",
      "byte_size": 1024,
      "sha256": "64-char-lowercase-hex",
      "preview_mime_type": "image/jpeg",
      "preview_byte_size": 512,
      "preview_sha256": "64-char-lowercase-hex",
      "width": 1920,
      "height": 1080,
      "sort_order": 0,
      "created_at": 0,
      "updated_at": 0,
      "deleted_at": null,
      "updated_by": "00000000-0000-4000-8000-000000000000"
    }
  ]
}
```

普通文件的 `preview_mime_type`、`preview_byte_size`、`preview_sha256`、`width` 和 `height` 为 `null`。

### 5.2 校验规则

- `format_version` 必须为 `1`。
- `device_id`、`uuid`、`note_uuid` 和 `updated_by` 必须是 UUID。
- `note_uuid` 在解析时不要求父便签已经存在；元数据应保留并等待 Note 同步完成，UI 暂不展示孤立记录。
- `kind` 只能是 `image` 或 `file`。
- `display_name` 为去除路径后的文件名，最多 255 个 Unicode 字符，不得为空。
- `mime_type` 必须是合法的 `type/subtype` ASCII 字符串，最多 100 个字符。上传白名单属于客户端策略；远端出现合法但当前不支持的类型时仍应接受元数据并显示为不支持，不能拒绝整个文档。
- `byte_size` 必须大于 0，协议可读上限为 8 GiB；客户端第一阶段上传限制仍为图片 10 MiB、普通文件 20 MiB。
- SHA-256 必须是 64 位小写十六进制字符串。
- 图片必须有正数 `width`、`height` 和完整预览字段，`preview_byte_size` 不得超过 2 MiB。
- 普通文件的图片与预览字段必须为 `null`。
- `sort_order` 和时间戳不得为负数。
- 同一文档不得包含重复附件 UUID。

远端元数据无效时必须停止附件同步并显示错误，不能按空文档覆盖。

### 5.3 冲突决胜

附件按 UUID 独立执行最终一致性合并：

```text
updated_at
deleted_at 是否存在
updated_by
note_uuid
kind
display_name
mime_type
byte_size
sha256
preview_mime_type
preview_byte_size
preview_sha256
width
height
sort_order
created_at
```

时间相同的删除记录优先，避免附件复活。二进制对象不可变，不参与内容合并。

## 6. 本地数据模型

新增数据库迁移和 `note_attachments` 表：

```sql
CREATE TABLE note_attachments (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uuid TEXT NOT NULL UNIQUE,
  note_uuid TEXT NOT NULL,
  kind TEXT NOT NULL,
  display_name TEXT NOT NULL,
  mime_type TEXT NOT NULL,
  byte_size INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  preview_mime_type TEXT,
  preview_byte_size INTEGER,
  preview_sha256 TEXT,
  width INTEGER,
  height INTEGER,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER,
  updated_by TEXT NOT NULL,
  local_original_path TEXT,
  local_preview_path TEXT,
  transfer_state TEXT NOT NULL DEFAULT 'pending_upload',
  transfer_error TEXT,
  remote_uploaded INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_note_attachments_note_order
  ON note_attachments(note_uuid, deleted_at, sort_order, uuid);

CREATE INDEX idx_note_attachments_transfer
  ON note_attachments(transfer_state, updated_at);
```

`local_original_path`、`local_preview_path`、`transfer_state`、`transfer_error` 和 `remote_uploaded` 为本机字段，不写入 `note-attachments.json`。

本地路径保存为相对于应用文件根目录或缓存根目录的受控相对路径，不保存 Picker 源路径，也不写入同步协议。

文件保存在 Tauri `app_data_dir` 下，不写死用户目录：

```text
<app_data_dir>/note-assets/<attachment_uuid>/original
<app_data_dir>/note-assets/<attachment_uuid>/preview.jpg
```

## 7. 桌面端工程设计

### 7.1 Rust 层

建议新增：

```text
src-tauri/src/note_attachments.rs
src-tauri/src/note_attachment_sync.rs
src-tauri/src/note_asset_store.rs
```

职责：

- `note_attachments.rs`：查询、创建、排序、软删除、恢复和本地传输状态。
- `note_attachment_sync.rs`：构建、校验、合并和落库元数据文档。
- `note_asset_store.rs`：沙箱路径、原文件复制、SHA-256、预览生成、缓存和安全删除。
- `s3_sync.rs`：增加原始字节 GET/PUT/DELETE，不把二进制转换为字符串。
- `sync.rs`：在现有互斥同步任务中编排附件上传、元数据合并和状态汇总。
- `commands.rs`：暴露窄 Tauri command，不在 command 中堆放文件与 SQL 逻辑。

图片解码器第一阶段只启用 JPEG、PNG 和 WebP。依赖必须关闭不需要的默认特性，并记录包体变化。原文件永远按字节复制，不因生成预览而改写。

### 7.2 Svelte 层

建议新增：

```text
src/lib/api/noteAttachmentApi.ts
src/lib/stores/noteAttachmentStore.ts
src/lib/components/NoteAttachmentGrid.svelte
src/lib/components/NoteAttachmentItem.svelte
src/lib/components/NoteAttachmentViewer.svelte
```

`NoteEditor.svelte` 只组合附件区和触发用户动作，不直接读取文件、计算摘要或发起 S3 请求。

## 8. 同步状态机与顺序

### 8.1 本地状态

```text
pending_upload -> uploading -> uploaded -> synced
                      |             |
                      v             v
                    failed <------- failed
```

远端附件按需下载时使用：

```text
remote_only -> downloading -> cached
                    |
                    v
                  failed
```

### 8.2 新增附件同步顺序

1. 刷新便签文字到 SQLite。
2. 同步 `notes.json`，确保父便签已存在。
3. 使用 `If-None-Match: *` 上传不可变 `original`。
4. 图片继续上传 `preview.jpg`。
5. 两个对象都成功后设置 `remote_uploaded = 1`。
6. 构建并同步 `note-attachments.json`。
7. 元数据上传成功后把本地状态设为 `synced`。

二进制成功但元数据失败时允许产生暂时孤儿对象，下次同步继续上传元数据。元数据不得先于二进制发布。

若不可变对象上传返回 409/412，必须通过 HEAD 元数据或下载校验远端大小与 SHA-256；内容一致时按幂等成功处理，不一致时报告 UUID 对象冲突，不能覆盖。

### 8.3 远端同步与按需下载

- 前台恢复、60 秒检查、手动同步和自动同步只对附件元数据做 ETag 检查，不遍历全部二进制对象。
- 合并元数据后，当前可见卡片按需下载预览图。
- 用户打开图片或附件时再下载原文件，并校验 SHA-256。
- 下载失败保留元数据和重试入口，不删除便签。
- 同一个附件的上传或下载任务按 UUID 去重。

### 8.4 整体同步状态

只有以下条件同时满足才显示 `已同步`：

- Todo 文档同步成功。
- Note 文档同步成功。
- 附件元数据同步成功。
- 不存在 `pending_upload`、`uploading` 或 `failed` 的本地附件。

状态详情显示 `2 个附件等待上传`、`1 个附件上传失败` 等可执行信息。

## 9. 删除、恢复与清理

- 删除附件写入 `deleted_at` 墓碑并立即从 UI 隐藏。
- 从未上传成功的附件可直接删除本地文件和记录，不产生远端墓碑。
- 已上传附件必须先同步墓碑；二进制文件在删除 30 天后才允许清理。
- 删除便签时，在同一数据库事务中软删除其活动附件。
- 恢复便签时恢复仍在 30 天保留期内的附件；远端文件缺失时显示明确错误。
- 元数据墓碑长期保留，避免长期离线设备重新发布旧附件。
- 设置页后续增加 `附件占用空间`、`清理本地缓存` 和 `清理远端无引用附件`。

物理清理需要 S3 `DeleteObject` 权限，但该权限不是图片与附件同步的前置条件。缺少删除权限时保留远端孤儿文件并提示无法释放空间，不能让正常同步失败。

远端清理必须根据已同步墓碑执行，不能通过列举前缀后删除未知对象。

## 10. 桌面 UI 方案

### 10.1 编辑器入口

- 在便签编辑器底部操作区增加图片和回形针图标。
- 图片入口优先放在颜色选择之前，保持完成、置顶和删除原行为不变。
- 第一阶段支持文件选择；拖放和剪贴板粘贴作为同一 Roadmap 的增强任务。
- 操作区在窄托盘面板下允许分组或横向滚动，不得与颜色圆点重叠。

### 10.2 附件区域

- 图片使用两列网格；单张图片使用较宽预览但不超过编辑器宽度。
- 缩略图显示上传进度、失败和重试状态。
- 普通附件使用紧凑行，显示类型图标、文件名、大小和状态。
- 点击图片打开当前窗口内查看器；点击文件在确认已下载后调用系统默认程序。
- 删除操作需要二次确认或提供短时撤销。

### 10.3 便签列表

- 卡片最多显示第一张图片的裁切预览。
- 其余附件使用 `3 张图片`、`2 个附件` 等紧凑提示。
- 图片加载失败不得改变卡片高度或阻止打开便签。
- 没有附件的卡片保持当前布局和高度。

## 11. 安全、隐私与性能

- 只信任实际解码结果和文件头，不只信任扩展名。
- `display_name` 只用于显示，必须移除目录、控制字符和路径分隔符。
- 禁止自动执行或预览可执行文件、脚本和未知二进制文件。
- 预览图不保留 EXIF；原图保持原始字节，设置页说明原图可能包含位置等元数据。
- S3 凭据继续保存在操作系统凭据库，不写入附件元数据或日志。
- SHA-256 用于下载完整性校验；S3 ETag 只用于元数据并发保护，不当作文件摘要。
- 列表只加载预览图，原图按需下载；本地预览缓存采用 LRU 或空间上限清理。
- 第一阶段使用单次 PUT。大文件和分片上传必须在单独阶段实现和验证。

## 12. 导入、导出与备份

- 现有 JSON 导入导出继续只包含便签和附件元数据，不嵌入二进制。
- 当 JSON 中包含附件元数据但没有二进制包时，导入预览必须提示 `附件文件不包含在 JSON 中`。
- 后续新增 `.eggdone-backup` 压缩备份，包含 JSON 清单和 `note-assets/` 文件。
- 压缩包导入必须校验路径、大小和 SHA-256，禁止目录穿越。
- 第一阶段不能在未实现完整备份前宣称 JSON 已备份附件文件。

## 13. 测试要求

### 13.1 自动测试

- 新库和旧库迁移，原 Todo、Note 数据不变。
- 附件 CRUD、排序、软删除、恢复和父便签级联软删除。
- Object Key 和资产前缀推导 fixture 与鸿蒙端一致。
- 元数据校验、合并、删除优先和稳定决胜。
- 原文件复制、摘要、预览生成及损坏文件处理。
- 二进制 404、401/403、409/412、超时和摘要不匹配。
- 元数据先后顺序：二进制失败时不得发布附件记录。
- Todo 或 Note 成功而附件失败时整体状态不得显示 `已同步`。

### 13.2 跨端与手动回归

- 桌面离线添加图片，重启后仍可查看，恢复网络后鸿蒙端可见。
- 鸿蒙添加 HEIC/HEIF 图片，桌面可以显示 JPEG 预览并下载原图。
- 同一附件重复触发同步不会重复上传或生成第二条记录。
- 两端同时删除和排序时合并结果一致。
- 上传中退出应用、网络中断、凭据错误后可以继续重试。
- 亮色、暗色和窄面板下按钮、进度和错误文字满足对比度要求。
- 删除便签后附件隐藏，墓碑同步后另一端也删除。

## 14. 完成标准

- `todos.json` 和 `notes.json` v1 完全不变。
- 两端 `note-attachments.json` fixture、校验和冲突结果完全一致。
- 附件本地保存和云同步状态分离，失败不会丢失本地文件。
- 图片列表只加载预览，原图按需下载且校验摘要。
- 旧客户端继续同步文字便签，不会覆盖附件元数据。
- `pnpm check`、`pnpm test`、`pnpm build`、`cargo fmt -- --check`、`cargo check` 和相关 Rust 测试通过。
- Windows 完成真实 S3 / MinIO 双端回归；macOS、Linux 不引入硬编码路径。
