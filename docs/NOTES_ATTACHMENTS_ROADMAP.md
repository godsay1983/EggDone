# EggDone 桌面端便签图片与附件 Roadmap

配套设计见 [NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md](NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md)。跨端协议阶段必须与鸿蒙端 [HARMONY_NOTES_ATTACHMENTS_ROADMAP.md](../../EggDoneHarmony/docs/HARMONY_NOTES_ATTACHMENTS_ROADMAP.md) 的 `HA0` 同步完成。

本 Roadmap 是已完成便签功能的扩展，不重新打开 `NOTES_ROADMAP.md` 中已经完成的基础工作。

## DA0：跨端协议与风险冻结

- [x] 固定 `note-attachments.json` v1 字段、空值和大小单位。
- [x] 固定附件元数据 Key、资产前缀和 UUID 对象路径推导规则。
- [x] 固定 MIME 白名单、图片上限、文件上限和每条便签数量限制。
- [x] 固定附件冲突决胜、删除墓碑和 30 天二进制保留规则。
- [x] 固定空白草稿添加附件时的自动标题和非空校验规则。
- [x] 建立桌面 Rust 与鸿蒙 ArkTS 共用的协议 fixture。
- [x] 验证旧客户端继续同步 `notes.json` 时不会触碰附件对象。
- [x] 明确 JSON 导出不包含二进制，并定义后续完整备份格式。

交付物：双端协议 fixture、对象 Key fixture、错误码和状态表。

## DA1：数据库迁移与领域模型

- [x] 新增 `note_attachments` 表、索引和本机传输字段。
- [x] 验证全新数据库建库。
- [x] 验证现有数据库升级且 Todo、Note、同步设置不变。
- [x] 新增 Attachment Rust 类型和严格参数校验。
- [x] 实现按便签查询、创建、排序、软删除、恢复和级联软删除。
- [x] 实现待上传、上传中、失败、已上传和仅远端状态迁移。
- [x] 增加 Repository 和 migration 测试。

完成条件：不依赖 UI 即可稳定保存附件元数据和恢复传输状态。

## DA2：本地资产存储与图片处理

- [x] 使用 `app_data_dir` 建立附件目录，不保存外部绝对路径。
- [x] 安全复制原文件并以临时文件加原子重命名落盘。
- [x] 计算 SHA-256、MIME、文件大小和图片尺寸。
- [x] 为 JPEG、PNG、WebP 生成最大 512px JPEG 预览。
- [x] 处理透明图片、超大尺寸、损坏文件和不支持格式。
- [x] 实现本地文件校验、缓存命中和安全删除。
- [ ] 增加存储层单元测试和样例图片 fixture。

完成条件：离线添加图片后重启应用仍可显示原图和预览。

## DA3：二进制 S3 / MinIO 客户端

- [x] 在 `s3_sync.rs` 增加字节 GET、PUT、HEAD 和受控 DELETE。
- [x] 上传不可变对象时使用 `If-None-Match: *`。
- [x] 保留 Content-Type、Content-Length 和 SHA-256 元数据。
- [x] 409/412 后校验远端大小和 SHA-256，一致时按幂等成功处理。
- [x] 下载完成后校验大小与 SHA-256。
- [x] 对 401/403、404、409/412、超时和网络中断返回可执行错误。
- [x] 同一附件 UUID 的传输任务去重。
- [ ] 使用 AWS S3 和至少一种 MinIO / 兼容存储验证。

完成条件：命令层可以上传、下载并校验原文件和预览图。

## DA4：附件元数据同步

- [x] 新增 AttachmentSyncDocument 构建、校验、稳定排序和合并。
- [x] 实现附件元数据 ETag、`If-Match` 和一次冲突重试。
- [x] 远端 404 时按空文档处理并使用 `If-None-Match: *` 创建。
- [x] 二进制全部成功后才把附件加入可上传元数据文档。
- [x] 将 Todo、Note、附件放入同一同步互斥任务。
- [x] 整体同步状态纳入元数据错误和待上传附件数量。
- [x] 前台恢复、60 秒检查和手动同步覆盖附件元数据。
- [x] 运行跨端 fixture，确认 Rust 与 ArkTS 合并结果一致。

完成条件：桌面新增图片后鸿蒙端可获取元数据和预览，失败时不误报已同步。

## DA5：图片 UI MVP

- [x] 便签编辑器增加图片入口，不挤压颜色和现有操作。
- [ ] 实现两列图片网格、空状态、上传进度、失败和重试。
- [x] 实现当前窗口内图片查看器和原图下载。
- [x] 实现附件删除和 6 秒短时撤销。
- [ ] 实现附件手动排序。
- [x] 便签卡片最多显示第一张预览和附件数量提示。
- [x] 没有附件时保持现有卡片和编辑器布局。
- [x] 增加拖放图片和剪贴板图片粘贴。
- [ ] 完成亮色、暗色、窄面板和键盘操作适配。

完成条件：Windows 托盘面板可完成图片添加、查看、同步、删除和重试。

## DA6：按需下载、缓存与清理

- [ ] 可见卡片只下载预览，打开查看器时再下载原图。
- [ ] 实现 `remote_only`、`downloading`、`cached` 和 `failed` 状态。
- [ ] 实现本地缓存空间统计和清理，不删除待上传原文件。
- [ ] 删除 30 天后按墓碑清理远端二进制。
- [ ] 缺少 `DeleteObject` 权限时只跳过物理清理，不影响正常同步。
- [ ] 删除便签时同步软删除全部附件。
- [ ] 设置页只读展示附件元数据 Key、资产前缀和待同步数量。
- [ ] 设置页增加清理本地附件缓存入口。

完成条件：大量图片不会在启动时全部下载，缓存清理后可以重新获取。

## DA7：普通附件

- [ ] 增加安全 MIME 白名单和 20 MiB 单文件限制。
- [ ] 编辑器增加回形针入口和紧凑附件行。
- [ ] 支持下载、重试、另存为和系统默认程序打开。
- [ ] 禁止自动打开脚本、可执行文件和未知二进制。
- [ ] 文件名清洗并处理重名、长名和非法字符。
- [ ] 验证 PDF、文本、Office 文档和 ZIP 双端同步。

完成条件：支持的普通附件可以跨端同步并安全打开，不影响图片流程。

## DA8：备份、回归与发布

- [ ] JSON 导出和导入明确区分附件元数据与二进制。
- [ ] 设计并实现包含二进制的 `.eggdone-backup` 完整备份。
- [ ] 验证离线添加、上传中退出、网络中断、凭据错误和摘要不匹配。
- [ ] 验证桌面添加、鸿蒙删除、桌面恢复的完整链路。
- [ ] 验证 HEIC/HEIF 远端图片在桌面显示 JPEG 预览。
- [ ] 运行 `pnpm check`、`pnpm test` 和 `pnpm build`。
- [ ] 运行 `cargo fmt -- --check`、`cargo check` 和相关 Rust 测试。
- [ ] 更新 README、手动回归文档、版本号和发布说明。

完成条件：真实 S3 / MinIO 双端测试通过，升级不破坏现有 Todo 和文字便签同步。

## 推荐提交边界

1. `docs(attachments): freeze cross-device attachment protocol`
2. `feat(attachments): add desktop attachment persistence`
3. `feat(attachments): add desktop local image assets`
4. `feat(sync): transfer note assets through s3`
5. `feat(attachments): add desktop image interface`
6. `feat(attachments): add generic file support`
7. `test(attachments): cover cross-device sync and recovery`
