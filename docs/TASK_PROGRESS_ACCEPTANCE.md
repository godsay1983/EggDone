# 任务进展记录实现与验收记录

- 日期：2026-10-01
- 状态：TP-1 生产功能及自动化闭环完成；TP-2 原生设备及隔离真实同步验收尚未执行。
- 应用版本保持桌面 1.4.2、鸿蒙 1.5.3；数据库 schema 27，JSON/完整备份内层数据 v9。
- 前置协议提交：桌面 fcdc484；鸿蒙 74abd8e。下列功能及样式修正纳入本次本地提交，未推送或发布。

## 使用流程

任务菜单新增“进展记录”。添加纯文本记录后立即在面板中显示；已有记录可通过行菜单编辑或删除。非空输入关闭前确认，失败保留输入，结果不明确时重试同一操作，不另建重复记录。超过 30 条按创建时间和 UUID 分页；有记录的任务展示紧凑数量入口。

已完成任务仍可维护记录；归档和回收站只读，恢复任务后重新开放编辑。复制、模板创建和重复任务下一实例不继承记录。进展不改变任务日期、排序、今日计划、等待状态或提醒，也不作用于只读系统日程。

## 数据与兼容边界

- 独立同步对象由当前任务 Object Key 自动派生，无新权限、手填路径或空间迁移。
- 并发新增取并集，同条修改按协议收敛，记录删除清空正文且优先于晚到编辑。
- 曾存在的对象消失时拒绝重建；同步期间新增修改保持待同步，不用旧快照确认新写入。
- 无记录且远端从未存在时不上传空对象；无变化远端复用有效 GET 的 ETag，不新增 PUT。
- v9 备份始终包含进展域。旧 v1-v8 缺域保留本机记录；旧版本显式携带进展、新版本缺域、损坏内容均拒绝，导入事务整体回滚。
- 永久删除终态同时清理记录正文、本机操作回执和覆盖提示，旧同步副本及旧备份不能复活已知终态的数据。
- 操作回执只存请求摘要、标识和版本，不保存额外正文副本；ACK、凭据和未提交输入不进入备份。
- 旧版应用不理解 v9。导入新备份须升级应用；请保留升级前备份。旧版不写进展对象，但不能作为本功能跨端验收设备。
- 固定域的旧空间迁移/恢复流程遇到非空进展会安全拒绝，不以遗漏新域的快照完成迁移。正常同步和自动加入无需用户另选空间。
- 本功能清理应用控制的当前数据与同步副本，不承诺删除用户此前导出的文件、旧空间备份或 S3 服务端版本历史；这些副本需按其存储策略管理。

## 已执行的隔离验证

| 范围 | 结果与边界 |
| --- | --- |
| 鸿蒙生产 repository/snapshot/session/backup | 26 组通过；含空记录父任务终态使旧快照 ACK 失效；宿主 SQLite 和可控传输适配器，不等于设备 RDB |
| 鸿蒙进展 Store | 7 组通过；真实状态编排、请求身份和输入保留 |
| 鸿蒙 UI | 7 组 handler/source 检查通过；含远端仅改进展时触发同步、失败不消费观察值及旧目标探测丢弃；不是实际渲染验收 |
| Harmony → Rust → Harmony 数据库交换 | 通过：35 条记录、30 条分页、迟到父任务、编辑/删除、稳定重试、冲突、生命周期及终态 |
| 双端 v9 混合备份交换 | 9 组通过，执行本轮编译 Rust 测试入口与鸿蒙生产导入/导出；终态、故障回滚、原始数字及重复字段验证 |
| 既有今日计划/等待/任务生命周期 | 鸿蒙 10 组计划、10 组等待、4 组生命周期通过；历史库夹具按真实迁移版本生成 |
| 新域完整同步/自动加入 | 鸿蒙 18 组生产 session 集成通过；含仅远端记录发现、PUT 期间新写入、CAS 重试、目标切换、旧迁移安全拒绝与自动加入；内存传输不是实际云端 |
| 既有共享同步与迁移 | 鸿蒙共享 session 57 场景、迁移 preflight 40 场景通过；轮询状态 9 场景且双端源码一致；清理同步排队与重试回归通过 |
| 桌面前端 | pnpm check 零错误/警告；68 文件、571 项测试通过；i18n 1122 键、智能视图 89 向量与快捷捕获 12 向量通过；pnpm build 通过 |
| 桌面 Rust | 全量库测试 541 通过、0 失败、43 ignored；跨端交换中单独执行相应 ignored 入口，不将其他隔离/设备入口记为通过；cargo fmt、cargo check 及内嵌页面 Debug 构建通过 |
| 桌面浏览器 | Playwright 24 组布局、15 项行为、12 组备份预览通过；模拟 IPC 不等于 Tauri 原生验收 |
| 桌面既有列表回归 | 今日计划 60 场景；回收站 24 布局及 4 个清理/迁移场景通过；完成区保持静态标题及独立进展入口，迁移组件仅隔离夹具挂载 |
| 鸿蒙 Debug 构建 | 最终 devecocli build 成功，33 tasks；依赖中仍有异常处理和废弃接口提示 |

交换脚本要求已编译 Rust 测试二进制；没有该参数会失败，不静默跳过另一端。测试仅使用临时目录、内存数据库和合成任务，不读取用户实际数据库、凭据或同步桶。

## 尚未执行

1. 鸿蒙手机、平板、键盘及大字体下的真实渲染；明暗主题与系统返回手势。
2. Windows 原生 Tauri 窗口内的输入、失焦、托盘和重启持久化验收。
3. 隔离真实 S3 上两端离线新增、同条冲突、错误凭据、断网和目标切换的完整 A02-A10 矩阵。
4. 真机往返同步、同网络延迟及大量记录性能测量。

未执行项保留在 TP-2，不以宿主测试、浏览器模拟 IPC 或成功打包代替原生验收。

## 本轮测试程序

- 桌面：D:/Develop/EggDone/src-tauri/target/debug/eggdone.exe，内嵌本轮构建页面，不依赖 Vite 开发服务器。使用 cargo build --offline --features tauri/custom-protocol 构建，不启动应用或迁移用户库。
- 鸿蒙：D:/Develop/EggDoneHarmony/EggDone/entry/build/default/outputs/default/entry-default-signed.hap。未安装到模拟器或真机。
- 浏览器截图：C:/Users/caozhipeng/AppData/Local/Temp/eggdone-progress-ui-1790836247802。临时证据非发布资产，不提交构建产物。
- 编译提示：桌面存在既有 dead_code 和前端大 chunk 提示；鸿蒙存在依赖异常处理/废弃 API 提示，未宣称告警清零。
- 桌面程序已核对包含本次样式修正版页面资源 2.6lyneKhb.js；SHA256：1A2AF8C4B433254ABFD6DDDF773F15E498E7C8687047FA37049B88DFCDCC4403。运行前建议保留已有数据备份；本轮未启动测试程序。

## 界面反馈修正（2026-10-01）

- 桌面：新增/编辑标题和输入文字显式使用前景色，避免继承首页 footer 的弱文字色；输入背景和不透明提示文字单独适配主题，字数计数同步提高对比度。
- 鸿蒙：进展操作入口采用填充胶囊、12sp 字号、30vp 高度和同组背景/文字色；进展计数与 StatusChip 使用相同的 24vp 最小高度、11vp 圆角及 8/4vp 内边距，保留点击和自动换行。
- 桌面本轮 pnpm check 零错误/警告，pnpm build 和内嵌页面 Debug 构建通过。浏览器 24 组布局、28 组编辑区对比度（标题、正文、提示及计数均至少 4.5）、15 项行为、12 组预览全部通过；明暗主题空面板截图已人工检查。
- 第一次浏览器运行遇到同目录 check/build 重写 tsconfig 导致页面重载和预览等待超时；待其他命令结束后完整重跑通过，不跳过预览断言。
- 本轮截图：C:/Users/caozhipeng/AppData/Local/Temp/eggdone-progress-ui-1790838698318。鸿蒙 7 组 handler/source 检查通过，增加按钮样式与标签尺寸一致性约束；Debug 构建成功。
- 此次样式修正仅改展示和测试；未改存储、同步、版本，纳入本次本地提交，未安装鸿蒙包。鸿蒙真机/平板实际对齐仍待用户验收。

## 复现命令

```powershell
# D:\Develop\EggDone
pnpm check
pnpm test
pnpm build
# D:\Develop\EggDone\src-tauri
cargo fmt -- --check
cargo check --offline
cargo test --offline --lib
# D:\Develop\EggDoneHarmony
node scripts/test-task-progress-storage.cjs
node scripts/test-task-progress-store.cjs
node scripts/test-task-progress-ui.cjs
node scripts/test-sync-progress-integration.cjs
node scripts/test-recurrence-sync-session.cjs
node scripts/test-remote-poll-state.cjs --desktop=D:\Develop\EggDone
node scripts/test-migration-preflight.cjs --peer=D:\Develop\EggDone
node scripts/test-purge-sync-coordination.cjs
node scripts/test-task-progress-cross-client.cjs --rust-test-binary=<本次生成的 eggdone_lib 测试二进制绝对路径>
node scripts/test-task-progress-backup.cjs --desktop=D:\Develop\EggDone --rust-test-binary=<同上>
# D:\Develop\EggDoneHarmony\EggDone
devecocli build --build-mode debug
```
