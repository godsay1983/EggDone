# 回收站跨端回归记录

## E8a3a 范围与结果

2026-09-12 完成两端生产同步内核与恢复接口的隔离 S3 回归。桌面调用实际 Rust/SQLite 同步内核与 trash::restore；鸿蒙执行生产 ArkTS SyncService、TrashRepository 和迁移，平台存储适配为宿主 SQLite。不是 Tauri 窗口到 ArkData 真机的端到端验收。

本轮没有修改生产数据协议或恢复逻辑；增加可重复执行的测试、编排入口和验收记录。新增普通单元测试命令会跳过需要 S3 的两项测试，必须执行下面的专用脚本并核对四阶段完成标记，不能将 ignored 当成通过。

## 执行方法

在桌面仓库运行（HarmonyRoot 指向对应的鸿蒙源码）：

```powershell
pwsh -NoProfile -File scripts/run-sync-core-s3.ps1 -TrashRecoverySessions -HarmonyRoot D:/Develop/EggDoneHarmony
```

需要 Docker、Cargo、Node，且本机已有 chrislusf/seaweedfs:4.34。脚本不拉镜像、不访问用户凭据；仅向 127.0.0.1 暴露临时端口，在带唯一运行标识的桶和 tmpfs 中操作，退出时核验容器标签并删除测试容器。默认端口 18477，冲突时使用 -Port 指定空闲端口。与 -CrossClientSessions 互斥，不改变原有同步测试流程。

## 四阶段验证

| 阶段 | 已验证内容 |
| --- | --- |
| 桌面 prepare | 两组独立任务/便签、一份真实 Markdown 附件上传；第一组删除并同步，保留附件原件 |
| 鸿蒙 exchange | 读取桌面删除记录与附件预览，恢复原 UUID；已完成状态和日期保留，旧提醒/内置重复/系列绑定清空；重复确认拒绝；恢复后模拟断网保留脏状态，重试成功；删除第二组并同步 |
| 桌面 verify | 旧活动副本重新上线不会覆盖恢复；读取鸿蒙删除记录并反向恢复；另一份旧删除副本上线不重新删除恢复内容；旧关联墓碑保留；附件远端下载及哈希校验通过 |
| 鸿蒙 verify | 新迁移数据库导入最终双端恢复状态，任务/便签无删除标记，旧关联仍为墓碑；附件仅为 remote_only，不伪造本机原件 |

桌面 verify 另物理删除隔离桶中的测试附件原件，再调用生产下载接口确认失败。附件元数据继续存在，不能据此承诺原件可恢复。删除状态由测试库 SQL 准备，不把它宣称为原生点击删除流程覆盖。

通过标记：
- TRASH_SESSION_DESKTOP_PREPARE_OK
- TRASH_SESSION_HARMONY_EXCHANGE_OK
- TRASH_SESSION_DESKTOP_VERIFY_OK
- TRASH_SESSION_HARMONY_VERIFY_OK

本次证据目录：C:/Users/CAOZHI~1/AppData/Local/Temp/eggdone-sync-core-a7a8053eac1e472b9045cc78749fb0fb。该目录为本机临时证据，不纳入版本库。成功退出已确认测试容器清理。

## 其他检查

- 原有 -CrossClientSessions 四阶段回归也通过：完整同步顺序、真实 S3 条件冲突及有限重试、附件阶段断网、错误凭据与恢复，确认新开关未影响旧测试入口。证据目录末级为 eggdone-sync-core-b407ddec6ff54d87a5f9e77646bb10ec，测试容器已清理。
- 桌面 pnpm check（0 错误、0 警告）、pnpm build、cargo fmt -- --check、cargo check 通过；Rust 保留既有 TraySnapshot.locale 未读取警告。本轮未改鸿蒙应用源码，沿用 a3bbb7d 已通过的 Debug 构建与覆盖安装结果，不宣称重新执行了鸿蒙构建。
- 桌面 cargo test --lib trash_：3 项通过，含共享 11 场景与磁盘库关闭重开；2 项隔离 S3 测试在上述四阶段中另行执行通过。
- 鸿蒙 node scripts/test-trash.cjs：11 组共享场景、分页、非法输入、调用者快照隔离通过；test-trash-store.cjs 的重复提交与写入/刷新失败分流通过。
- 平板模拟器（MatePad Pro 13，2880×1920）：在现有库打开回收站、任务恢复预览、取消返回列表通过，未点击确认恢复、未改既有记录。本轮未完成手机回收站验证。
- 宿主重开库不等于用户历史版本迁移验收。通知实际触发、撤销与界面生命周期仍需原生验证。

## E8a3b 待验收

使用新建、可辨认的 QA 记录，不操作有价值的原内容：

1. 两端各新建任务和带附件便签，删除后在另一端回收站预览、取消、再确认恢复；检查原 UUID、日期和完成状态，附件能实际打开。
2. 使用旧版本形成的用户库覆盖升级；不卸载、不清除数据。检查删除记录可见，恢复后关闭重开仍保留。
3. 另一端在预览期间更新/再次删除同一记录，旧预览应拒绝提交并要求重新预览。
4. 真机离线恢复、联网重试、另一台长时间离线设备重新上线，确认不重复创建、不复活旧关联、不重启旧重复规则或提醒。
5. 手机、小平板、横屏和分屏，覆盖长正文、附件名称、中英文、大字体以及恢复中返回/重复点击。
6. 在专用测试桶中验收缓存已清理与远端原件已清理两种情况；元数据恢复后缺少原件必须正确提示，不显示文件已下载。

E8a3a 完成不关闭 E8a3 或整个 E8a。E8a3b 与 E6/E7 既有人工门槛保留；缺少人工条件时，后续开发可进入 E8b 便签本地历史，不将等待人工测试伪装为已通过。
