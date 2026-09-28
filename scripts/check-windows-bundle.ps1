param(
    [string]$InstallerPath = "",
    [switch]$RequireDistributionSigning
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$TauriConfigPath = Join-Path $Root "src-tauri\tauri.conf.json"
$TauriConfig = Get-Content -LiteralPath $TauriConfigPath -Raw | ConvertFrom-Json
$ExpectedName = [string]$TauriConfig.productName
$ExpectedVersion = [string]$TauriConfig.version
$ExecutablePath = Join-Path $Root "src-tauri\target\release\decc.exe"
$BundleDir = Join-Path $Root "src-tauri\target\release\bundle\nsis"

if ([string]::IsNullOrWhiteSpace($InstallerPath)) {
    $Architecture = switch ($env:PROCESSOR_ARCHITECTURE) {
        "AMD64" { "x64" }
        "ARM64" { "arm64" }
        default { "*" }
    }

    $Candidates = @(
        Get-ChildItem -LiteralPath $BundleDir -File -Filter "$($ExpectedName)_$($ExpectedVersion)_$Architecture-setup.exe" -ErrorAction SilentlyContinue
    )

    if ($Candidates.Count -eq 0) {
        $Candidates = @(
            Get-ChildItem -LiteralPath $BundleDir -File -Filter "$($ExpectedName)_$($ExpectedVersion)_*-setup.exe" -ErrorAction SilentlyContinue
        )
    }

    if ($Candidates.Count -ne 1) {
        throw "Expected exactly one NSIS installer for $ExpectedName $ExpectedVersion, found $($Candidates.Count)."
    }

    $InstallerPath = $Candidates[0].FullName
}

function Get-AuthenticodeInspection {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    try {
        $Signature = Get-AuthenticodeSignature -LiteralPath $Path -ErrorAction Stop
        return [PSCustomObject]@{
            Status = [string]$Signature.Status
            SignerSubject = if ($Signature.SignerCertificate) { $Signature.SignerCertificate.Subject } else { "" }
            VerificationError = ""
        }
    }
    catch {
        return [PSCustomObject]@{
            Status = "Unavailable"
            SignerSubject = ""
            VerificationError = $_.Exception.Message
        }
    }
}

function Get-Sha256 {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $Stream = [System.IO.File]::OpenRead($Path)
    try {
        $Sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            $Bytes = $Sha256.ComputeHash($Stream)
            return ([System.BitConverter]::ToString($Bytes)).Replace("-", "")
        }
        finally {
            $Sha256.Dispose()
        }
    }
    finally {
        $Stream.Dispose()
    }
}

function Inspect-ReleaseFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$Label
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Label is missing: $Path"
    }

    $File = Get-Item -LiteralPath $Path
    if ($File.Length -le 0) {
        throw "$Label is empty: $Path"
    }

    $Product = [string]$File.VersionInfo.ProductName
    $Version = [string]$File.VersionInfo.FileVersion
    if ($Product -ne $ExpectedName) {
        throw "$Label product mismatch: expected '$ExpectedName', got '$Product'."
    }
    if ($Version -ne $ExpectedVersion) {
        throw "$Label version mismatch: expected '$ExpectedVersion', got '$Version'."
    }

    $Authenticode = Get-AuthenticodeInspection -Path $Path
    $Hash = Get-Sha256 -Path $Path

    [PSCustomObject]@{
        Label = $Label
        Path = $Path
        SizeBytes = $File.Length
        Product = $Product
        Version = $Version
        SignatureStatus = $Authenticode.Status
        SignerSubject = $Authenticode.SignerSubject
        SignatureVerificationError = $Authenticode.VerificationError
        SHA256 = $Hash
    }
}

$Executable = Inspect-ReleaseFile -Path $ExecutablePath -Label "Release executable"
$Installer = Inspect-ReleaseFile -Path $InstallerPath -Label "NSIS installer"

Write-Host "Windows bundle OK - $ExpectedName $ExpectedVersion"
foreach ($Artifact in @($Executable, $Installer)) {
    Write-Host "$($Artifact.Label): $($Artifact.Path)"
    Write-Host "  Size: $($Artifact.SizeBytes) bytes"
    Write-Host "  Signature: $($Artifact.SignatureStatus)"
    if (-not [string]::IsNullOrWhiteSpace($Artifact.SignatureVerificationError)) {
        Write-Host "  Signature verification unavailable: $($Artifact.SignatureVerificationError)"
    }
    Write-Host "  SHA256: $($Artifact.SHA256)"
}

$Strict = $RequireDistributionSigning -or $env:DECC_REQUIRE_DISTRIBUTION_SIGNING -eq "1"
if ($Strict) {
    foreach ($Artifact in @($Executable, $Installer)) {
        if ($Artifact.SignatureStatus -ne "Valid" -or [string]::IsNullOrWhiteSpace($Artifact.SignerSubject)) {
            throw "Distribution verification requires a valid Authenticode signature for $($Artifact.Label). Current status: $($Artifact.SignatureStatus)."
        }
    }

    Write-Host "Windows distribution signing verification passed."
}
