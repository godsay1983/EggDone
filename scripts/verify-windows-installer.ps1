param(
    [Parameter(Mandatory = $true)]
    [string]$BaseInstaller,

    [Parameter(Mandatory = $true)]
    [string]$UpgradeInstaller,

    [string]$ExpectedBaseVersion = "0.1.0",
    [string]$ExpectedUpgradeVersion = "0.1.1"
)

$ErrorActionPreference = "Stop"
$uninstallRoot = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall"

function Get-EggDoneInstallation {
    Get-ChildItem $uninstallRoot -ErrorAction SilentlyContinue |
        ForEach-Object { Get-ItemProperty $_.PSPath } |
        Where-Object { $_.DisplayName -eq "EggDone" } |
        Select-Object -First 1
}

function Invoke-Installer {
    param([string]$Path)

    $process = Start-Process -FilePath $Path -ArgumentList "/S" -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "安装器返回退出码 $($process.ExitCode)：$Path"
    }
}

function Assert-InstalledVersion {
    param([string]$ExpectedVersion)

    $installation = Get-EggDoneInstallation
    if (-not $installation) {
        throw "未找到 EggDone 卸载注册信息"
    }
    if ($installation.DisplayVersion -ne $ExpectedVersion) {
        throw "安装版本不匹配，期望 $ExpectedVersion，实际 $($installation.DisplayVersion)"
    }

    $installLocation = $installation.InstallLocation.Trim().Trim('"')
    $executable = Join-Path $installLocation "eggdone.exe"
    if (-not (Test-Path -LiteralPath $executable)) {
        throw "安装目录缺少 eggdone.exe：$executable"
    }
    if ((Get-Item $executable).VersionInfo.ProductVersion -ne $ExpectedVersion) {
        throw "eggdone.exe 产品版本不匹配"
    }

    return [PSCustomObject]@{
        Registry = $installation
        InstallLocation = $installLocation
    }
}

if (Get-Process eggdone -ErrorAction SilentlyContinue) {
    throw "EggDone 正在运行，请先退出后再验证安装器"
}
if (Get-EggDoneInstallation) {
    throw "当前用户已安装 EggDone，为避免覆盖真实环境，验证已停止"
}
if (-not (Test-Path -LiteralPath $BaseInstaller)) {
    throw "基础安装包不存在：$BaseInstaller"
}
if (-not (Test-Path -LiteralPath $UpgradeInstaller)) {
    throw "升级安装包不存在：$UpgradeInstaller"
}

$installed = $false
try {
    Invoke-Installer $BaseInstaller
    $installed = $true
    $base = Assert-InstalledVersion $ExpectedBaseVersion
    Write-Host "基础版本安装通过：$($base.InstallLocation)"

    Invoke-Installer $UpgradeInstaller
    $upgrade = Assert-InstalledVersion $ExpectedUpgradeVersion
    Write-Host "覆盖升级通过：$($upgrade.Registry.DisplayVersion)"

    $downgrade = Start-Process -FilePath $BaseInstaller -ArgumentList "/S" -Wait -PassThru
    $afterDowngrade = Assert-InstalledVersion $ExpectedUpgradeVersion
    if ($downgrade.ExitCode -eq 0) {
        throw "降级安装器应返回非零退出码"
    }
    Write-Host "降级阻止通过：安装器退出码 $($downgrade.ExitCode)，当前版本 $($afterDowngrade.Registry.DisplayVersion)"

    $uninstallCommand = $upgrade.Registry.QuietUninstallString
    if (-not $uninstallCommand) {
        $uninstallCommand = $upgrade.Registry.UninstallString
    }
    if (-not $uninstallCommand) {
        throw "卸载注册信息缺少卸载命令"
    }
    $uninstaller = $uninstallCommand.Trim().Trim('"') -replace '\s+/S$', ''
    if (-not (Test-Path -LiteralPath $uninstaller)) {
        throw "卸载程序不存在：$uninstaller"
    }

    $process = Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "卸载程序返回退出码 $($process.ExitCode)"
    }
    $installed = $false

    if (Get-EggDoneInstallation) {
        throw "卸载后仍存在 EggDone 卸载注册信息"
    }
    $upgradeLocation = $upgrade.InstallLocation
    if (Test-Path -LiteralPath $upgradeLocation) {
        throw "卸载后安装目录仍存在：$upgradeLocation"
    }

    Write-Host "卸载通过"
} finally {
    if ($installed) {
        $installation = Get-EggDoneInstallation
        if ($installation) {
            $uninstallCommand = $installation.QuietUninstallString
            if (-not $uninstallCommand) {
                $uninstallCommand = $installation.UninstallString
            }
            $uninstaller = $uninstallCommand.Trim().Trim('"') -replace '\s+/S$', ''
            if (Test-Path -LiteralPath $uninstaller) {
                Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait | Out-Null
            }
        }
    }
}
