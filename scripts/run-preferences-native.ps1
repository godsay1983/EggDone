param(
  [string]$Executable = (Join-Path $PSScriptRoot '../src-tauri/target/search-native/debug/eggdone.exe')
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$exe = (Resolve-Path -LiteralPath $Executable).Path
$allowed = [IO.Path]::GetFullPath((Join-Path $root 'src-tauri/target/search-native')) + [IO.Path]::DirectorySeparatorChar
if (-not $exe.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) { throw 'Use the isolated search-native build only' }
if ($exe -ne (Join-Path $allowed 'debug/eggdone.exe')) { throw 'Use the isolated Debug executable documented for this test' }
if (Get-Process -Name eggdone -ErrorAction SilentlyContinue | Where-Object Path -eq $exe) {
  throw 'The isolated executable is already running; inspect that process instead of starting another'
}
if (Get-NetTCPConnection -LocalPort 9228 -State Listen -ErrorAction SilentlyContinue) { throw 'CDP port is occupied; do not attach to an unknown process' }
$output = Join-Path $env:TEMP ('eggdone-preferences-native-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $output
$state = Join-Path $output 'recovery.json'
$script = Join-Path $PSScriptRoot 'test-preferences-native.mjs'
$originalArgs = $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
$script:owned = $null
$result = @{ schemaVersion = 1; passed = $false; restartMode = 'forced-process'; executable = $exe;
  sha256 = (Get-FileHash -LiteralPath $exe).Hash; processes = @(); error = $null;
  restoration = 'not-needed'; restorationError = $null; cleanupCompleted = $false }
Write-Host "Evidence: $output"
function Start-Isolated([string]$Phase) {
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9228'
  $script:owned = Start-Process -FilePath $exe -WorkingDirectory $root -WindowStyle Hidden -PassThru `
    -RedirectStandardOutput (Join-Path $output "$Phase-stdout.log") -RedirectStandardError (Join-Path $output "$Phase-stderr.log")
  $result.processes += @{ phase = $Phase; pid = $script:owned.Id; started = $script:owned.StartTime.ToUniversalTime().ToString('o') }
  $diagnostic = @{ phase = $Phase; ready = $false; attempts = 0; targetUrls = @(); lastError = $null }
  for ($attempt = 0; $attempt -lt 60; $attempt++) {
    $diagnostic.attempts = $attempt + 1
    if ($script:owned.HasExited) {
      $diagnostic.lastError = "Isolated app exited during $Phase startup (exit $($script:owned.ExitCode))"
      $diagnostic | ConvertTo-Json -Depth 5 | Out-File -LiteralPath (Join-Path $output "$Phase-startup.json") -Encoding utf8
      throw $diagnostic.lastError
    }
    try {
      $targets = Invoke-RestMethod 'http://127.0.0.1:9228/json/list' -TimeoutSec 1
      $diagnostic.targetUrls = @($targets | ForEach-Object url)
      $diagnostic.lastError = $null
      $diagnostic.ready = @($targets | Where-Object { $_.type -eq 'page' -and $_.url -eq 'http://tauri.localhost/' }).Count -gt 0
    } catch { $diagnostic.lastError = $_.Exception.Message }
    $diagnostic | ConvertTo-Json -Depth 5 | Out-File -LiteralPath (Join-Path $output "$Phase-startup.json") -Encoding utf8
    if ($diagnostic.ready) { return }
    Start-Sleep -Milliseconds 500
  }
  throw 'Isolated CDP startup timed out; inspect the same process before retrying'
}
function Stop-Isolated {
  if ($script:owned -and -not $script:owned.HasExited) {
    $actual = Get-Process -Id $script:owned.Id -ErrorAction Stop
    if ($actual.Path -ne $exe -or $actual.StartTime -ne $script:owned.StartTime) { throw 'Unexpected process identity; refusing to stop it' }
    Stop-Process -Id $actual.Id -ErrorAction Stop
    if (-not $script:owned.WaitForExit(10000)) { throw 'Isolated process did not exit' }
  }
  $script:owned = $null
}
try {
  Start-Isolated 'write'
  & node $script write $state
  if ($LASTEXITCODE -ne 0) { throw 'Preference write phase failed' }
  Stop-Isolated
  Start-Isolated 'read'
  & node $script read $state
  if ($LASTEXITCODE -ne 0) { throw 'Preference restart verification failed' }
} catch {
  $result.error = $_.Exception.Message
} finally {
  try {
    if (Test-Path -LiteralPath $state) {
      $result.restoration = 'required'
      if (-not $script:owned -or $script:owned.HasExited) { Start-Isolated 'restore' }
      & node $script restore $state
      if ($LASTEXITCODE -ne 0) { throw "Preference restoration failed; recovery state remains at $state" }
      $result.restoration = 'passed'
    }
  } catch {
    $result.restoration = 'failed'
    $result.restorationError = $_.Exception.Message
  } finally {
    try {
      Stop-Isolated
      $result.cleanupCompleted = $true
    } catch { $result.error = "$($result.error) Cleanup: $($_.Exception.Message)" }
    finally {
      $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = $originalArgs
      try {
        if ((Get-FileHash -LiteralPath $exe).Hash -ne $result.sha256) { throw 'The executable changed during testing' }
      } catch { $result.error = "$($result.error) Binary: $($_.Exception.Message)" }
      $result.passed = $null -eq $result.error -and $result.restoration -eq 'passed' -and $result.cleanupCompleted
      $result | ConvertTo-Json -Depth 5 | Out-File -LiteralPath (Join-Path $output 'result.json') -Encoding utf8
    }
  }
}
& node (Join-Path $PSScriptRoot 'check-preferences-native-report.mjs') $output
if ($LASTEXITCODE -ne 0) { throw "Native preference run did not pass; inspect $output/result.json" }
Write-Host 'Native preference process-restart checks passed; isolated preferences restored.'
