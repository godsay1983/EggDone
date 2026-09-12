param([ValidateRange(1024, 65535)][int]$Port = 18477)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
foreach ($command in @('docker', 'cargo')) { $null = Get-Command $command -ErrorAction Stop }
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
    $log = Join-Path $logs 'desktop-sync-core.log'
    Write-Host "Running the desktop production sync core against isolated S3. Evidence: $logs"
    Push-Location (Join-Path $root 'src-tauri')
    try {
        & cargo test --lib commands::sync_core_tests::s3_tests::isolated_s3_offline_peers_unlink_binary_and_credentials_recovery -- --ignored --exact --nocapture 2>&1 |
            Out-File -LiteralPath $log -Encoding utf8
        if ($LASTEXITCODE -ne 0) { throw "Desktop sync core failed; see $log" }
        $report = Get-Content -LiteralPath $log -Raw
        if ($report -notmatch 'test result: ok\. 1 passed; 0 failed; 0 ignored;' -or $report -notmatch 'SYNC_CORE_S3_DESKTOP_OK:') {
            throw "Expected test did not execute; see $log"
        }
    } finally { Pop-Location }
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
Write-Host "Desktop sync core S3 passed; disposable service removed. Evidence: $logs"
