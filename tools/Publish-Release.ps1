# Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
<#
.SYNOPSIS
    Assembles release\Cairn: the launcher (one native Rust executable, named Cairn.Launcher.exe so shortcuts and upgrades keep working),
    its icon, and the master server (in its own MasterServer\ folder, self-contained win-x64). The programs that run a game's session
    come with that game's mod pack, never with the launcher.

.DESCRIPTION
    The master server goes into its own folder: flat-merging it let a later copy overwrite one of its dependency DLLs with an
    incompatible version from another project's bundle ("Could not load Microsoft.Extensions.Configuration.Abstractions").
#>
param([string]$OutDir = (Join-Path $PSScriptRoot "..\release\Cairn"))

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Path $OutDir | Out-Null

Write-Output "Building the launcher (cargo build --release) ..."
Push-Location (Join-Path $root "rust")
try {
    & cargo build --release -p cairn-launcher --locked 2>&1 | ForEach-Object { Write-Output $_ }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally { Pop-Location }
$built = Join-Path $root "rust\target\release\cairn.exe"
if (-not (Test-Path $built)) { throw "expected build output not found: $built" }
Copy-Item $built (Join-Path $OutDir "Cairn.Launcher.exe")
Copy-Item (Join-Path $root "rust\crates\cairn-launcher\assets\app.ico") (Join-Path $OutDir "app.ico")

Write-Output "Publishing the master server ..."
$masterProj = Join-Path $root "dotnet\Cairn.MasterServer\Cairn.MasterServer.csproj"
& dotnet publish $masterProj -c Release -p:PublishProfile=FolderProfile 2>&1 | ForEach-Object { Write-Output $_ }
if ($LASTEXITCODE -ne 0) { throw "publish failed: $masterProj" }
$masterDir = Join-Path (Split-Path $masterProj -Parent) "bin\Release\net8.0\publish"
if (-not (Test-Path $masterDir)) { throw "expected publish output not found: $masterDir" }
$masterOut = Join-Path $OutDir "MasterServer"
New-Item -ItemType Directory -Path $masterOut -Force | Out-Null
Copy-Item "$masterDir\*" $masterOut -Recurse -Force

Write-Output "release payload: $OutDir"
