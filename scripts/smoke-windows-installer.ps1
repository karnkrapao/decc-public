param(
    [string]$InstallerPath = ""
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$TauriConfig = Get-Content -LiteralPath (Join-Path $Root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$ExpectedName = [string]$TauriConfig.productName
$ExpectedVersion = [string]$TauriConfig.version
$BundleDir = Join-Path $Root "src-tauri\target\release\bundle\nsis"

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
    $Install = Start-Process -FilePath $InstallerPath -ArgumentList @("/S", "/D=$InstallDir") -Wait -PassThru
    if ($Install.ExitCode -ne 0) {
        throw "NSIS installer failed with exit code $($Install.ExitCode)."
    }
    if (-not (Test-Path -LiteralPath $InstallDir -PathType Container)) {
        throw "NSIS installer did not create the requested isolated install directory."
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

    if (Test-Path -LiteralPath $SmokeRoot) {
        Remove-Item -LiteralPath $SmokeRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
