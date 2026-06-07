# EggDone（蛋定 Todo）

EggDone 是一个轻量级、跨平台、托盘常驻的 Todo 桌面应用。应用启动后不显示普通主窗口；点击系统托盘或菜单栏图标，会在图标附近打开 Todo 面板。面板失去焦点后自动隐藏。

项目中的「拖拖蛋」是原创吉祥物：慵懒蛋黄角色配合任务勾选牌，不使用 Gudetama、蛋黄哥、Sanrio 等现有商业 IP 素材。

## MVP 功能

- 托盘常驻，启动时隐藏面板
- 左键点击托盘图标打开或隐藏面板
- 托盘右键菜单：打开/隐藏、新增任务、关于、退出
- 快速新增、行内编辑、完成和取消完成 Todo
- 拖动排序、清除已完成、软删除及 5 秒撤销
- JSON 导入导出、UUID 合并和 SQLite 手动备份
- 可配置 AWS S3、MinIO 和其他 S3 兼容存储
- Access Key 和 Secret Key 保存到系统凭据库
- 可配置全局快捷键，默认 `Ctrl + Shift + Space`
- 可选开机自动运行，并静默进入托盘
- 显示未完成任务数量和空状态
- 亮色和暗色主题切换，首次启动跟随系统并记住选择
- 面板无边框、置顶、跳过任务栏，失焦自动隐藏
- 面板按托盘所在显示器定位，并限制在显示器工作区内
- 区分托盘点击与普通失焦，避免弹层和原生下拉操作误隐藏
- SQLite 本地持久化
- 数据库顺序迁移，旧版数据可自动升级
- 单实例运行，重复启动时唤醒已有面板
- Windows 优先，同时保留 macOS 和 Linux 结构

## 技术栈

- Tauri 2
- Svelte 5 + SvelteKit + TypeScript
- Rust + rusqlite（bundled SQLite）
- pnpm

## 开发环境

请先安装：

- Node.js 20 或更高版本
- pnpm 10 或更高版本
- Rust stable 工具链
- 对应平台的 Tauri 系统依赖

Windows 需要 WebView2。Windows 10/11 通常已安装。

## 开发命令

```bash
pnpm install
pnpm tauri dev
```

应用启动后默认隐藏，请在系统托盘中找到 EggDone 图标并左键点击。

常用检查命令：

```bash
pnpm check
pnpm build
pnpm test
cd src-tauri
cargo check
cargo test
cargo fmt -- --check
```

## 构建

```bash
pnpm tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`。不同平台会生成对应的安装包格式。

## 数据存储

应用首次启动时会在平台应用数据目录创建 `eggdone.sqlite3`。数据库包含 `todos` 表：

| 字段 | 说明 |
| --- | --- |
| `id` | 自增主键 |
| `uuid` | 跨设备唯一标识 |
| `title` | 任务内容 |
| `completed` | 完成状态 |
| `sort_order` | 任务排序值 |
| `created_at` | UTC 创建时间（毫秒时间戳） |
| `updated_at` | UTC 更新时间（毫秒时间戳） |
| `updated_by` | 最后修改该任务的设备 UUID |
| `completed_at` | UTC 完成时间（毫秒时间戳，可空） |
| `deleted_at` | UTC 软删除时间（毫秒时间戳，可空） |

`schema_migrations` 表记录已执行的数据库版本，`app_metadata` 保存本机 `device_id`，`sync_settings` 只保存 Endpoint、Region、Bucket、Object Key 等非敏感配置。Access Key 和 Secret Key 保存到操作系统凭据库，不写入 SQLite。开发时可以删除数据库以重置数据，具体根目录由 Tauri 的 `app_data_dir` 按平台决定。

项目已包含版本化同步文档和本地合并核心：按 Todo UUID 合并，优先采用较新的 `updated_at`；时间相同时优先保留删除记录，再通过 `updated_by` 稳定决胜。设置页可配置 AWS S3 或自定义 S3 Endpoint，支持 MinIO 常用的 Path Style 和 HTTP。HTTP 必须显式确认明文传输风险。

当前“测试连接”会向配置的 Bucket 和 Object Key 发起签名请求，验证 Endpoint、凭据和访问权限；返回 404 时会提示同步文件尚未创建，此时仍需确认 Bucket 已提前创建。实际上传、下载、ETag 冲突保护和“立即同步”仍在后续阶段。

面板右上角的“数据管理”可导出版本化 JSON、预览并合并导入文件，或创建一致的 SQLite 快照。导入只更新 `updated_at` 更新的同 UUID 任务，不会直接覆盖整个本地数据库。

面板右上角的“设置”可管理全局快捷键、系统开机启动和 S3 / MinIO 同步连接。删除系统凭据时会同时禁用同步。快捷键冲突时会保留之前的有效配置并显示错误。

## 目录结构

```text
EggDone/
├─ src/
│  ├─ lib/
│  │  ├─ api/todoApi.ts          # Tauri command 调用
│  │  ├─ api/syncApi.ts          # 同步配置和连接测试调用
│  │  ├─ components/             # Todo 面板和列表项
│  │  ├─ stores/todoStore.ts     # Todo 状态与操作
│  │  └─ types.ts
│  ├─ routes/+page.svelte        # SvelteKit 页面入口
│  └─ app.css
├─ src-tauri/
│  ├─ icons/                     # 图标源文件及各平台图标
│  ├─ src/
│  │  ├─ commands.rs             # 前后端命令
│  │  ├─ db.rs                   # SQLite 初始化
│  │  ├─ panel_position.rs       # 多显示器面板定位计算
│  │  ├─ s3_sync.rs              # S3 配置、系统凭据和连接测试
│  │  ├─ sync.rs                 # 同步文档、冲突决胜和 UUID 合并
│  │  ├─ tray.rs                 # 托盘菜单、事件和窗口定位
│  │  ├─ lib.rs                  # Tauri 应用装配
│  │  └─ main.rs
│  └─ tauri.conf.json
└─ AGENTS.md
```

## 当前限制

- 托盘附近定位使用平台提供的图标坐标；不可用时回退到主屏幕右下角。
- Windows 混合 DPI 多显示器仍需在 125%、150% 和 200% 缩放下进行实机验收。
- 当前暂不包含分组、提醒、远端读写同步和搜索。
- S3 / MinIO 尚未实现对象上传下载、ETag 冲突保护和自动同步。

后续优化和版本规划见 [OPTIMIZATION_TODO.md](OPTIMIZATION_TODO.md)。
