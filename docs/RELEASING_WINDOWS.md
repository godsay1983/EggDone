# Windows 发布流程

## 发布前

1. 同步以下三个版本号：
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
2. 更新 README、优化计划和发布说明。
3. 执行：

```powershell
pnpm release:check
```

4. 按 [MANUAL_REGRESSION.md](MANUAL_REGRESSION.md) 完成手动回归。

## 构建 NSIS 安装包

```powershell
pnpm build:windows
```

安装包输出到：

```text
src-tauri/target/release/bundle/nsis/
```

当前安装器使用当前用户安装模式，不要求管理员权限；WebView2 缺失时会静默下载安装引导程序。安装器禁止用低版本覆盖高版本。

## 自动验证安装器

准备两个相邻版本的 NSIS 安装包后运行：

```powershell
.\scripts\verify-windows-installer.ps1 `
  -BaseInstaller .\src-tauri\target\release\bundle\nsis\EggDone_0.1.0_x64-setup.exe `
  -UpgradeInstaller .\src-tauri\target\release\bundle\nsis\EggDone_0.1.1_x64-setup.exe
```

脚本会验证当前用户模式的全新安装、覆盖升级、降级拦截、版本元数据和静默卸载。为避免破坏开发机上的真实安装，只要检测到 EggDone 已安装或正在运行，脚本就会停止。

Tauri 2.11.2 的 NSIS 静默安装路径可能跳过内置版本比较。`src-tauri/windows/installer-hooks.nsh` 会在复制文件前再次检查版本，确保低版本安装器无法覆盖高版本。

## 代码签名

正式公开发布前应配置可信 Windows 代码签名证书。证书指纹、私钥和时间戳服务属于发布环境机密，不提交到仓库。

未签名安装包只适合内部测试。Windows SmartScreen 可能显示未知发布者警告。

## 验证

在干净的 Windows 10/11 用户环境中验证：

1. 全新安装并首次启动。
2. 覆盖安装升级，确认 SQLite、设置和系统凭据仍可用。
3. 卸载后确认程序文件、开机启动项和运行进程已移除。
4. 重新安装，确认保留的用户数据符合发布策略。

自动脚本不启动应用，也不修改应用数据目录。SQLite、设置和系统凭据的升级保留仍需按手动回归清单验证。

## 回滚

安装器禁止降级，因此回滚应发布一个版本号更高、代码内容回退的修复版本。例如从 `0.5.0` 回滚功能时发布 `0.5.1`，不要重新分发 `0.4.x`。

数据库 migration 只能向前执行。回滚版本必须能够读取当前 schema，不得删除或降级用户数据库。
