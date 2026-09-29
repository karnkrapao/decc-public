param(
    [string]$InstallerPath = ""
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$TauriConfig = Get-Content -LiteralPath (Join-Path $Root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$ExpectedName = [string]$TauriConfig.productName
$ExpectedVersion = [string]$TauriConfig.version
$BundleDir = Join-Path $Root "src-tauri\target\release\bundle\nsis"
$GeneratedNsisDir = Join-Path $Root "src-tauri\target\release\nsis"

if ([string]::IsNullOrWhiteSpace($InstallerPath)) {
    $Candidates = @(
        Get-ChildItem -LiteralPath $BundleDir -File -Filter "$($ExpectedName)_$($ExpectedVersion)_*-setup.exe" -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending
    )
    if ($Candidates.Count -eq 0) {
        throw "No NSIS installer exists for $ExpectedName $ExpectedVersion. Run the Windows release preflight first."
    }
    $InstallerPath = $Candidates[0].FullName
}

$GeneratedInstallerScript = Get-ChildItem -LiteralPath $GeneratedNsisDir -Recurse -File -Filter "installer.nsi" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $GeneratedInstallerScript) {
    throw "Generated NSIS installer script was not found under $GeneratedNsisDir. Run the Windows release preflight first."
}

$NsisText = Get-Content -LiteralPath $GeneratedInstallerScript.FullName -Raw
function Get-NsisDefine([string]$Name) {
    $Pattern = '(?m)^!define\s+' + [regex]::Escape($Name) + '\s+"([^"]*)"'
    $Match = [regex]::Match($NsisText, $Pattern)
    if (-not $Match.Success) {
        throw "Generated NSIS installer is missing define '$Name'."
    }
    return $Match.Groups[1].Value
}

$Manufacturer = Get-NsisDefine "MANUFACTURER"
$InstallMode = Get-NsisDefine "INSTALLMODE"
if ($InstallMode -ne "currentUser") {
    throw "Windows installer smoke currently supports Tauri NSIS currentUser mode only; generated mode is '$InstallMode'."
}

$ProductRegistryPath = "HKCU:\Software\$Manufacturer\$ExpectedName"
$UninstallRegistryPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$ExpectedName"
$SmokePathPattern = "$($env:TEMP)\decc-installer-smoke-*\install"

function Normalize-InstallLocation($Value) {
    if ($null -eq $Value) {
        return ""
    }
    return ([string]$Value).Trim().Trim('"')
}

function Get-ProductInstallLocation {
    if (-not (Test-Path -LiteralPath $ProductRegistryPath)) {
        return ""
    }
    return Normalize-InstallLocation ((Get-Item -LiteralPath $ProductRegistryPath).GetValue(""))
}

function Get-UninstallInstallLocation {
    if (-not (Test-Path -LiteralPath $UninstallRegistryPath)) {
        return ""
    }
    $Value = (Get-ItemProperty -LiteralPath $UninstallRegistryPath -Name InstallLocation -ErrorAction SilentlyContinue).InstallLocation
    return Normalize-InstallLocation $Value
}

function Test-IsSmokeInstallLocation([string]$Value) {
    return -not [string]::IsNullOrWhiteSpace($Value) -and $Value -like $SmokePathPattern
}

function Remove-StaleSmokeRegistryState {
    $ProductLocation = Get-ProductInstallLocation
    if (Test-IsSmokeInstallLocation $ProductLocation) {
        Write-Host "Removing stale smoke product registry state: $ProductRegistryPath -> $ProductLocation"
        Remove-Item -LiteralPath $ProductRegistryPath -Recurse -Force
    }

    if (Test-Path -LiteralPath $UninstallRegistryPath) {
        $UninstallLocation = Get-UninstallInstallLocation
        if (Test-IsSmokeInstallLocation $UninstallLocation) {
            Write-Host "Removing stale smoke uninstall registry state: $UninstallRegistryPath -> $UninstallLocation"
            Remove-Item -LiteralPath $UninstallRegistryPath -Recurse -Force
        }
    }
}

$ExistingProductLocation = Get-ProductInstallLocation
if (-not [string]::IsNullOrWhiteSpace($ExistingProductLocation) -and -not (Test-IsSmokeInstallLocation $ExistingProductLocation)) {
    throw "Refusing installer smoke because DECC is already registered at '$ExistingProductLocation'. Use a clean Windows user/machine for installer lifecycle testing."
}

if (Test-Path -LiteralPath $UninstallRegistryPath) {
    $ExistingUninstallLocation = Get-UninstallInstallLocation
    if ([string]::IsNullOrWhiteSpace($ExistingUninstallLocation) -or -not (Test-IsSmokeInstallLocation $ExistingUninstallLocation)) {
        throw "Refusing installer smoke because a normal DECC uninstall registration already exists. Use a clean Windows user/machine for installer lifecycle testing."
    }
}

Remove-StaleSmokeRegistryState

$SmokeRoot = Join-Path $env:TEMP "decc-installer-smoke-$PID"
$InstallDir = Join-Path $SmokeRoot "install"
$StartedApp = $null
$Uninstaller = $null

if (Test-Path -LiteralPath $SmokeRoot) {
    Remove-Item -LiteralPath $SmokeRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $SmokeRoot | Out-Null

try {
    Write-Host "Installing NSIS artifact into isolated smoke path: $InstallDir"
    $Install = Start-Process -FilePath $InstallerPath -ArgumentList @("/S", "/NS", "/D=$InstallDir") -Wait -PassThru
    if ($Install.ExitCode -ne 0) {
        throw "NSIS installer failed with exit code $($Install.ExitCode)."
    }
    if (-not (Test-Path -LiteralPath $InstallDir -PathType Container)) {
        throw "NSIS installer did not create the requested isolated install directory."
    }

    $RegisteredProductLocation = Get-ProductInstallLocation
    if ($RegisteredProductLocation -ne $InstallDir) {
        throw "NSIS product registry location mismatch: expected '$InstallDir', got '$RegisteredProductLocation'."
    }

    $RegisteredUninstallLocation = Get-UninstallInstallLocation
    if ($RegisteredUninstallLocation -ne $InstallDir) {
        throw "NSIS uninstall registry location mismatch: expected '$InstallDir', got '$RegisteredUninstallLocation'."
    }

    $AppExecutable = Get-ChildItem -LiteralPath $InstallDir -Recurse -File -Filter "*.exe" |
        Where-Object { $_.Name -notmatch "^uninstall(?:er)?\.exe$" } |
        Where-Object { $_.VersionInfo.ProductName -eq $ExpectedName } |
        Select-Object -First 1

    if (-not $AppExecutable) {
        throw "Installed DECC executable was not found under $InstallDir."
    }
    if ([string]$AppExecutable.VersionInfo.FileVersion -ne $ExpectedVersion) {
        throw "Installed executable version mismatch: expected $ExpectedVersion, got $($AppExecutable.VersionInfo.FileVersion)."
    }

    Write-Host "Launching installed executable: $($AppExecutable.FullName)"
    $StartedApp = Start-Process -FilePath $AppExecutable.FullName -PassThru
    Start-Sleep -Seconds 3
    if ($StartedApp.HasExited) {
        throw "Installed DECC exited during smoke launch with code $($StartedApp.ExitCode)."
    }

    & taskkill.exe /PID $StartedApp.Id /T /F | Out-Null
    $StartedApp.WaitForExit()
    $StartedApp = $null

    $Uninstaller = Get-ChildItem -LiteralPath $InstallDir -Recurse -File -Filter "uninstall*.exe" |
        Select-Object -First 1
    if (-not $Uninstaller) {
        throw "NSIS uninstaller was not found under $InstallDir."
    }

    Write-Host "Uninstalling isolated smoke installation."
    $Uninstall = Start-Process -FilePath $Uninstaller.FullName -ArgumentList "/S" -Wait -PassThru
    if ($Uninstall.ExitCode -ne 0) {
        throw "NSIS uninstaller failed with exit code $($Uninstall.ExitCode)."
    }

    Start-Sleep -Seconds 2
    if (Test-Path -LiteralPath $InstallDir) {
        throw "NSIS uninstall left the isolated install directory behind: $InstallDir"
    }

    Remove-StaleSmokeRegistryState

    if (Test-IsSmokeInstallLocation (Get-ProductInstallLocation)) {
        throw "NSIS smoke left the product install registry pointing at a temporary smoke path."
    }
    if (Test-Path -LiteralPath $UninstallRegistryPath) {
        throw "NSIS smoke left the DECC uninstall registration behind."
    }

    Write-Host "Windows installer smoke passed - install -> launch -> uninstall - $ExpectedName $ExpectedVersion"
}
finally {
    if ($StartedApp -and -not $StartedApp.HasExited) {
        & taskkill.exe /PID $StartedApp.Id /T /F | Out-Null
    }

    if ($Uninstaller -and (Test-Path -LiteralPath $Uninstaller.FullName)) {
        try {
            Start-Process -FilePath $Uninstaller.FullName -ArgumentList "/S" -Wait | Out-Null
        }
        catch {
        }
    }

    try {
        Remove-StaleSmokeRegistryState
    }
    catch {
        Write-Warning "Could not clean smoke registry state: $($_.Exception.Message)"
    }

    if (Test-Path -LiteralPath $SmokeRoot) {
        Remove-Item -LiteralPath $SmokeRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
