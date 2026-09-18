# 迁移生产校验与真实 S3 多对象验证

日期：2026-09-18。阶段：LC2a-2b-1，已本地提交：桌面 ca30607、鸿蒙 330b411；未推送。后续本机门槛见[本机预检查](TASK_MIGRATION_LOCAL_PREFLIGHT.md)，本文保留本阶段验证边界。

## 本轮交付

在已有迁移日志原型上接入两端生产解析器，并完成隔离真实 S3 的六阶段迁移验证。仍为测试代码，不是应用迁移命令、正式备份格式或清空入口；不读取用户数据库、文件或云配置。

- 镜像原型 scripts/migration-journal-prototype.cjs：绑定存储目标摘要，支持在 capture、prepare、load/confirm 和恢复执行时调用领域校验器。
- 镜像 host 测试：补充外来存储目标和重启时校验器拒绝的回归。
- 鸿蒙 scripts/migration-production-s3-adapter.cjs：复用生产 Object Key 派生、文档解析/校验、S3 签名与 HTTP 请求。
- 鸿蒙 scripts/test-migration-journal-s3.cjs：真实条件写入、错误凭据、源版本变化及两类丢失回复后的日志恢复。
- 桌面 src-tauri/src/sync_core_migration_s3_tests.rs：生产构造器/有效 fixture 生成来源，两端副本逐字节比较并用 Rust 生产解析器复核。
- 桌面 scripts/run-sync-core-s3.ps1 新增 MigrationJournalSessions 模式，复用已有容器所有权检查及清理。

## 目标与校验绑定

原型计划 expected 新增 binding，以 endpoint、region、bucket、主 Object Key、pathStyle、allowHttp 的有序数组计算 SHA-256；不包含凭据。capture、源版本重查和执行 I/O 前后检查 binding，并继续检查本机目标 epoch/revision。

测试原型 format 从 eggdone.migration.fixture.v1 改为 v2。旧原型计划缺少 binding 会被拒绝，需要在隔离数据上重新预览；不提供静默补值。这不是生产同步版本、数据库 schema 或备份 v5 升级。

正式应用还需要将绑定接入同步设置的真实 epoch、锁和凭据切换流程，本轮不是这部分接线的证明。

### 复用的生产代码

8 个元数据对象覆盖任务/分组、便签、附件元数据、重复规则、关联、清单条目、清单定义和任务模板：

| 领域 | 桌面解析/校验 | 鸿蒙解析/校验 |
| --- | --- | --- |
| 任务/分组 | sync::validate_document + serde | S3SyncClient.normalizeDocument + SyncMergeService |
| 便签 | note_sync::validate_document + serde | S3SyncClient.normalizeNoteDocument + NoteSyncMergeService |
| 附件元数据 | note_attachment_sync::validate_document + serde | NoteAttachmentSyncMergeService |
| 重复规则 | recurrence_protocol::parse_document | parseRecurrenceDocument |
| 任务便签关联 | task_note_link_protocol::parse_document | parseTaskNoteLinkDocument |
| 清单条目/定义 | task_checklist_protocol::parse_items / parse_definitions | parseChecklistItems / parseChecklistDefinitions |
| 模板 | task_template_protocol::parse | parseTemplates |

鸿蒙 host 调用生产类的私有标准化/请求方法是测试适配，不新增应用公开 API。原字节备份不被标准化结果替换，未知字段和原始排版不因此丢失。

每个领域分别注入无效身份，两端现有解析器均拒绝。此证据不证明所有畸形输入和跨领域依赖均已完整覆盖；未来正式迁移仍需校验规则当前实例、清单定义、孤立引用等组合。

Object Key 通过两端真实派生器得到，再与当前原型固定 account/todos.json 拓扑核对；不支持的配置拒绝运行。尚未完成任意用户 Object Key 的通用迁移适配。

## 六阶段真实传输

1. 桌面 prepare：新建随机隔离桶。生产任务/便签/附件构造器、关联快照和已有有效 fixture 生成 8 份文档，逐一生产校验后上传；另上传 1 份合成文件原始字节。不是运行完整应用同步或用户界面。
2. 鸿蒙 exchange：生产解析器复核 9 类记录，逐域拒绝非法身份。真实源对象被测试改成非法身份时 capture 拒绝；源内容在预览后改变时执行拒绝。恢复测试源后重新预览、确认。错误凭据不当作缺失。真实 If-None-Match 拒绝覆盖，复制中途模拟成功回复丢失，保存 confirmed 状态退出。
3. 鸿蒙 publish：新 Node 进程从同一 SQLite 备份继续，复核已有副本，不重写源对象。条件创建发布标记成功后模拟回复丢失；本机目标仍为 legacy。
4. 鸿蒙 resume：另一进程读取发布证明、校验全部副本，事务切换合成目标并标记 complete。逐项核对源 ETag 没有因迁移变化。
5. 桌面 verify：逐字节比较全部元数据与二进制副本，校验长度与哈希，并重新调用 Rust 生产解析器；随后使用合法旧格式向旧主对象写入晚到修改。
6. 鸿蒙 verify：新快照仍是原内容，生产解析器继续通过；已完成操作重试不重置目标，不吸收旧空间晚到内容，也不改发布标记。

丢失回复在服务端成功写入后由测试适配抛错，不是宣称模拟了真实网络设备故障。不同阶段是真实独立 Node 进程；25 个强制终止恢复时点仍来自上一阶段的合成对象存储测试，未把两类证据混为真实 S3 强制断电测试。

## 执行结果

- MigrationJournalSessions 六阶段全部通过，runner 成功退出并移除自己的容器。
- 日志及仅含合成数据的 SQLite 证据：C:/Users/caozhipeng/AppData/Local/Temp/eggdone-sync-core-eda72e7503864c439f585d56af551269。
- 两端各 44 项 host 迁移检查通过，分别覆盖 25 个测试子进程终止恢复时点。
- 两端原终态合并原型各 29 项检查、125 组合并结合性回归通过。
- 桌面 cargo test --lib：345 通过、22 项按条件忽略。新增两个 S3 测试已在 runner 中显式运行；1 个解析器用例纳入普通单测。
- 桌面 pnpm check：0 错误/0 警告；pnpm build、cargo check、cargo fmt -- --check 通过。既有 chunk 大小和 dead-code 提示保留。
- 本轮未修改生产 ArkTS/UI/资源，未构建或安装 HAP，未操作已安装客户端。

开发中新增 Rust 测试最初将包含故意非法 UTF-16 的整个 checklist fixture 解析为 Value，导致测试构造失败；已遵循现有测试使用 RawValue 只读取有效样例。没有更改 fixture 或放宽生产解析器。

复现命令，在桌面仓库运行：

~~~powershell
pwsh -NoProfile -File scripts/run-sync-core-s3.ps1 -MigrationJournalSessions -HarmonyRoot D:/Develop/EggDoneHarmony
~~~

仅使用已缓存 SeaweedFS 4.34、loopback、随机测试桶、tmpfs 和公开合成凭据，不下载镜像、不使用已有卷，不触及用户存储。SQLite 证据留在临时日志目录，不提交。

## 仍未完成

本轮完成 LC2a-2b 的生产解析/固定路径/真实多对象传输部分，不能把整个 LC2a-2b 或 LC2a 标为完成。

下一子阶段 LC2a-2b-2：
- 本机未同步编辑、待上传附件及真实备份盘点与迁移前门槛，连接生产 dirty/ACK/revision；不能仅检查云端就视为已汇合。
- 任意受支持 Object Key 的映射、正式源目标标识和备份完整性规则。
- 现有领域的依赖一致性、终态领域与真实新空间导入/加入流程。
- 旧空间晚到内容核对和正式回退限制。

后续 LC2a-3 才完成原生迁移接线、已发布旧版及手机/平板验证。仅靠 ETag 重查仍不能冻结旧客户端，也不能形成 8 个源对象的原子一致快照。正式清空、物理清理、生产协议冻结及用户数据迁移均未开始。
