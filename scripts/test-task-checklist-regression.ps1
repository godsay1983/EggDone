param(
  [string]$DesktopRoot = '',
  [string]$HarmonyRoot = '',
  [switch]$SkipUi,
  [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$checkout = Split-Path $PSScriptRoot -Parent
if (!$DesktopRoot) {
  $DesktopRoot = if (Test-Path (Join-Path $checkout 'src-tauri')) { $checkout } else { Join-Path (Split-Path $checkout -Parent) 'EggDone' }
}
if (!$HarmonyRoot) {
  $HarmonyRoot = if (Test-Path (Join-Path $checkout 'EggDone/entry')) { $checkout } else { Join-Path (Split-Path $checkout -Parent) 'EggDoneHarmony' }
}
$DesktopRoot = (Resolve-Path -LiteralPath $DesktopRoot).Path
$HarmonyRoot = (Resolve-Path -LiteralPath $HarmonyRoot).Path
if (!(Test-Path (Join-Path $DesktopRoot 'src-tauri/Cargo.toml')) -or
    !(Test-Path (Join-Path $HarmonyRoot 'EggDone/build-profile.json5'))) { throw 'Expected both EggDone checkouts.' }
$logs = Join-Path ([IO.Path]::GetTempPath()) ('eggdone-checklist-regression-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $logs | Out-Null
$results = [Collections.Generic.List[object]]::new()
$source = @{}
foreach ($root in @($DesktopRoot, $HarmonyRoot)) {
  $source[$root] = @{ commit = (& git -C $root rev-parse HEAD); status = @(& git -C $root status --short) }
}
function Run-Step([string]$Name, [string]$Cwd, [string]$Tool, [string[]]$Arguments) {
  $log = Join-Path $logs ($Name + '.log')
  Write-Host "RUN $Name"
  $timer = [Diagnostics.Stopwatch]::StartNew()
  Push-Location -LiteralPath $Cwd
  $code = -1
  try {
    Get-Command $Tool -ErrorAction Stop | Out-Null
    # Native compiler warnings on stderr are not PowerShell terminating errors.
    $ErrorActionPreference = 'Continue'
    & $Tool @Arguments > $log 2>&1
    $code = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = 'Stop'
    Pop-Location
    $timer.Stop()
    $results.Add(@{ name = $Name; exitCode = $code; seconds = $timer.Elapsed.TotalSeconds; log = $log })
  }
  if ($code -ne 0) { Get-Content -LiteralPath $log -Tail 45; throw "Failed: $Name ($code)" }
  Write-Host "PASS $Name"
}
$passed = $false
try {
  Run-Step 'shared-composition' $HarmonyRoot 'node' @('scripts/test-task-composition.cjs', "--desktop=$DesktopRoot")
  Run-Step 'shared-contract' $HarmonyRoot 'node' @('scripts/test-task-checklist-contract.cjs', "--peer=$DesktopRoot")
  foreach ($name in @('storage','editor','inheritance','sync','views','editor-gateway','editor-session','panel','details')) {
    Run-Step "harmony-$name" $HarmonyRoot 'node' @("scripts/test-task-checklist-$name.cjs", "--desktop=$DesktopRoot")
  }
  foreach ($name in @('test-checklist-scope','test-checklist-task-draft','test-checklist-task-fields','test-task-undo',
      'test-todo-completion-recurrence','test-recurrence-transaction','test-task-note-link-backup','test-recurrence-backup')) {
    Run-Step $name $HarmonyRoot 'node' @("scripts/$name.cjs", "--desktop=$DesktopRoot")
  }
  Run-Step 'desktop-rust' (Join-Path $DesktopRoot 'src-tauri') 'cargo' @('test','--locked','--lib','--quiet')
  Run-Step 'cross-client-backup' $HarmonyRoot 'node' @('scripts/test-task-checklist-backup.cjs', "--desktop=$DesktopRoot")
  Run-Step 'cross-client-http' $HarmonyRoot 'node' @('scripts/test-task-checklist-http.cjs', "--desktop=$DesktopRoot")
  Run-Step 'desktop-unit' $DesktopRoot 'pnpm' @('exec','vitest','run','--maxWorkers=1')
  Run-Step 'desktop-types' $DesktopRoot 'pnpm' @('check')
  Run-Step 'desktop-i18n' $DesktopRoot 'pnpm' @('i18n:check')
  Run-Step 'desktop-rust-format' (Join-Path $DesktopRoot 'src-tauri') 'cargo' @('fmt','--','--check')
  Run-Step 'desktop-rust-check' (Join-Path $DesktopRoot 'src-tauri') 'cargo' @('check','--locked')
  if (!$SkipUi) { Run-Step 'desktop-browser' $DesktopRoot 'node' @('scripts/test-task-checklist-ui.mjs') }
  if (!$SkipBuild) {
    Run-Step 'desktop-test-build' $DesktopRoot 'pnpm' @('tauri','build','--debug','--no-bundle')
    Run-Step 'harmony-test-build' (Join-Path $HarmonyRoot 'EggDone') 'devecocli' @('build','--product','default','--modules','entry@default','--build-mode','debug')
  }
  $passed = $true
} finally {
  @{ passed = $passed; source = $source; steps = @($results.ToArray()); skipUi = [bool]$SkipUi;
    skipBuild = [bool]$SkipBuild; nativeAcceptance = 'Separate user/device acceptance; not certified by this runner.'
  } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $logs 'results.json') -Encoding utf8
  Write-Host "Evidence: $logs"
}
