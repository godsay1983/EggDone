# EggDone 开发约定

本文件记录后续 Codex 和开发者在本仓库中的协作规则。

## 项目目标

EggDone 是轻量级托盘 Todo，不是完整任务管理平台。修改应优先保持启动快、界面小、依赖少、离线可用。

MVP 核心链路必须保持可用：

1. 启动后托盘常驻且面板隐藏。
2. 左键托盘图标打开或隐藏面板。
3. 面板失焦或关闭时隐藏，只有托盘“退出”结束进程。
4. Todo 可以新增、完成、取消完成、删除并持久化。

## 架构边界

- `src/lib/api/` 只负责 Tauri command 调用和数据传输。
- `src/lib/stores/` 管理前端状态和业务操作。
- `src/lib/components/` 保存可复用 UI，不直接操作 SQLite。
- `src-tauri/src/commands.rs` 定义前端可调用命令。
- `src-tauri/src/db.rs` 负责数据库连接、建表和迁移入口。
- `src-tauri/src/tray.rs` 负责托盘、菜单、面板显隐和定位。
- Linux 托盘由 `src-tauri/src/tray_ksni.rs` 经 StatusNotifierItem(ksni)实现,因为 tray-icon 的 GTK 后端不转发左键 Activate;ksni 不可用时回退 Tauri 托盘。
- 同步域：`src-tauri/src/s3_sync.rs` 负责 S3/MinIO 配置、系统凭据和连接测试；`sync.rs` 负责 Todo 合并与冲突决胜；`note_sync.rs`、`note_attachment_sync.rs`、`note_asset_store.rs` 负责便签和附件的同步；前端 `src/lib/sync/autoSync.ts` 编排自动同步（本地修改 4 秒防抖、前台 60 秒 ETag 轮询、网络类错误有限退避）。
- `notes.rs` 负责便签 CRUD，`note_attachments.rs` 负责附件存储；`reminders.rs` 负责系统提醒投递和去重；`schedule.rs` 只做日期与到期时间的本地日历换算，不包含业务逻辑。
- `data_exchange.rs` 负责 JSON 导入导出、合并预览和 SQLite 备份快照。
- 多语言：前端字典在 `src/lib/i18n/`（zh-CN/en-US），Rust 侧文案在 `src-tauri/src/i18n.rs`；`scripts/check-i18n-catalogs.mjs` 和 `scripts/check-i18n-hardcodes.mjs` 校验字典键、占位符和模板中的中文硬编码。
- `error_codes.rs` 定义 `EGGDONE_ERROR::CODE::detail` 错误字符串协议，前端按前缀解析分类展示。
- 不要把托盘、数据库和 command 逻辑重新堆入 `lib.rs`。

## 编码规范

- TypeScript 开启严格类型检查，不使用无说明的 `any`。
- Rust 错误应返回可理解的信息，不在正常业务路径使用 `unwrap()`。
- SQL 必须使用参数绑定，禁止拼接用户输入。
- 新增数据库字段时提供向前兼容的迁移。
- 保持组件小而明确；只有消除实际复杂度时才增加抽象。
- 注释说明原因和平台差异，不复述代码。
- UI 文案以简体中文为主，标识符和代码注释使用英文。

## 视觉与 IP

- 使用蛋黄、checkbox、圆角卡片等原创通用元素。
- 不得加入 Gudetama、蛋黄哥、Sanrio 或其他现有 IP 的名称、图像、轮廓复刻和素材。
- `src-tauri/icons/app-icon.png` 是透明背景的应用图标源文件，更新后使用 `pnpm tauri icon <source>` 重新生成各平台图标。
- 不引入大型 UI 框架；优先使用局部 Svelte 组件和原生 CSS。

## 跨平台要求

- Windows 是第一验证平台。
- 修改窗口或托盘逻辑时，检查 macOS 菜单栏与 Linux 托盘 API 的兼容性。
- 平台特定实现必须有通用回退，必要时使用条件编译隔离。
- 文件路径必须通过 Tauri path API 获取，不写死用户目录或路径分隔符。

## 验证要求

提交前至少运行：

```bash
pnpm check
pnpm i18n:check
pnpm test
pnpm build
cd src-tauri
cargo fmt -- --check
cargo check
cargo test
```

完整发布门禁为 `pnpm release:check`（依次执行 i18n:check、check、test、build 和全部 cargo 检查）。

前端单测使用 Vitest，可运行单个文件或按模式筛选：

```bash
pnpm test -- src/lib/utils/todoDates.test.ts
```

涉及托盘或窗口行为时，还需运行 `pnpm tauri dev` 手动验证：

- 启动时不显示面板。
- 托盘左键显隐正常。
- 托盘右键菜单四项可用。
- 失焦和关闭按钮只隐藏面板。
- 退出菜单结束进程。

涉及数据库时，使用全新数据库和已有数据库各验证一次。

## 变更原则

- 保持改动聚焦，不顺带重构无关模块。
- 不提交 `node_modules/`、`target/`、本地数据库或编辑器临时文件。
- 更新用户可见行为时同步更新 README。
- 云同步（S3/MinIO）已实现；第一阶段仍不加入账户系统、遥测或自动更新，除非需求明确提出。
