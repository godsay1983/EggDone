# 桌面统一搜索原生运行验证

## 范围与隔离

2026-09-13 补 E8c3 的 Windows 原生主界面路径。使用真实 WebView2、Tauri IPC、Rust、SQLite 和附件存储，不替换 IPC 返回值，不使用浏览器 fixture 页面。

- 应用源码基线：`f40c1ca`；本次只新增测试和文档，不修改生产代码、版本或协议。
- 独立标识：`com.eggdone.searchtest`。脚本在任何数据操作前断言标识、同步关闭且未配置凭据，拒绝普通客户端。
- 独立构建目录：`src-tauri/target/search-native`，不覆盖普通 `target/debug/eggdone.exe`。测试前普通客户端 PID 52336 正在运行，测试后其 PID、路径和启动时间未变。
- 测试数据库：`%APPDATA%/com.eggdone.searchtest/eggdone.sqlite3`。测试样本使用每次唯一前缀；不读取普通客户端数据库、不清空或卸载任何应用、不接入 S3。
- 附件通过实际文件输入控件导入 Markdown 和原有应用 PNG，并由原生 API 回读校验字节。`setInputFiles` 不代表 Windows 文件选择对话框人工验收。
- 归档模拟只对本轮新建任务 UUID 执行绑定 UPDATE；保存失败由真实 SQLite 触发器注入，谓词限定本轮便签 UUID。失败注入表和触发器在恢复时删除，`finally` 兜底清理；测试样本保留在独立库中方便复查。

## 复现步骤

在桌面仓库根目录执行，先确认 9228 端口没有被其他进程使用。需要本机已有 Rust/MSVC、Tauri 依赖和 Playwright；`PLAYWRIGHT_PATH` 可指向已安装的 Playwright 包，不要求额外安装浏览器。

```powershell
$env:CARGO_TARGET_DIR = Join-Path $PWD 'src-tauri/target/search-native'
node node_modules/@tauri-apps/cli/tauri.js build --debug --no-bundle --config '{"identifier":"com.eggdone.searchtest"}'
if ($LASTEXITCODE -ne 0) { throw 'Isolated build failed' }

$exe = (Resolve-Path 'src-tauri/target/search-native/debug/eggdone.exe').Path
$testProcess = $null
try {
    if (Get-NetTCPConnection -LocalPort 9228 -State Listen -ErrorAction SilentlyContinue) {
        throw 'Port 9228 is already in use'
    }
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9228'
    $testProcess = Start-Process -FilePath $exe -WindowStyle Hidden -PassThru
    $ready = $false
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        if ($testProcess.HasExited) { throw 'Isolated app exited before CDP was ready' }
        try {
            $null = Invoke-RestMethod 'http://127.0.0.1:9228/json/version'
            $ready = $true
            break
        } catch { Start-Sleep -Milliseconds 500 }
    }
    if (!$ready) { throw 'CDP did not become ready' }
    node scripts/test-content-search-native.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Native search regression failed; inspect the printed report path' }
} finally {
    Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
    Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    if ($testProcess -and !$testProcess.HasExited) {
        $current = Get-Process -Id $testProcess.Id -ErrorAction Stop
        if ($current.Path -ne $exe -or $current.StartTime -ne $testProcess.StartTime) {
            throw 'Unexpected process identity; refusing to stop it'
        }
        Stop-Process -Id $current.Id -ErrorAction Stop
        $null = $testProcess.WaitForExit(10000)
    }
}
```

测试要求隔离应用使用中文，默认浅色。脚本记录实际语言、主题、CSS 视口和原生缩放；不是通过 CSS `zoom` 或模拟 viewport 冒充原生窗口。脚本退出只断开 CDP，外层负责结束自己启动的进程。

## 本次结果

最终证据目录：`%TEMP%/eggdone-search-native-1789238196652`。`report.json` 中 `passed=true`、12 项流程检查通过、`pageErrors=[]`；三档实测为中文浅色、CSS 360×560、原生缩放 100%/125%/150%。应用版本 1.0.10，Windows WebView2/Edge 152。

| 流程 | 原生证据 |
| --- | --- |
| 真实附件 | Markdown 与 PNG 实际存储字节匹配；另一个便签持有同名文件，用于排除按文件名误选父便签 |
| 分页任务返回 | 每轮 23 个测试任务；第一页 20 项，第二页 3 项；打开未归档任务，返回保留查询、任务类型、第二页和滚动位置 |
| 归档只读 | 全文与测试内容完全一致，包含末尾标记；没有编辑输入控件；数据库归档字段仍非空 |
| 文件定位 | 正确父便签标题；唯一高亮目标的 UUID 与文件一致，实际键盘焦点落到该节点；附件 UUID 和顺序不变 |
| 图片定位 | 同上；定位时图片查看器未打开，主动点击后加载 128×128 原图；Escape 先退到附件管理器，再退到编辑器 |
| 失效目标 | 已显示的附件结果，其父便签被原生 API 删除后拒绝打开，显示不可用；重新搜索排除该结果 |
| 三档缩放 | 原生窗口和 WebView 缩放命令生效；搜索面板未水平越界，每档完成文件定位和返回 |
| 搜索前保存 | 来源编辑器的未保存正文在打开搜索前写入真实 SQLite；关闭搜索回到该正文 |
| 来源保存失败 | SQLite 拒绝 UPDATE 时不打开搜索，草稿仍在，数据库仍是旧文；撤销故障后点击重试，正文确实落库 |
| 目标返回失败 | 从搜索打开的便签保存失败时不返回结果，草稿和旧库内容分别保留；重试后返回原查询和便签类型 |

表中三档缩放各计一项，因此报告共 12 项，不按表格行数另增通过数量。最后检查两项故障注入对象均不存在、同步仍关闭，重新加载 WebView 后附件 UUID 和顺序保持。测试进程 PID 8812 已结束，9228 无监听。

最终目录的 8 张截图已逐张查看：归档全文、文件/图片高亮、主动打开图片、三档缩放和来源保存失败。目标边框清楚，关闭与返回可操作；原图主动打开后显示正常。不能把这些局部截图扩大为所有页面和配置的视觉验收。

隔离可执行文件 SHA-256：`3A2A21A314EE136CBB76C5DCD5C19AA62A6235B5558B3C444B2AD9B09B754584`，生成时间 2026-09-13 02:26:59。产物和截图均留在构建/临时目录，不提交到 Git。

此前调试脚本有三处失败：样本备注超过 1000 字符、标题定位到了隐藏的搜索结果、便签返回按钮误用任务的“返回来源”文案。均修正测试数据/定位方式后完整重跑，不将这些记为产品缺陷或通过项。

提交前 `node --check`、`pnpm check`（0 错误/0 警告）、`pnpm build`、`pnpm test`（35 文件、306 项）、`cargo fmt -- --check`、`cargo check` 和 `git diff --check` 通过。Rust 保留既有 `TraySnapshot.locale` 未使用告警；本次没有修改该模块。

## 保留边界

- 本批不是中英文、深浅主题、所有窗口大小的完整原生矩阵；也未验收原生下拉菜单展开的系统绘制，既有浏览器配色证据不能直接替代。
- 搜索期间按契约不加载附件预览；未缓存图片显示占位，主动打开原图的路径已验证。本批未记录全部原生 I/O，不能仅凭查看器未打开宣称证明“零读取/零下载”，远程附件另验。
- 来源/目标保存故障使用 SQLite 注入，不代替真实磁盘已满、权限丢失或云端异常。目标返回用例为从来源搜索打开同一便签，不代表多层跨便签导航矩阵。
- 不覆盖用户旧库升级、长期大库、真实双端同步、物理手机/平板或最终用户验收；不关闭整个 E8c3、E8c2-H2 或其他阶段。
- 本轮补可重复的验收工具和证据，不额外扩大产品功能。
