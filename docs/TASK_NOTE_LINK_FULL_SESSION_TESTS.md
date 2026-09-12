# 完整同步内核验证

更新：2026-09-12。所属阶段：E7/L3d2。L3d2a桌面内核与L3d2b跨端完整会话自动化已完成，原生用户环境验收仍在L3d3，不据此认定L3整体通过。

## L3d2b 跨端完整会话

- 桌面L3d2a已提交 `de60450`，鸿蒙文档 `9fd7038`。本轮新增桌面准备/复核阶段及鸿蒙宿主测试，未改生产ArkTS、协议、schema或UI。
- 四阶段通过：桌面真实内核上传任务/便签/关联/文件；鸿蒙真实SyncService合并离线记录、解绑和修改；桌面真实内核确认墓碑/实体/附件改名并再次修改；全新鸿蒙数据库再次同步并核对最终状态。
- 鸿蒙加载生产SyncService、SyncDocumentService、NoteSyncDocumentService、NoteAttachmentSyncDocumentService、关联与附件Repository、SyncRuntimeCoordinator及S3SyncClient。没有替换这些业务服务为固定响应。
- 实际S3条件写冲突：第二个签名请求在原PUT前更新对象，S3返回真实412；一次冲突重新上传任务/便签/关联后成功，两次冲突停止并保留links待同步，下一会话可恢复。核对两轮完整请求顺序与403/412真实响应。
- 附件元数据读取前注入离线错误，已确认的实体/关联保持确认，附件仍待同步，恢复后清除；实际错误签名返回403，不清除本地修改，恢复凭据后成功。另验证跨SyncService实例互斥、成功/失败记录、错误摘要不含凭据和地址。
- 宿主适配范围：RDB使用独立Node SQLite；NetworkKit接口由Node HTTP承接真实签名请求；安全存储仅持有公开测试凭据；SHA1使用Node crypto；原生文件读取被显式拒绝，原生时区转换不在本套范围。鸿蒙验证的是附件元数据，不声称验证其原生二进制上传/文件选择器。
- `cargo test --lib`：253通过、7项隔离测试默认忽略；本轮显式运行新增准备/复核各1项、原单桌面S3模式1项。鸿蒙四阶段中exchange/verify均有完成标记，既有9项关联会话、42项完整会话/配置回归通过。桌面check/build/fmt/check通过。无ArkTS修改，未重新安装设备或将宿主结果标注为原生测试。
- 最终跨端证据：`C:/Users/CAOZHI~1/AppData/Local/Temp/eggdone-sync-core-3768e1836ee94257901276dded4ae7df`；查看`full_session_prepare.log`、`harmony-exchange.log`、`full_session_verify.log`、`harmony-verify.log`。首轮附件测试的旧时间戳被合并逻辑正确拒绝，改为递增时间戳后重跑通过。

在桌面仓库运行跨端模式：

```powershell
./scripts/run-sync-core-s3.ps1 -Port 18477 -CrossClientSessions -HarmonyRoot D:/Develop/EggDoneHarmony
```

同一脚本默认模式仍只运行桌面实例；两种模式均限制随机隔离桶与loopback，完成或失败后清理本轮容器，不影响已有服务。下方L3d2a结果保留为历史证据。

## 当前结果

- L3d1 已提交：桌面 `6dd3111`、鸿蒙 `7092be2`，原生关联传输证据仍见 [S3集成](TASK_NOTE_LINK_S3_INTEGRATION.md)。
- 桌面生产 `sync_now_inner` 改为接收数据库、同步运行时、附件仓库及通知回调，保持原通知调用位置；没有复制一份同步算法供测试使用。
- 5 项完整内核自动化通过：实体先上传再上传关联；关联冲突最多两轮且重新上传实体；文件成功但附件元数据失败后的局部确认及恢复；附件元数据冲突预算及上传中编辑；目标切换后的旧回执失效。
- 真实隔离 S3 用例通过：两个独立桌面内核实例离线创建不同任务和便签、合并、较新的解绑覆盖旧活动关联、上传 Markdown 文件并在另一实例下载和校验、错误凭据不清除待同步状态、恢复凭据后双向收敛。
- `cargo test --lib`：253 通过、0 失败、5 个隔离 S3 用例默认忽略。本轮显式执行其中新增的完整内核 S3 用例，1 通过。已有另外4项的本轮未重跑，不将默认忽略描述为失败或本轮通过。
- `pnpm check`、`pnpm build`、`cargo fmt -- --check`、`cargo check`通过，保留现有 `TraySnapshot.locale` 未使用字段告警。

## 复现

在桌面仓库运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib commands::sync_core_tests
./scripts/run-sync-core-s3.ps1 -Port 18477
```

需已有本机 Docker 镜像 `chrislusf/seaweedfs:4.34`；脚本不拉取镜像，不使用用户凭据，不连接用户存储桶。脚本仅监听127.0.0.1，使用随机桶名、公开测试凭据、独立临时数据库和附件目录、tmpfs；退出时按随机标签核验并移除本轮容器，保留日志。端口被占用会拒绝启动，可选择另一个端口。

成功证据目录：`C:/Users/CAOZHI~1/AppData/Local/Temp/eggdone-sync-core-d1c6827011764427bbacfe8a44b37de8`。查看其中 `desktop-sync-core.log`、`image.txt` 和 `server.log`。首轮 S3 测试使用了不符合便签协议的设备标识，已改为数据库生成的 UUID 后重跑通过，没有放宽生产校验；故障轮容器亦已清理。

## 验证边界与下一步

- 这不是完整客户端 UI/系统凭据仓库/托盘通知的端到端测试。测试调用真实内核并持有同一个运行时锁，但不执行 Tauri 命令外层的凭据读取及成功/失败记录逻辑。
- 两个桌面实例本身不等于桌面与鸿蒙；本轮L3d2b另行提供真实鸿蒙业务服务的宿主跨端证据，仍不等于物理设备端到端验收。
- L3d2a冲突和晚到回执使用可控HTTP故障响应；L3d2b另外验证实际S3条件写冲突。二者都不是用户真实云端环境的并发验收。
- 尚未验证用户实际旧库、物理设备断网、用户 S3/TLS 环境或原生文件选择器恢复。L3d3 继续保留，不以本轮自动化代替。
- 无界面、schema、同步协议或版本变化；鸿蒙L3d2a仅同步文档，L3d2b新增宿主测试脚本，未重新构建或安装。尚未开放 L4 关联入口。
