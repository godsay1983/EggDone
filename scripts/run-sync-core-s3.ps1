param(
    [ValidateRange(1024, 65535)][int]$Port = 18477,
    [switch]$CrossClientSessions,
    [switch]$TrashRecoverySessions,
    [switch]$NoteHistorySessions,
    [switch]$ArchiveRecoverySessions,
    [switch]$PurgeCompatibilitySessions,
    [switch]$MigrationJournalSessions,
    [switch]$AssetSafetySessions,
    [switch]$CloudSnapshotSessions,
    [switch]$PublicationSessions,
    [switch]$SpaceProtocolSessions,
    [switch]$PurgeRemoteSessions,
    [switch]$ActivationSessions,
    [switch]$DirectPurgeSessions,
    [switch]$AutoJoinSessions,
    [string]$HarmonyRoot
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
foreach ($command in @('docker', 'cargo')) { $null = Get-Command $command -ErrorAction Stop }
if (@($CrossClientSessions, $TrashRecoverySessions, $NoteHistorySessions, $ArchiveRecoverySessions, $PurgeCompatibilitySessions, $MigrationJournalSessions, $AssetSafetySessions, $CloudSnapshotSessions, $PublicationSessions, $SpaceProtocolSessions, $PurgeRemoteSessions, $ActivationSessions, $DirectPurgeSessions, $AutoJoinSessions).Where({ $_.IsPresent }).Count -gt 1) { throw 'Select only one cross-client suite' }
if ($CrossClientSessions -or $TrashRecoverySessions -or $NoteHistorySessions -or $ArchiveRecoverySessions -or $PurgeCompatibilitySessions -or $MigrationJournalSessions -or $AssetSafetySessions -or $CloudSnapshotSessions -or $PublicationSessions -or $SpaceProtocolSessions -or $PurgeRemoteSessions -or $ActivationSessions -or $DirectPurgeSessions -or $AutoJoinSessions) {
    $null = Get-Command node -ErrorAction Stop
    if (-not $HarmonyRoot) { throw 'Cross-client suites require the explicit Harmony checkout path' }
    $HarmonyRoot = (Resolve-Path -LiteralPath $HarmonyRoot).Path
    $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-full-sync-s3.cjs'
    if ($TrashRecoverySessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-trash-sync-s3.cjs' }
    if ($NoteHistorySessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-note-history-sync-s3.cjs' }
    if ($ArchiveRecoverySessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-archive-sync-s3.cjs' }
    if ($PurgeCompatibilitySessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-purge-compat-s3.cjs' }
    if ($MigrationJournalSessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-migration-journal-s3.cjs' }
    if ($AssetSafetySessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-asset-safety-s3.cjs' }
    if ($CloudSnapshotSessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-cloud-snapshot-s3.cjs' }
    if ($PublicationSessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-publication-s3.cjs' }
    if ($SpaceProtocolSessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-space-protocol-s3.cjs' }
    if ($ActivationSessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-space-activation-s3.cjs' }
    if ($DirectPurgeSessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-direct-purge-s3.cjs' }
    if ($AutoJoinSessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-auto-join-s3.cjs' }
    if ($PurgeRemoteSessions) { $harmonyTest = Join-Path $HarmonyRoot 'scripts/test-purge-remote-s3.cjs' }
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
    if ($ArchiveRecoverySessions) { $marker = 'ARCHIVE_SESSION_HARMONY_' + $Phase.ToUpperInvariant() + '_OK:' }
    if ($PurgeCompatibilitySessions) { $marker = 'PURGE_COMPAT_HARMONY_' + $Phase.ToUpperInvariant() + '_OK:' }
    if ($MigrationJournalSessions) { $marker = 'MIGRATION_HARMONY_' + $Phase.ToUpperInvariant() + '_OK:' }
    if ($AssetSafetySessions) { $marker = 'ASSET_SAFETY_HARMONY_OK:' }
    if ($CloudSnapshotSessions) { $marker = 'CLOUD_SNAPSHOT_HARMONY_OK:' }
    if ($PublicationSessions) { $marker = 'PUBLICATION_HARMONY_OK:' }
    if ($SpaceProtocolSessions) { $marker = 'SPACE_PROTOCOL_HARMONY_OK:' }
    if ($ActivationSessions) { $marker = 'ACTIVATION_HARMONY_OK:' }
    if ($DirectPurgeSessions) { $marker = 'DIRECT_PURGE_HARMONY_OK:' }
    if ($AutoJoinSessions) { $marker = 'AUTO_JOIN_HARMONY_OK:' }
    if ($PurgeRemoteSessions) { $marker = 'PURGE_REMOTE_HARMONY_OK:' }
    if ((Get-Content -LiteralPath $log -Raw) -notmatch $marker) { throw "Harmony phase did not complete; see $log" }
}

try {
    $env:EGGDONE_NS7_S3_RUN = $run
    $env:EGGDONE_NS7_S3_PORT = $Port.ToString()
    $image = 'chrislusf/seaweedfs:4.34'
    & docker image inspect $image --format '{{.Id}}' | Out-File (Join-Path $logs 'image.txt') -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw 'Pinned test image must already be installed; this script never pulls images' }
    # Only loopback is exposed. Test objects live in tmpfs, never in existing Docker volumes.
    # SeaweedFS allocates up to seven sparse volumes per bucket on first write.
    $volumeLimit = if ($ActivationSessions) { '-volume.max=14' } else { '-volume.max=1' }
    $cid = (& docker run -d --name $name --label "eggdone.sync-core.run=$run" --pull never `
        -p "127.0.0.1:${Port}:8333" --tmpfs /data:rw,size=536870912 `
        --mount "type=bind,source=$config,target=/etc/s3-fixture.json,readonly" `
        $image server '-dir=/data' '-ip=127.0.0.1' '-ip.bind=0.0.0.0' '-master.volumeSizeLimitMB=1024' `
        $volumeLimit '-volume.preStopSeconds=0' '-s3' '-s3.config=/etc/s3-fixture.json' | Out-String).Trim()
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
    if ($AutoJoinSessions) {
        Invoke-DesktopTest 'migration::migration_prepare' 'MIGRATION_DESKTOP_PREPARE_OK'
        Invoke-DesktopTest 'activation::missing_seed' 'ACTIVATION_MISSING_SEED_OK'
        Invoke-DesktopTest 'activation::prepare' 'ACTIVATION_DESKTOP_PREPARE_OK'
        Invoke-DesktopTest 'auto_join::prepare' 'AUTO_JOIN_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'exchange'
        Invoke-DesktopTest 'auto_join::verify' 'AUTO_JOIN_DESKTOP_VERIFY_OK'
        Invoke-HarmonyPhase 'verify'
    } elseif ($DirectPurgeSessions) {
        Invoke-DesktopTest 'migration::migration_prepare' 'MIGRATION_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'purge'
        Invoke-DesktopTest 'activation::direct_verify_and_purge' 'DIRECT_PURGE_DESKTOP_OK'
        Invoke-HarmonyPhase 'verify'
    } elseif ($ActivationSessions) {
        Invoke-DesktopTest 'migration::migration_prepare' 'MIGRATION_DESKTOP_PREPARE_OK'
        Invoke-DesktopTest 'activation::missing_seed' 'ACTIVATION_MISSING_SEED_OK'
        Invoke-DesktopTest 'activation::prepare' 'ACTIVATION_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'verify'
        Invoke-DesktopTest 'activation::verify' 'ACTIVATION_DESKTOP_VERIFY_OK'
        # A second disposable bucket exercises the opposite creator/purger direction.
        $env:EGGDONE_NS7_S3_RUN = [guid]::NewGuid().ToString('N')
        Invoke-DesktopTest 'migration::migration_prepare' 'MIGRATION_DESKTOP_PREPARE_OK'
        Invoke-DesktopTest 'activation::missing_seed' 'ACTIVATION_MISSING_SEED_OK'
        Invoke-HarmonyPhase 'create'
        Invoke-DesktopTest 'activation::reverse' 'ACTIVATION_DESKTOP_REVERSE_OK'
        Invoke-HarmonyPhase 'reverse'
    } elseif ($PurgeRemoteSessions) {
        Invoke-DesktopTest 'migration::migration_prepare' 'MIGRATION_DESKTOP_PREPARE_OK'
        Invoke-DesktopTest 'migration::space_protocol_prepare' 'SPACE_PROTOCOL_DESKTOP_PREPARE_OK'
        Invoke-DesktopTest 'purge_remote::prepare' 'PURGE_REMOTE_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'verify'
        Invoke-DesktopTest 'purge_remote::verify' 'PURGE_REMOTE_DESKTOP_VERIFY_OK'
    } elseif ($SpaceProtocolSessions) {
        Invoke-DesktopTest 'migration::migration_prepare' 'MIGRATION_DESKTOP_PREPARE_OK'
        Invoke-DesktopTest 'migration::space_protocol_prepare' 'SPACE_PROTOCOL_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'verify'
        Invoke-DesktopTest 'migration::space_protocol_verify' 'SPACE_PROTOCOL_DESKTOP_VERIFY_OK'
    } elseif ($PublicationSessions) {
        Invoke-DesktopTest 'migration::migration_prepare' 'MIGRATION_DESKTOP_PREPARE_OK'
        Invoke-DesktopTest 'migration::publication_verify' 'PUBLICATION_DESKTOP_OK'
        Invoke-HarmonyPhase 'verify'
    } elseif ($CloudSnapshotSessions) {
        Invoke-DesktopTest 'migration::migration_prepare' 'MIGRATION_DESKTOP_PREPARE_OK'
        Invoke-DesktopTest 'migration::cloud_snapshot_verify' 'CLOUD_SNAPSHOT_DESKTOP_OK'
        Invoke-HarmonyPhase 'verify'
    } elseif ($AssetSafetySessions) {
        Invoke-DesktopTest 'asset_safety_prepare' 'ASSET_SAFETY_DESKTOP_OK'
        Invoke-HarmonyPhase 'verify'
    } elseif ($MigrationJournalSessions) {
        Invoke-DesktopTest 'migration::migration_prepare' 'MIGRATION_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'exchange'
        Invoke-HarmonyPhase 'publish'
        Invoke-HarmonyPhase 'resume'
        Invoke-DesktopTest 'migration::migration_verify' 'MIGRATION_DESKTOP_VERIFY_OK'
        Invoke-HarmonyPhase 'verify'
    } elseif ($PurgeCompatibilitySessions) {
        Invoke-DesktopTest 'purge_compat::legacy_prepare' 'PURGE_COMPAT_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'exchange'
        Invoke-DesktopTest 'purge_compat::legacy_late_write' 'PURGE_COMPAT_DESKTOP_LATE_OK'
        Invoke-HarmonyPhase 'verify'
    } elseif ($ArchiveRecoverySessions) {
        Invoke-DesktopTest 'archive::archive_session_prepare' 'ARCHIVE_SESSION_DESKTOP_PREPARE_OK'
        Invoke-HarmonyPhase 'exchange'
        Invoke-DesktopTest 'archive::archive_session_verify' 'ARCHIVE_SESSION_DESKTOP_VERIFY_OK'
        Invoke-HarmonyPhase 'verify'
    } elseif ($NoteHistorySessions) {
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
Write-Host "Sync core S3 passed (cross-client: $CrossClientSessions; trash recovery: $TrashRecoverySessions; note history: $NoteHistorySessions; archive recovery: $ArchiveRecoverySessions; purge compatibility: $PurgeCompatibilitySessions; migration journal: $MigrationJournalSessions; asset safety: $AssetSafetySessions; cloud snapshot: $CloudSnapshotSessions; publication: $PublicationSessions; space protocol: $SpaceProtocolSessions; purge remote: $PurgeRemoteSessions; activation: $ActivationSessions; direct purge: $DirectPurgeSessions; auto join: $AutoJoinSessions); disposable service removed. Evidence: $logs"
