# Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
<#
.SYNOPSIS
    One command, one Setup.exe: runs the gates, assembles release\Cairn with tools\Publish-Release.ps1 and compiles
    installer\Cairn.iss with Inno Setup into release\Cairn-Setup-<version>.exe.

.DESCRIPTION
    The version comes from the VERSION file and from nowhere else: it is stamped into the Setup filename, the installer's
    Add/Remove Programs entry and the exe's version resource. The installer carries only the launcher; each game's mod is a pack
    the launcher downloads from that game's own repository (docs\PACKS.md).

    Code signing is not a switch: when the Azure Artifact Signing settings are present (tools\CodeSigning.ps1) every shipped exe
    and DLL, then the Setup and its uninstaller, are signed with a timestamp and checked; when absent the build is unsigned and
    the build log says so.

.PARAMETER SkipPublish
    Compile the installer against whatever release\Cairn already contains (iterating on the .iss only).
#>
param(
    [switch]$SkipPublish,
    [string]$Version,
    [ValidateSet('dark', 'dynamic', 'light')]
    [string]$Appearance = 'dark'
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

if (-not $Version) {
    $versionFile = Join-Path $root "VERSION"
    if (-not (Test-Path $versionFile)) { throw "VERSION file not found at $versionFile" }
    $Version = (Get-Content $versionFile -TotalCount 1).Trim()
}
if ($Version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
    throw "VERSION must be strict SemVer MAJOR.MINOR.PATCH (e.g. 0.1.4), got: '$Version'. See docs\VERSIONING.md"
}

function Get-Iscc {
    $onPath = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    foreach ($candidate in @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    )) {
        if (Test-Path $candidate) { return $candidate }
    }
    throw "Inno Setup 6 not found. Install it with: winget install --id JRSoftware.InnoSetup"
}

$iscc = Get-Iscc

# everything this build prints goes to release\BUILD-<version>.log too
$relDir = Join-Path $root "release"
Start-Transcript -Path (Join-Path $relDir "BUILD-$Version.log") -Force | Out-Null
trap { try { Stop-Transcript | Out-Null } catch {}; break }
$gitCommit = if (Get-Command git -ErrorAction SilentlyContinue) { & git -C $root rev-parse HEAD 2>$null } else { "unknown" }
Write-Host "Build of $Version at $(Get-Date -Format s), commit $gitCommit"
Write-Host "Inno Setup compiler: $iscc"

# code signing, when its settings are present (never committed: environment or the git-ignored tools\signing.local.json)
. (Join-Path $PSScriptRoot "CodeSigning.ps1")
$signing = Get-cairnSigningConfig
if ($signing) {
    $signTool = Find-cairnSignTool $signing
    if (-not (Test-Path $signing.Dlib)) { throw "Azure.CodeSigning.Dlib.dll not found: $($signing.Dlib)" }
    Write-Host "Code signing: ON (Azure Artifact Signing, account $($signing.Account), profile $($signing.Profile)); signtool $signTool"
} else {
    Write-Host "Code signing: OFF -- no signing settings found. This build is UNSIGNED." -ForegroundColor Yellow
}

$payload = Join-Path $root "release\Cairn"

if (-not $SkipPublish) {
    Write-Host "Unit tests (rust) ..."
    Push-Location (Join-Path $root "rust")
    try {
        & cargo test --workspace --locked
        if ($LASTEXITCODE -ne 0) { throw "the Rust tests FAILED. Not shipping." }
    } finally { Pop-Location }
    Write-Host "Unit tests (dotnet\Cairn.MasterServer.Tests) ..."
    & dotnet test (Join-Path $root "dotnet\Cairn.MasterServer.Tests\Cairn.MasterServer.Tests.csproj") -c Release --nologo -v q
    if ($LASTEXITCODE -ne 0) { throw "MasterServer.Tests FAILED. Not shipping." }

    & powershell -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "Publish-Release.ps1")
    if ($LASTEXITCODE -ne 0) { throw "Publish-Release.ps1 failed" }

    # sign the payload before anything runs it or hashes it (the manifest below describes the signed bytes)
    if ($signing) {
        $signMeta = New-cairnSigningMetadata $signing
        try { Invoke-cairnSignPayload -Dir $payload -Config $signing -SignTool $signTool -Metadata $signMeta }
        finally { Remove-Item $signMeta -Force -ErrorAction SilentlyContinue }
    }
}

# the .iss embeds this folder wholesale, so an empty or missing one would compile into an installer that installs nothing
foreach ($f in @("Cairn.Launcher.exe", "app.ico")) {
    if (-not (Test-Path (Join-Path $payload $f))) { throw "release payload incomplete: $payload\$f not found (run without -SkipPublish)" }
}


# a manifest of everything this Setup carries, so the installer can prove after installing that every file landed and that
# nothing else sits in the install directory (tools\New-InstallManifest.ps1 owns the format). It runs after publish and
# before ISCC, so it describes exactly the bytes this Setup embeds.
& (Join-Path $PSScriptRoot "New-InstallManifest.ps1") `
    -AppDir $payload `
    -ModFiles @{} `
    -OutFile (Join-Path $payload "install-manifest.txt")

$iss = Join-Path $root "installer\Cairn.iss"
Write-Host "Compiling $iss (version $Version, appearance $Appearance) ..."
$isccArgs = @("/DAppVersion=$Version", "/DWizardAppearance=$Appearance")
$signMeta = $null
if ($signing) {
    # Inno signs the Setup and the uninstaller it embeds with the same command ($f = the file)
    $signMeta = New-cairnSigningMetadata $signing
    $isccArgs += "/DcairnSign=1"
    $isccArgs += ("/Scairnsign=" + (Get-cairnSignCommand $signing $signTool $signMeta -ForInno) + ' $f')
}
try { & $iscc @isccArgs $iss }
finally { if ($signMeta) { Remove-Item $signMeta -Force -ErrorAction SilentlyContinue } }
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }

$setup = Join-Path $root "release\Cairn-Setup-$Version.exe"
if (-not (Test-Path $setup)) { throw "ISCC reported success but $setup is missing" }
if ($signing) { Assert-cairnSigned -Files @($setup) }

$mb = [math]::Round((Get-Item $setup).Length / 1MB, 1)
Write-Host ""
Write-Host "Installer: $setup ($mb MB)"
Write-Host ("sha256   : " + (Get-FileHash $setup -Algorithm SHA256).Hash.ToLowerInvariant())
Write-Host ("signed   : " + $(if ($signing) { "yes (Azure Artifact Signing)" } else { "NO -- unsigned build" }))
Stop-Transcript | Out-Null
