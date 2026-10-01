# 任务进展记录发布准备

- 日期：2026-10-01；状态：用户核心验收通过，本机候选包已生成，未推送或发布。
- 版本：桌面 1.5.0；鸿蒙 1.6.0 / 1000036，buildVersion 1。
- 数据：schema 27，JSON/完整备份内层 v9，完整备份外层 v1，进展与系统日历共享格式 v1。
- 升级前保留备份；旧客户端不能导入 v9。新进展对象自动派生，无新增权限或迁移步骤。

## 本轮回归

- 桌面：i18n 1122 键、智能视图 89 向量、快捷捕获 12 向量通过；check 零错误/警告；前端 571 项通过；生产页面构建通过。
- Rust：fmt/check 通过；全量库测试 541 成功、0 失败、44 ignored。跨端数据交换、混合备份和新性能入口分别显式运行，未将其他 ignored 项视为通过。
- 浏览器：24 布局、28 对比度、15 行为、12 备份预览通过，截图位于 C:/Users/caozhipeng/AppData/Local/Temp/eggdone-progress-ui-1790841284224。
- 鸿蒙宿主：116 共享协议向量及 20000 条/16 MiB 边界、26 存储组、7 Store 组、7 UI 检查、18 同步集成组、57 既有 session 场景、9 轮询场景、40 preflight 场景和清理排队检查通过。
- 新编译 Rust 与鸿蒙生产 repository 的往返交换通过；v9 混合备份/永久删除 9 组通过。
- 首轮 pnpm release:check 在并发构建期间出现两个 5 秒测试超时及一个后续状态断言失败；pnpm test --maxWorkers=2 完整重跑 571 项通过。其余检查、生产构建、fmt/check 和 Rust 测试单独执行通过，不将首轮整条命令记为成功。
- 既有 dead_code、大 chunk、依赖异常处理和废弃接口提示仍在；未宣称告警清零。

## 隔离大量记录检查

Windows x64；单个合成任务，正文为短文本，每页 30 条。首屏和聚合计数取 5 次中位数，无变化同步取 3 次中位数。Rust 为 Debug + 本地 HTTP，ArkTS 为宿主 SQLite + 内存传输；测量仅进展域及最终回执，不含完整应用同步、真实 RDB 或互联网。

| 客户端 | 条数 | 首屏 ms | 聚合计数 ms | 完整分页 ms | 无变化同步 ms | 页数 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust Debug | 1000 | 3.26 | 1.06 | 126.83 | 895.85 | 34 |
| Rust Debug | 10000 | 9.37 | 13.64 | 3871.70 | 9033.10 | 334 |
| Rust Debug | 20000 | 13.82 | 21.00 | 13270.12 | 17769.02 | 667 |
| ArkTS 宿主 | 1000 | 1.15 | 0.38 | 31.05 | 304.10 | 34 |
| ArkTS 宿主 | 10000 | 1.14 | 3.68 | 992.42 | 2442.12 | 334 |
| ArkTS 宿主 | 20000 | 2.06 | 5.85 | 2990.01 | 5072.71 | 667 |

三档文档均为 238708 / 2416710 / 4866710 字节。两端均完整分页，无重复/遗漏，计数一致；每档三次无变化同步为 3 GET、0 PUT。此处通过的是完整性和请求数断言，没有将延迟设为通过阈值。整域合并/严格校验在极端数据量下成本明显；Debug 结果不能当作发布版耗时或证明两端真实速度差异。真实发布版、同网络和真机计时仍需专门覆盖。

第一次性能运行复用了只等下一次请求 5 秒的 HTTP 测试服务，大数据 CPU 处理使服务先结束；夹具改为每次测量独立服务后通过。未调整应用超时或协议安全检查。

证据：C:/Users/caozhipeng/AppData/Local/Temp/eggdone-progress-performance-Pl8rd7/results.json。合成输入和结果由测试生成，不包含实际任务、日程或凭据。

复现（先在桌面编译当前测试二进制，再在鸿蒙仓库执行）：

```powershell
# D:/Develop/EggDone/src-tauri
cargo test --offline --lib task_progress_cross_client_tests --no-run
# D:/Develop/EggDoneHarmony
node scripts/test-task-progress-performance.cjs --rust-test-binary=D:/Develop/EggDone/src-tauri/target/debug/deps/eggdone_lib-5b77e58dfd73106c.exe
```

## 本机发布包

| 文件 | 大小（字节） | SHA256 |
| --- | ---: | --- |
| D:/Develop/EggDone/src-tauri/target/release/bundle/nsis/EggDone_1.5.0_x64-setup.exe | 9300641 | 436AC1FB46C302F5F48B30D87244885FEF58D87E0198DEDEF322612E7732342B |
| D:/Develop/EggDone/src-tauri/target/release/eggdone.exe | 28330496 | 834FDDC5B5315C9512BABDBCFB23B9F4E9F75656EE03BE4CEDC5FAC1932189B5 |
| D:/Develop/EggDoneHarmony/EggDone/build/outputs/default/EggDone-default-signed.app | 5837172 | CB2ABD852ABA284EBBAEE65D838FCA49A2FCB710621956E13785039B1FEB8A0E |
| D:/Develop/EggDoneHarmony/EggDone/entry/build/default/outputs/default/entry-default-signed.hap | 7981180 | 5404BC30D06640CE42E1081D93BFE5765CDBFF2CD4759FF4377AEBEEC5F8DC87 |

- Windows：pnpm build:windows 成功，x64 NSIS；可执行文件和安装包 ProductVersion/FileVersion 均为 1.5.0；原生可执行文件包含当前页面资源 2.CtGbGBx3.js。
- 鸿蒙：devecocli build --product default --build-mode release 成功，42 tasks；APP 内 pack.info 和 HAP module.json 均核对 1.6.0 / 1000036，内嵌 HAP debug=false。沿用现有签名配置，未创建或更改签名材料，商店提交继续遵守既有发布签名流程。
- 新包未安装、未运行到用户数据库、未上传商店或真实同步桶。本轮未生成 Linux 安装包。
- 输出目录是构建缓存，下次构建可能覆盖；重新构建后应重新计算校验值，不能继续沿用本表。
- 发布包覆盖安装、完整真机/平板/主题/字号、同网络计时和实际云端故障矩阵不由成功打包代替。用户确认核心测试通过已单独记录，不据此伪造逐项矩阵。
