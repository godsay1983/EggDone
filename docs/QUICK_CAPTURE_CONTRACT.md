# 快速收集契约 v1

日期：2026-09-05。对应 DNS4 / HNS4。此能力不改变 schema、S3 Object Key 或同步格式。

## 共享模型

`CaptureDraft` 包含 `target: todo | note`、`title`、`body`、`source_url`、`source_app` 和 `truncated`。
它仅存在于内存。接收、预览、取消均不创建任务或便签，不写入同步对象。进程退出后未确认草稿不恢复。

- 统一按 UTF-16 单元限制标题 100、正文 20000、来源名称 100；截断不得切断代理项对。
- CRLF/CR 转 LF；清除控制字符和双向文本控制符；标题换行折为单空格。
- 总输入超过 100000 单元直接拒绝。来源 URL 最多 2048，只接受 HTTP/HTTPS，无凭据、空白、控制符、反斜杠。不抓取网页，不读取路径，不执行传入内容。
- 提示截断，保留未截断的来源 URL。拼接来源后正文超限时禁止保存，允许用户编辑；不静默丢失链接。
- 任务备注上限 1000；超过时提示缩短或改存便签，而不静默裁掉内容。
- 来源名不可信，不用来证明来源应用身份，不记录 Want、正文或 URL 到日志。
- 正文中的链接和标记始终按纯文本处理，不解析 HTML，不自动识别日期。

## 确认与映射

确认层可切换任务/便签、修改标题与正文，并取消。禁止保存空内容。
标题未填写时：任务以正文第一行或来源 URL 的前100单元作为标题；便签保留空标题并使用原有列表摘要规则。
原文完整进入任务备注或便签正文；来源 URL 以单独一段追加，结尾已有相同 URL 时不重复追加。

任务可明确勾选“识别标题中的日期、分组和重要标记”；默认关闭以免分享文章误识别。
勾选时复用原有快捷新增解析器，并展示预览；未指定分组时不继承隐藏的当前筛选分组。
任务标题、备注、分组、日期、提醒、重复规则、重要级别一起提交；不先创建空任务再补详情。
便签复用 NoteStore.add 的普通保存和 dirty 流程。旧编辑器内容先按原有规则保存，不被外部草稿替换。

## 桌面端入口

设置中新增“快速便签快捷键”，默认关闭。可选择 Ctrl+Shift+N、Ctrl+Alt+N、Alt+Shift+N；
macOS 对 CommandOrControl 使用 Command。保留原任务快捷键，沿用系统注册冲突与恢复策略。

本轮选择等价命令行入口，**未注册 eggdone:// 协议**：

```powershell
& 'C:\Program Files\EggDone\eggdone.exe' --capture --target note --title '资料' --text '待整理文字'
& 'C:\Program Files\EggDone\eggdone.exe' --capture --target todo --title '明天看文档' --url 'https://example.com'
```

以上可执行路径仅为示例，以实际安装路径为准。字段使用独立参数，拒绝重复字段、未知字段和缺值。
首次启动入队，第二实例转发；先注册前端监听再读取队首，避免页面未加载漏收。
确认层不自动读取剪贴板，普通输入框粘贴由用户触发。

## 鸿蒙端入口

依据当前 SDK 的 Share Kit 文档“应用内处理分享内容”：
`onCreate/onNewWant` 接收 `ohos.want.action.sendData`，
通过 `systemShare.getSharedData(want)` 读取 `SharedRecord`，不从 onForeground 获取数据。
module skills 声明 `general.plain-text`、`general.hyperlink`，单记录，
仅消费 content 与可选 title。不接收文件/图片 URI，不新增权限。
现有 EntryAbility 只分发；解码与队列位于 CaptureInbox，纯校验位于 CaptureIntentParser。
确认层位于导航上层，保留已有页面；手机自适应宽度，平板居中限宽，内容滚动，操作区固定。

最多保留8个待处理入口。鸿蒙会合并队列中完全相同的文字/标题/URL；
关闭、保存仅消费匹配的队首 ID，重复关闭不会清除下一条。消费后再次主动分享相同内容属于新的收集。
不承诺系统提供跨进程永久唯一的分享 ID。接收队列不等同于已保存内容。

## 验证与限制

自动验证与真实平台验收分开记录：
- 桌面：`pnpm capture:check`、Vitest、Rust 保存事务/回滚及 CLI 测试。
- 跨端：`node scripts/check-capture.mjs --harmony=D:/Develop/EggDoneHarmony`，12项共享输入用例。
- 鸿蒙：LocalTest；`node scripts/test-capture-inbox.cjs` 验证 ShareKit 边界替身、异常恢复、重复事件与FIFO；
  `node scripts/test-reminder-completion.cjs` 使用生产迁移/Repository 与主机 SQLite 验证收集字段和 dirty。
- 浏览器：`scripts/test-capture-ui.mjs` 加载真实 Svelte 确认层，测试8组语言/主题/尺寸及取消/确认。
  使用 Playwright，可通过 PLAYWRIGHT_PATH 指定已有运行时，不属于 Windows 原生快捷键验收。
- 不把主机 SQLite、浏览器或 LocalTest 视为真实系统分享、快捷键或 S3/MinIO 验收。

## 人工验收

1. 桌面开启快捷便签键：隐藏、已打开设置/便签时唤起，取消不新增；键位冲突可恢复原配置。
2. 桌面首次启动和已启动时执行示例CLI：各生成一个待确认草稿。无参数普通启动保持原行为。
3. 鸿蒙浏览器分享链接、备忘录分享文字、至少一个第三方应用分享：冷/热启动都出现确认层。
4. 两端测试空内容、1000字任务备注、长链接、超长分享和重复触发；保存失败时草稿仍在，不能重复点击创建。
5. 鸿蒙手机与平板横竖屏、软键盘展开，检查保存/取消可达、不遮挡。切换中英文与深浅主题。
6. 确认保存后检查真实双向S3/MinIO同步、任务日期、分组与备注；鸿蒙任务卡片刷新。取消后对端不能多出记录。

本轮不修改已验收的实况窗、通知完成动作、签名或版本号。
