# 归档跨端完整同步回归与验收

日期：2026-09-18。阶段：LC1c 自动化补齐及用户确认收尾，纳入本次提交。双端归档产品入口不变；后续已开始 LC2a-1 兼容原型，仍没有清空回收站入口。

## 本轮交付

- 桌面增加 sync_core_archive_s3_tests.rs，并接入现有隔离 S3 runner 的 ArchiveRecoverySessions 模式。
- 鸿蒙增加 scripts/test-archive-sync-s3.cjs，使用生产 ArchiveRepository、TodoRepository.archiveCompleted 和完整 SyncService。
- 鸿蒙 host harness 补齐 ArchiveRepository 使用的 TextEncoder / SHA256 适配，原有 SHA1 适配保持；没有修改生产 ArkTS 文件。
- 两端共享 roadmap 的旧“下一步 LC1b-2b”文字已修正为实际 LC1c / LC2a 顺序。已有界面验收反馈保留，不重复基础测试。

## 实际测试路径

1. 桌面内存 SQLite 新库迁移后，准备两个已完成的旧式每日重复任务、已勾选清单和有效便签关联，通过生产归档函数归档，再经完整同步内核上传。
2. 鸿蒙生产 SyncService 从临时 S3 拉取，准备另外两个任务并通过生产 archiveCompleted 归档；对桌面来源任务分别取消归档、重新打开，重复调用相同 operation ID 不再写入。
3. 鸿蒙恢复后模拟断网，确认已写入任务不回滚、todos 仍待同步；解除故障后重试完成。
4. 桌面带着旧活动副本重连，验证鸿蒙恢复结果；第二个桌面测试副本修改归档任务并同步，使原预览失效，恢复旧预览返回 ARCHIVE_CONFLICT。
5. 桌面重新预览后恢复鸿蒙来源任务，验证同请求重试；用错误测试凭据上传失败，待同步标记不丢失；正确凭据恢复后同步。
6. 仍持有旧归档副本的测试端重连，不能撤销已完成的恢复；鸿蒙全新测试库再次拉取，确认双向最终状态一致。

任务数据只来自合成 fixture，不读取用户数据库。归档与恢复均调用生产持久层；用于构造并发变化的标题修改直接写测试数据库，不声称经过用户界面。

## 已验证的结果

- 取消归档保留完成状态和完成时间，重新打开清空完成状态与完成时间。
- 两种恢复均移除归档标记和旧提醒字段；重新打开不继承重复规则、下次日期或系列绑定。
- 原全天日期保留。鸿蒙数据库另存 UI 时间投影，不能误要求本机 due_at 为空；跨端 wire 仍保持全天日期语义。
- 清单勾选/内容和有效关联保留，任务总数始终四条，不生成额外后继或新重复规则。
- 旧归档副本重连、重复调用、并发标题变化、离线和凭据错误重试，均未覆盖更新内容或提前确认上传。
- 回归场景使用旧式 repeat_rule 历史任务。不是所有活动自定义重复规则或系统提醒行为的完整端到端证明；已有归档规则冲突等内核测试继续保留。

## 执行证据

- 新 ArchiveRecoverySessions 四阶段全部通过：桌面 prepare → 鸿蒙 exchange → 桌面 verify → 鸿蒙 verify。
- 原 TrashRecoverySessions 四阶段通过，确认共用 host harness 的 SHA256/TextEncoder 扩展不破坏回收站恢复。
- 桌面 cargo test --lib：344 通过、18 项按条件忽略；新增两个 S3 测试在上面的 runner 中显式执行，不把忽略当通过。
- 桌面 pnpm check：0 错误/0 警告；pnpm build、cargo fmt -- --check、cargo check 通过。原大 chunk、Rust dead-code 提示保留。
- 鸿蒙 test-archive-storage.cjs --desktop=D:/Develop/EggDone：23 个共享用例、新库/升级/导入与生产 JSON 往返通过。
- 鸿蒙 test-archive-panel.cjs、test-archive-batch-panel.cjs、test-archive-detail.cjs 通过。
- 本轮没有改生产 ArkTS/UI/资源，未重新构建或安装 HAP，未运行真机系统通知。

成功日志在当前 Windows 用户临时目录：

- eggdone-sync-core-8f6f907b2c6f48318042668e16035651：归档。
- eggdone-sync-core-bf207e11af4f4e8793d73195febdb675：原回收站回归。

初次新测试的失败是测试错误地要求鸿蒙全天日期本机时间投影为空；按现有 TodoDateMapping 与归档实现修正断言后完整重跑通过，没有修改生产日期逻辑。失败日志目录 eggdone-sync-core-7349119d13284c909f503009317dd83f 保留作排查依据，不计通过。

## 隔离与复现

复用已缓存的 chrislusf/seaweedfs:4.34 镜像；不下载镜像，绑定 127.0.0.1、随机命名测试桶、tmpfs 数据目录和公开的测试凭据。不读取用户配置，不使用已有 Docker volume。runner 校验自身容器的 run 标签后移除，两个成功测试的临时服务均已清理，日志不入库。

在桌面仓库运行：

```powershell
pwsh -NoProfile -File scripts/run-sync-core-s3.ps1 -ArchiveRecoverySessions -HarmonyRoot D:/Develop/EggDoneHarmony
pwsh -NoProfile -File scripts/run-sync-core-s3.ps1 -TrashRecoverySessions -HarmonyRoot D:/Develop/EggDoneHarmony
```

端口默认 18477；占用时通过 -Port 指定另一个空闲端口。runner 每次只允许一种跨端套件，缺少镜像或显式鸿蒙目录时拒绝运行。

## 验收边界与下一步

用户此前反馈鸿蒙底栏测试通过、其他测试正常；2026-09-18 在明确剩余验收为双端归档/取消归档/重新打开同步一致、旧提醒不重新触发及旧重复记录不推进后，回复“确认，开始”。据此记录本次业务范围验收通过，不推定设备型号、存储服务或主题/字号全组合覆盖。

自动化证据仍仅为生产同步代码 + 主机 SQLite + 隔离真实 S3，不冒充手机 ArkData 或系统通知测试。LC1c 本次范围收尾；未逐项验证的设备/主题/字号矩阵保留至发布回归，不重复已验收基础操作。后续进度见[LC2a 兼容原型](TASK_PURGE_COMPATIBILITY_PROTOTYPE.md)，不跳过门槛增加清空按钮，也不擅自迁移用户同步空间。
