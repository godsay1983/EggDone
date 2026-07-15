# EggDone 便签附件备份格式

本文件是桌面端与 HarmonyOS 端共享的数据交换约定。两端修改字段、默认值或目录结构时必须同步更新。

## 1. 普通 JSON

现有 `eggdone-data.json` 继续使用 `format_version: 1`，并增加两个向后兼容的可选字段：

```json
{
  "format_version": 1,
  "exported_at": 0,
  "groups": [],
  "todos": [],
  "notes": [],
  "note_attachments": [],
  "attachment_files_included": false
}
```

- `note_attachments` 使用 `note-attachments.json` v1 的 `SyncNoteAttachment` 数组结构。
- 普通 JSON 只导出已经上传到对象存储或已经删除的附件元数据，不导出尚未上传且无法远端恢复的本地附件。
- `attachment_files_included` 在普通 JSON 中必须为 `false`，附件原文件和预览图不允许 Base64 嵌入 JSON。
- 缺少 `note_attachments` 的旧文件按空数组处理。
- 缺少 `attachment_files_included` 的旧文件按 `false` 处理。
- 导入附件元数据后，本地无缓存的活动附件进入 `remote_only`，由现有按需下载流程从 S3 或 MinIO 恢复。
- 导入预览必须明确显示附件数量，并提示“附件文件不包含在 JSON 中”。
- 普通 JSON 声明 `attachment_files_included: true` 时必须拒绝导入，避免把不完整文件误认为完整备份。

## 2. 完整备份容器

完整备份扩展名为 `.eggdone-backup`，底层为标准 ZIP。第一版目录结构固定为：

```text
manifest.json
data.json
note-assets/
  <attachment-uuid>/
    original
    preview.jpg
```

- `data.json` 使用第 1 节结构，并将 `attachment_files_included` 设为 `true`。
- `manifest.json` 记录容器版本、创建时间、`data.json` 的 SHA-256，以及每个二进制条目的相对路径、字节数和 SHA-256。
- 普通文件只有 `original`；图片必须包含 `original` 和 `preview.jpg`。
- ZIP 条目路径只允许上述固定相对路径，不允许绝对路径、反斜杠、空段、`.`、`..`、符号链接或重复条目。
- 导入前必须校验单文件大小、总解压大小、条目数量和 SHA-256，全部通过后才能写入数据库。
- 导入使用临时目录和原子移动；失败时不得留下部分元数据或覆盖已有有效缓存。

## 3. 第一阶段实现边界

当前 DA8/HA8 第一提交边界只实现普通 JSON 的附件元数据导入导出和明确提示。`.eggdone-backup` 的 ZIP 创建、校验和二进制恢复在下一提交边界实现。
