# 任务与便签关联：L3d1 原生 S3 集成

更新：2026-09-12。L3c 已本地提交：桌面 `94ec48b`、鸿蒙 `565a8f3`。本轮 L3d1 测试及文档增量未提交，不升版、不推送。

## 范围与边界

直接执行桌面生产 `TaskNoteLinkTransport`（Rust/rust-s3）和鸿蒙生产 `S3SyncClient`（ArkTS/NetworkKit/签名），通过独立 SeaweedFS 4.34 S3 服务交换关联记录。两端同时调用生产链接 Repository，鸿蒙在独立原生 RDB 中合并；桌面还调用备份恢复内核验证解绑保护。没有用 HTTP 响应替身代替该服务。

这是**真实传输、链接合并和墓碑交换集成**，不是完整应用同步验收：
- 不运行完整 SyncService 会话，不上传任务/便签实体，也不确认这些实体的 ACK。测试中的链接允许悬挂，不能据此认为已验证“实体先于链接上传”。
- “离线变更”是在网络读取前准备本地记录，再下载合并；没有模拟手机断网/弱网、后台挂起或系统恢复。
- 不触碰用户数据库、应用同步凭据或正式 S3 桶，不修改用户设置，不新增产品按钮。
- L3d2 完整会话与 L3d3 用户旧库/物理设备/文件选择器/云端 TLS 等验收继续未完成；关联 UI 仍为后续 L4。

## 验证顺序

1. 桌面创建本轮随机桶；使用中文与字面百分号路径验证 HEAD/GET 初次 404。
2. 桌面从独立 SQLite 链接仓库上传活动关联 A；If-None-Match 创建成功，再次使用旧不存在快照写入返回冲突。
3. 错误签名 HEAD/GET 返回拒绝，不当成空对象；本地数据与 revision/ACK/ETag 不变，正确凭据可再次读取。
4. 鸿蒙在网络读取前保存活动关联 B，下载 A 后合并，再为 A 写更新时钟为 300 的解绑墓碑。
5. 鸿蒙使用最新 ETag 上传 A 墓碑与 B，HEAD 标记改变；使用旧 ETag 上传活动 A 返回冲突，重新下载内容与仓库一致。
6. 鸿蒙错误凭据失败后本地状态不变；使用正确凭据重试恢复读取。独立路径验证原生创建 404 对象与旧不存在快照冲突。
7. 桌面预置时钟为 200 的本地活动 A，接收鸿蒙 A 墓碑与 B，A 保持解绑；再导入活动 A 时间戳为 5000 的备份，仍不复活本地解绑。
8. 桌面条件更新 B，验证 ETag 改变；旧 ETag 不能覆盖最新数据，最终读取与本地合并结果一致。
9. 清理本轮标签确认的临时容器与 hdc 映射。独立 RDB 由测试关闭并删除；不卸载应用或清空用户数据。

本用例不伪造已同步状态：未执行实体上传，鸿蒙链接 ACK 保持待同步且 ETag 为空。完整会话的 ACK 由后续 L3d2 验证。

## 执行入口

从鸿蒙仓库根目录执行：

```powershell
./scripts/run-s3-integration.ps1 -Device 127.0.0.1:5557 -DesktopRoot D:/Develop/EggDone -Port 18475 -TaskNoteLinks
./scripts/run-s3-integration.ps1 -Device 127.0.0.1:5555 -DesktopRoot D:/Develop/EggDone -Port 18475 -TaskNoteLinks
```

- 两次必须顺序执行；设备ID应先用 `devecocli device list` 确认。这里记录的是本轮实际手机/平板模拟器。
- 脚本只接受本机回环服务，使用本机已有固定镜像，不拉取镜像、不挂载生产数据卷；S3 数据在容器 tmpfs。公开测试凭据只用于本轮临时服务，不得用于对外服务。
- 默认不加 `-TaskNoteLinks` 时仍执行原 NS7 规则集成及默认17项原生测试，默认报告校验没有放宽。
- 关联模式先通过 `run-device-tests.ps1 -Suite TaskNoteLinks` 顺序构建并覆盖安装，验证5项关联原生RDB测试；之后单独运行1项网络集成套件。正常原生测试不访问临时S3。
- Rust 两项新增测试为 `cfg(test)` 且显式 ignored，仅脚本按 `--ignored --exact` 执行，普通 cargo test 不连接服务器。
- 鸿蒙专用入口为 `aa test -s taskNoteLinkS3 1 -s ns7Run ... -s ns7Port ...`；运行ID与端口校验沿用已有隔离测试机制，不修改生产资源配置。
- 每段报告必须恰好运行预期用例并成功；hdc退出0或只有构建成功均不算测试通过。

## 本轮结果

手机 Mate 80 Pro Max 模拟器和 MatePad Pro 13 模拟器分别完成：
- 主包 Debug、ohosTest 构建及覆盖安装。
- TaskNoteLinkNative：5/5，0失败/忽略。
- 桌面 prepare：1/1；鸿蒙 TaskNoteLinkS3Integration：1/1；桌面 verify：1/1。
- 所有临时容器和本轮端口映射均清理；再次查询两个设备的转发列表均为空。原有 globalmesh-seaweedfs 和 portainer 未修改。

其他回归：
- Rust 全量248项通过，4项独立S3测试默认忽略。本次新加的2项已在上述隔离运行中显式通过；原NS7规则集成没有在本轮重跑。
- 鸿蒙关联会话9组、传输43例通过。源码MCP检查0错误，保留测试抛错提示；实际ArkTS构建通过。
- 生产界面和主包源码没有新增行为，主包SHA-256与L3c一致，不要求用户为本轮测试重新做界面验收。

证据均位于本机 TEMP，不提交日志和构建产物：
- 手机S3：`eggdone-ns7-bbb459fb53654e238424eadae8fe5488`
- 手机构建/RDB：`eggdone-device-tests-bee9933853a740ff8f4899717d186038`
- 平板S3：`eggdone-ns7-7df431cc99a84ec887233240dff4c4be`
- 平板构建/RDB：`eggdone-device-tests-fc31b731905e4bc7aa53fce9d7a59d75`
- 首次失败记录：`eggdone-ns7-5b7a51b2dcce4aa1a370af842e84ea11`。测试比较未实现 PartialEq 的结构导致编译失败，改为逐字段断言后完整复跑；未改生产结构或放宽断言，失败临时容器已清理。

包SHA-256：
- 主包：`CD9D287312F4566651AAB3FEE2D7798F8B6CB03C4309B34EB72BCC6C8ABD69CF`
- 手机测试包：`848513B43A3F7660DD2FF226BCC1519E61FD35740B42A0389B1E2AE19C53B15F`
- 平板测试包：`B782B050ADDBE6BE6C12A179223FEEE6959FB44C0FD63E180A7B0D0EEA2EA82B`

## 下一步

L3d2：连接两端完整同步编排，验证实体先上传、链接dirty/ACK、冲突重试及失败后的重入。L3d3：在具备备份与隔离条件后验证实际旧库、物理设备及原生文件恢复。未取得对应证据前不能把L3整体或L5发布验收标为完成；既有E6人工验证缺口也继续保留。
