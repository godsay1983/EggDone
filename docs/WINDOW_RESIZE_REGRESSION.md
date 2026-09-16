# Windows 主窗口边缘缩放修复

## 原因与范围

主窗口为无标题栏、可调整大小的托盘面板。Windows 上的 Tauri 2.11.2 将 WebView2 LostFocus 转成 WindowEvent::Focused(false)，原实现直接进入自动隐藏。原生边框不保证发送 DOM pointerdown，因此 300ms 的内部交互宽限不能覆盖边框点击或长时间拖拽。

现在隐藏前使用 GetForegroundWindow 核对主窗口 HWND：主窗口仍在前台时忽略此内部失焦，不登记托盘隐藏记录；切换到其他窗口时保留原有对话框保护、托盘切换和失焦隐藏语义。不改窗口尺寸、缩放偏好、版本号或数据库。平台判断仅 Windows 编译，macOS/Linux 保持原路径。

参考：[Tauri 同类问题](https://github.com/tauri-apps/tauri/issues/10767)、[GetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getforegroundwindow)。本地依赖源码核对了 WebView2 LostFocus 的转发路径，不单凭上游问题认定本机现象。

## 验证

右下角不再绘制额外的 L 形缩放提示线；保留原 10px 角部热区、双向缩放光标与拖拽事件。浏览器回归在各主题和窗口宽度下检查没有伪元素装饰，同时继续验证八方向命中。

- Rust 回归：内部原生焦点转移无需 DOM 宽限，随后真实失焦仍可隐藏；内部失焦不吞掉下次托盘切换；保留已有对话框和托盘时序测试。
- 浏览器窗口回归：八个方向实际命中测试及 resize IPC 顺序，右键不触发缩放。原生 API 为测试替身，不能代替 Windows 拖拽验收。
- 2026-09-16 自动化结果：11 项 Rust 托盘/焦点测试、8 项窗口偏好测试、16 组中英文/深浅色/窗口宽度组合及八方向命中/IPC 全部通过。Svelte 检查 0 errors / 0 warnings，前端构建、Rust check 和 fmt 通过；保留既有 TraySnapshot.locale 未使用警告。
- Windows 隔离原生实例 `com.eggdone.resizetest` 已成功构建、启动，核对实际 Tauri IPC 标识后操作。可打开设置并切换大窗口/150% 预设。自动拖拽工具尝试上边和右边时窗口保持可见，但尺寸未稳定变化，因此不计作八方向原生拖拽通过；其他平台未运行。测试进程已结束。
- 人工验收仍需在修复版中拖动上边、右上角、右边，并回归其他边角；分别验证放大、缩小、按住超过一秒、松手后再拖、点击窗口外隐藏和托盘重新显示。另检查尺寸保存及 100%/150% 内容缩放。不要使用旧安装版代替修复版。
- 普通应用标识的修复版 Debug 可执行文件已构建：`src-tauri/target/debug/eggdone.exe`，命令为 `node node_modules/@tauri-apps/cli/tauri.js build --debug --no-bundle -- --locked`。无需前端开发服务器；先退出旧客户端再运行，避免单实例转交到旧进程。不是安装器，未更新已安装版本或发布。

复跑：`cargo test --locked --lib tray::tests`（src-tauri 目录），`node scripts/test-window-ui.mjs`（仓库根，需 PLAYWRIGHT_PATH），以及 `pnpm check`、`pnpm build`、`cargo fmt -- --check`、`cargo check --locked`。
