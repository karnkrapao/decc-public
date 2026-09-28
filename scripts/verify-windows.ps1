$ErrorActionPreference = "Stop"

function Invoke-NativeStep {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Command
    )

    Write-Host $Label
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE."
    }
}

Invoke-NativeStep "Installing locked frontend dependencies..." {
    pnpm install --frozen-lockfile
}

Invoke-NativeStep "Running canonical verification..." {
    pnpm verify
}

Invoke-NativeStep "Building integrated Tauri app without bundling/signing..." {
    pnpm tauri build --debug --no-bundle
}

Write-Host "Windows verification gates passed."
