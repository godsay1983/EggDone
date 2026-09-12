param(
    [ValidateRange(1024, 65535)][int]$Port = 18477,
    [switch]$CrossClientSessions,
    [switch]$TrashRecoverySessions,
    [switch]$NoteHistorySessions,
    [string]$HarmonyRoot
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
foreach ($command in @('docker', 'cargo')) { $null = Get-Command $command -ErrorAction Stop }
if (@($CrossClientSessions, $TrashRecoverySessions, $NoteHistorySessions).Where({ $_.IsPresent }).Count -gt 1) { throw 'Select only one cross-client suite' }
if ($CrossClientSessions -or $TrashRecoverySessions -or $NoteHistorySessions) {
    $null = Get-Command node -ErrorAction Stop
    if (-not $HarmonyRoot) { throw 'Cross-client suites require the explicit Harmony checkout path' }
    $HarmonyRoot = (Resolve-Path -LiteralPath $HarmonyRoot).Path
    $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-full-sync-s3.cjs'
    if ($TrashRecoverySessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-trash-sync-s3.cjs' }
    if ($NoteHistorySessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-note-history-sync-s3.cjs' }
    if (-not (Test-Path -LiteralPath $harmonyTest)) { throw 'Harmony checkout is missing the full session test' }
}
if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) { throw 'Port occupied; select another port' }
$run = [guid]::NewGuid().ToString('N')
$name = "eggdone-sync-core-$run"
$logs = Join-Path $env:TEMP $name
$null = New-Item -ItemType Directory -Path $logs
$config = Join-Path $logs 'public-s3-fixture.json'
@{ identities = @(@{ name = 'isolated-sync-core'; credentials = @(@{
    accessKey = 'eggdone-ns7-test-access'; secretKey = 'eggdone-ns7-public-test-fixture'
}); actions = @('Admin', 'Read', 'Write', 'List') }) } |
    ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $config -Encoding utf8
$oldRun = $env:EGGDONE_NS7_S3_RUN
$oldPort = $env:EGGDONE_NS7_S3_PORT
$created = $false
$cleanupFailed = $false

function Invoke-DesktopTest([string]$Test, [string]$Marker) {
    $log = Join-Path $logs ($Test.Replace('::', '-') + '.log')
    Push-Location (Join-Path $root 'src-tauri')
    try {
        & cargo test --lib "commands::sync_core_tests::s3_tests::$Test" -- --ignored --exact --nocapture 2>&1 |
            Out-File -LiteralPath $log -Encoding utf8
        if ($LASTEXITCODE -ne 0) { throw "Desktop sync core failed; see $log" }
        $report = Get-Content -LiteralPath $log -Raw
        if ($report -notmatch 'test result: ok\. 1 passed; 0 failed; 0 ignored;' -or $report -notmatch $Marker) {
            throw "Expected test did not execute; see $log"
        }
    } finally { Pop-Location }
}

function Invoke-HarmonyPhase([string]$Phase) {
    $log = Join-Path $logs "harmony-$Phase.log"
    & node $harmonyTest $Phase 2>&1 | Out-File -LiteralPath $log -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw "Harmony host session failed; see $log" }
    $marker = 'FULL_SESSION_HARMONY_' + $Phase.ToUpperInvariant() + '_OK:'
    if ($TrashRecoverySessions) { $marker = 'TRASH_SESSION_HARMONY_' + $Phase.ToUpperInvariant() + '_OK:' }
    if ($NoteHistorySessions) { $marker = 'HISTORY_SESSION_HARMONY_' + $Phase.ToUpperInvariant() + '_OK:' }
    if ((Get-Content -LiteralPath $log -Raw) -notmatch $marker) { throw "Harmony phase did not complete; see $log" }
}

try {
    $env:EGGDONE_NS7_S3_RUN = $run
    $env:EGGDONE_NS7_S3_PORT = $Port.ToString()
    $image = 'chrislusf/seaweedfs:4.34'
    & docker image inspect $image --format '{{.Id}}' | Out-File (Join-Path $logs 'image.txt') -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw 'Pinned test image must already be installed; this script never pulls images' }
    # Only loopback is exposed. Test objects live in tmpfs, never in existing Docker volumes.
    $cid = (& docker run -d --name $name --label "eggdone.sync-core.run=$run" --pull never `
        -p "127.0.0.1:${Port}:8333" --tmpfs /data:rw,size=536870912 `
        --mount "type=bind,source=$config,target=/etc/s3-fixture.json,readonly" `
        $image server '-dir=/data' '-ip=127.0.0.1' '-ip.bind=0.0.0.0' '-master.volumeSizeLimitMB=1024' `
        '-volume.max=1' '-volume.preStopSeconds=0' '-s3' '-s3.config=/etc/s3-fixture.json' | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $cid -notmatch '^[0-9a-f]{64}$') { throw 'Disposable S3 failed to start' }
    $created = $true
    $ready = $false
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    while ([DateTime]::UtcNow -lt $deadline) {
        $state = (& docker inspect $cid --format '{{.State.Running}}' | Out-String).Trim()
        if ($state -ne 'true') { throw 'Disposable S3 exited' }
        try {
            $r = Invoke-WebRequest -Uri "http://127.0.0.1:$Port/" -TimeoutSec 2 -SkipHttpErrorCheck
            if ([int]$r.StatusCode -eq 403) { $ready = $true; break }
        } catch { }
        Start-Sleep -Milliseconds 500
    }
    if (-not $ready) { throw 'Authenticated S3 readiness timed out' }
    Write-Host "Running the desktop production sync core against isolated S3. Evidence: $logs"
    if ($NoteHistorySessions) {
        Invoke-DesktopTest 'history::history_session_prepare' 'HISTORY_SESSION_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'exchange'
        Invoke-DesktopTest 'history::history_session_verify' 'HISTORY_SESSION_DESKTOP_VERIFY_OK'
        Invoke-HarmonyPhase 'verify'
    } elseif ($TrashRecoverySessions) {
        Invoke-DesktopTest 'trash_session_prepare' 'TRASH_SESSION_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'exchange'
        Invoke-DesktopTest 'trash_session_verify' 'TRASH_SESSION_DESKTOP_VERIFY_OK'
        Invoke-HarmonyPhase 'verify'
    } elseif ($CrossClientSessions) {
        Invoke-DesktopTest 'full_session_prepare' 'FULL_SESSION_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'exchange'
        Invoke-DesktopTest 'full_session_verify' 'FULL_SESSION_DESKTOP_VERIFY_OK'
        Invoke-HarmonyPhase 'verify'
    } else {
        Invoke-DesktopTest 'isolated_s3_offline_peers_unlink_binary_and_credentials_recovery' 'SYNC_CORE_S3_DESKTOP_OK:'
    }
} finally {
    if ($created) {
        & docker logs $cid 2>&1 | Out-File (Join-Path $logs 'server.log') -Encoding utf8
        $label = (& docker inspect $cid --format '{{index .Config.Labels "eggdone.sync-core.run"}}' | Out-String).Trim()
        if ($label -eq $run) {
            & docker rm -f $cid | Out-Null
            if ($LASTEXITCODE -ne 0) { $cleanupFailed = $true }
        } else {
            $cleanupFailed = $true
            Write-Warning 'Container ownership not confirmed; nothing removed'
        }
    }
    $env:EGGDONE_NS7_S3_RUN = $oldRun
    $env:EGGDONE_NS7_S3_PORT = $oldPort
}
if ($cleanupFailed) { throw 'Disposable resource cleanup incomplete' }
Write-Host "Sync core S3 passed (cross-client: $CrossClientSessions; trash recovery: $TrashRecoverySessions; note history: $NoteHistorySessions); disposable service removed. Evidence: $logs"
