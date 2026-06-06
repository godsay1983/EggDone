# EggDone（蛋定 Todo）

EggDone 是一个轻量级、跨平台、托盘常驻的 Todo 桌面应用。应用启动后不显示普通主窗口；点击系统托盘或菜单栏图标，会在图标附近打开 Todo 面板。面板失去焦点后自动隐藏。

项目中的「拖拖蛋」是原创吉祥物：慵懒蛋黄角色配合任务勾选牌，不使用 Gudetama、蛋黄哥、Sanrio 等现有商业 IP 素材。

## MVP 功能

- 托盘常驻，启动时隐藏面板
- 左键点击托盘图标打开或隐藏面板
- 托盘右键菜单：打开/隐藏、新增任务、关于、退出
- 快速新增、完成、取消完成和删除 Todo
- 显示未完成任务数量和空状态
- 亮色和暗色主题切换，首次启动跟随系统并记住选择
- 面板无边框、置顶、跳过任务栏，失焦自动隐藏
- SQLite 本地持久化
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
cd src-tauri
cargo check
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
| `title` | 任务内容 |
| `completed` | 完成状态 |
| `created_at` | 创建时间 |
| `updated_at` | 更新时间 |

开发时可以删除该数据库以重置数据。具体根目录由 Tauri 的 `app_data_dir` 按平台决定。

## 目录结构

```text
EggDone/
├─ src/
│  ├─ lib/
│  │  ├─ api/todoApi.ts          # Tauri command 调用
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
│  │  ├─ tray.rs                 # 托盘菜单、事件和窗口定位
│  │  ├─ lib.rs                  # Tauri 应用装配
│  │  └─ main.rs
│  └─ tauri.conf.json
└─ AGENTS.md
```

## 当前限制

- 托盘附近定位使用平台提供的图标坐标；不可用时回退到主屏幕右下角。
- MVP 暂不包含任务编辑、分组、提醒、同步、搜索和开机自启动。
