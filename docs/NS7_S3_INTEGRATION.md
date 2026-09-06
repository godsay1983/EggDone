# NS7 隔离 S3 原生传输集成

更新：2026-09-06。适用候选版本：桌面 1.0.8、鸿蒙 1.1.17 / 1000024。

## 范围

本测试直接执行桌面生产 `RecurrenceTransport`（Rust/rust-s3）和鸿蒙生产 `S3SyncClient`（ArkTS/原生 NetworkKit/签名），通过独立 SeaweedFS 4.34 S3 服务交换规则对象，不使用 HTTP 响应替身。

这属于**真实传输层集成**，不是整应用双端同步验收。它不覆盖本地数据库合并、离线同时完成、配置切换迟到回包、旧客户端或附件完整备份；NS7 的 R4/R5 不能仅据此全部勾选。

## 验证链路

1. 桌面创建随机临时桶，对中文和字面百分号路径执行 HEAD/GET，确认规则对象初次 404。
2. 桌面使用 If-None-Match 创建规则；再次使用旧的不存在快照写入必须冲突。错误签名的 HEAD/GET 必须返回拒绝，而不是当作对象不存在。
3. 鸿蒙从同一对象读取桌面规则，核对来源和时间戳；使用最新 ETag 更新为鸿蒙版本。
4. 鸿蒙再次用旧 ETag 写入必须冲突，HEAD 标记必须变化，重新下载必须得到刚写入的数据；错误凭据同样被拒绝。
5. 鸿蒙在独立对象路径验证 404 创建和旧不存在快照冲突。
6. 桌面重新读取鸿蒙数据并进行条件更新，再次验证旧 ETag 不会覆盖新内容。
7. 停止并删除本次创建的容器，移除本次反向端口映射，恢复原来的脚本环境变量。

三段使用唯一运行 ID 和随机桶名，完整报告必须分别为 Rust 1/1、Hypium 1/1、Rust 1/1。鸿蒙默认 17 项测试另外运行、另外校验，不把普通套件的成功冒充网络套件成功。

## 安全复跑

前提：

- Windows PowerShell 7、Docker Linux daemon、已安装的 `chrislusf/seaweedfs:4.34` 镜像。
- Node、Rust/Cargo、devecocli，以及可覆盖安装当前调试包的目标设备。
- 两个 checkout 都包含本次集成测试。先备份重要数据；脚本只覆盖安装，绝不卸载应用。
- 默认本机端口 18473 空闲，目标设备没有使用相同端口的 hdc 转发规则。

从鸿蒙仓库根目录执行：

```powershell
.\scripts\run-s3-integration.ps1 -Device '<设备ID>' -DesktopRoot 'D:\Develop\EggDone'
```

端口被占用时显式增加 `-Port <空闲端口>`。脚本只支持回环地址，不接受外部 endpoint，不读取用户应用的凭据文件；不使用已运行的其他对象存储容器或数据卷，不自动拉取/升级镜像。脚本中的公开测试凭据仅用于本次临时服务，**不得用于任何持久化或对外服务**。

临时 S3 数据存于该容器的 tmpfs，只向主机 127.0.0.1 发布 S3 端口。容器创建时带唯一所有权标签，清理前核对标签；设备只增加和移除本次端口映射。清理不完整时脚本不得报告成功。

## 测试入口

- 桌面：`src-tauri/src/recurrence_s3_integration_tests.rs`，仅 `cfg(test)` 编译；两个用例显式 ignored，由集成脚本带 `--ignored --exact` 运行。普通 `cargo test` 不会自动连接服务。
- 鸿蒙：`RecurrenceS3Integration.test.ets`，仅属于 ohosTest；只有 `aa test -s ns7S3 1` 才注册该专用套件。参数字典键保留 `-s ` 前缀，不修改生成的 TestRunner。
- 默认 `run-device-tests.ps1` 继续要求全部 17 项通过；集成脚本另用 `RecurrenceS3Integration: 1` 完整报告清单校验，不放宽默认校验器。

本次没有增加产品设置、网络权限、数据库版本、签名或实况窗行为。

## 证据与限制

首轮完整联调已在 Mate 80 Pro Max / HarmonyOS 7 模拟器通过。日志目录为 TEMP/`eggdone-ns7-4d89c5f8af644df4b1aab5fc407512dc`，包括桌面两段报告、鸿蒙原生报告、服务日志与镜像 ID；主包/测试包构建和 SHA256 在同期 `eggdone-device-tests-*` 目录。最终复跑记录见 [NS7 候选记录](NS7_REGRESSION_AND_RELEASE.md)。

源码检查工具本次未提供有效独立诊断：CLI lint 返回 Files checked: 0，不能算静态检查通过；有效证据是实际 ArkTS 构建及设备执行。没有声称 HarmonyOS 6 真机、云厂商 TLS、弱网、系统提醒或正式包升级通过。

首次环境启动失败和套件选择错误均保留失败记录。修复了 PowerShell 带点号原生参数需引用的问题和 Hypium 参数前缀读取；未为通过测试而更改生产传输逻辑或放宽断言。
