Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
$workspaceCargoToml = Join-Path $repoRoot "Cargo.toml"

function Get-WorkspaceVersion {
    param(
        [string]$CargoTomlPath
    )

    $content = Get-Content $CargoTomlPath
    $inWorkspacePackage = $false

    foreach ($line in $content) {
        if ($line -match '^\[workspace\.package\]') {
            $inWorkspacePackage = $true
            continue
        }
        if ($inWorkspacePackage -and $line -match '^\[') {
            break
        }
        if ($inWorkspacePackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') {
            return $Matches[1]
        }
    }

    throw "Could not find workspace package version in $CargoTomlPath"
}

$version = Get-WorkspaceVersion -CargoTomlPath $workspaceCargoToml
$releaseExe = Join-Path $repoRoot "target\release\encrypt-app.exe"
$distRoot = Join-Path $repoRoot "dist\windows"
$packageName = "Aegis-Vault-v$version-win64"
$packageDir = Join-Path $distRoot $packageName
$zipPath = Join-Path $distRoot ($packageName + ".zip")
$appExeName = "Aegis Vault.exe"
$appExePath = Join-Path $packageDir $appExeName
$readmeTemplate = Join-Path $repoRoot "packaging\windows\README-WINDOWS.txt"
$packageInfoPath = Join-Path $packageDir "PACKAGE-INFO.txt"
$projectReadmePath = Join-Path $packageDir "README-project.md"
$launcherPath = Join-Path $packageDir "launch-aegis-vault.bat"
$shaPath = Join-Path $packageDir "SHA256SUMS.txt"

Write-Host "Building release binary..."
cargo build -p desktop-app --release

if (-not (Test-Path $releaseExe)) {
    throw "Release binary not found: $releaseExe"
}

if (Test-Path $packageDir) {
    Remove-Item -LiteralPath $packageDir -Recurse -Force
}

if (Test-Path $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}

New-Item -ItemType Directory -Path $packageDir -Force | Out-Null

Copy-Item -LiteralPath $releaseExe -Destination $appExePath
Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination $projectReadmePath
Copy-Item -LiteralPath $readmeTemplate -Destination (Join-Path $packageDir "README-WINDOWS.txt")

$packageInfo = @(
    "Product: Aegis Vault"
    "Version: $version"
    "Platform: Windows x64"
    "Build Date (UTC): $([DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss'))"
    "Binary: $appExeName"
    "Package Type: Portable ZIP"
)
Set-Content -LiteralPath $packageInfoPath -Value $packageInfo -Encoding UTF8

$launcher = @(
    "@echo off"
    "set SCRIPT_DIR=%~dp0"
    'start "" "%SCRIPT_DIR%Aegis Vault.exe"'
)
Set-Content -LiteralPath $launcherPath -Value $launcher -Encoding ASCII

$hash = Get-FileHash -LiteralPath $appExePath -Algorithm SHA256
$hashLine = "$($hash.Hash.ToLowerInvariant()) *$appExeName"
Set-Content -LiteralPath $shaPath -Value $hashLine -Encoding ASCII

Write-Host "Creating archive..."
Compress-Archive -Path (Join-Path $packageDir "*") -DestinationPath $zipPath -CompressionLevel Optimal

Write-Host ""
Write-Host "Windows package created:"
Write-Host "  Folder: $packageDir"
Write-Host "  Zip:    $zipPath"
