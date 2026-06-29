Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
$workspaceCargoToml = Join-Path $repoRoot "Cargo.toml"
$portableScript = Join-Path $scriptDir "package-windows.ps1"
$installerScript = Join-Path $repoRoot "packaging\windows\installer.iss"
$iconPath = Join-Path $repoRoot "assets\aegis-vault.ico"
$distRoot = Join-Path $repoRoot "dist\windows"

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

function Resolve-InnoCompiler {
    if ($env:INNO_SETUP_COMPILER -and (Test-Path $env:INNO_SETUP_COMPILER)) {
        return $env:INNO_SETUP_COMPILER
    }

    $candidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 6\ISCC.exe"
    )

    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path $candidate)) {
            return $candidate
        }
    }

    throw "Inno Setup compiler not found. Install Inno Setup 6 or set INNO_SETUP_COMPILER."
}

$version = Get-WorkspaceVersion -CargoTomlPath $workspaceCargoToml
$packageName = "Aegis-Vault-v$version-win64"
$packageDir = Join-Path $distRoot $packageName
$outputBaseName = "Aegis-Vault-Setup-v$version-win64.exe"
$outputPath = Join-Path $distRoot $outputBaseName
$iscc = Resolve-InnoCompiler

if (-not (Test-Path $iconPath)) {
    throw "Installer icon not found: $iconPath"
}

Write-Host "Building portable package..."
& powershell -ExecutionPolicy Bypass -File $portableScript

if (-not (Test-Path $packageDir)) {
    throw "Portable package directory not found: $packageDir"
}

if (Test-Path $outputPath) {
    Remove-Item -LiteralPath $outputPath -Force
}

Write-Host "Building installer with Inno Setup..."
& $iscc `
    "/DMyAppVersion=$version" `
    "/DMySourceDir=$packageDir" `
    "/DMyOutputDir=$distRoot" `
    "/DMyIconFile=$iconPath" `
    $installerScript

if (-not (Test-Path $outputPath)) {
    throw "Installer output not found: $outputPath"
}

Write-Host ""
Write-Host "Windows installer created:"
Write-Host "  Installer: $outputPath"
