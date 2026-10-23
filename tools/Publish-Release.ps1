# Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
<#
.SYNOPSIS
    Assembles release\Cairn: the launcher (one native Rust executable, named Cairn.Launcher.exe so shortcuts and upgrades keep working),
    its icon, and the master server (CairnMasterServer.exe and its appsettings.json, in their own MasterServer\ folder). The programs that run a game's session
    come with that game's mod pack, never with the launcher.

.DESCRIPTION
    The master server keeps its own folder, as before: the launcher starts it from MasterServer\CairnMasterServer.exe.
#>
param([string]$OutDir = (Join-Path $PSScriptRoot "..\release\Cairn"))

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Path $OutDir | Out-Null

Write-Output "Building the launcher and the master server (cargo build --release) ..."
Push-Location (Join-Path $root "rust")
# cargo prints its progress on stderr, which PowerShell would turn into a terminating error under "Stop"
$previous = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    & cargo build --release -p cairn-launcher -p cairn-master-server --locked 2>&1 | ForEach-Object { Write-Output "$_" }
    $cargoExit = $LASTEXITCODE
} finally { $ErrorActionPreference = $previous; Pop-Location }
if ($cargoExit -ne 0) { throw "cargo build failed" }
$built = Join-Path $root "rust\target\release\cairn.exe"
if (-not (Test-Path $built)) { throw "expected build output not found: $built" }
Copy-Item $built (Join-Path $OutDir "Cairn.Launcher.exe")
Copy-Item (Join-Path $root "rust\crates\cairn-launcher\assets\app.ico") (Join-Path $OutDir "app.ico")

$masterOut = Join-Path $OutDir "MasterServer"
New-Item -ItemType Directory -Path $masterOut -Force | Out-Null
Copy-Item (Join-Path $root "rust\target\release\cairn-master-server.exe") (Join-Path $masterOut "CairnMasterServer.exe")
Copy-Item (Join-Path $root "rust\crates\cairn-master-server\appsettings.json") (Join-Path $masterOut "appsettings.json")

Write-Output "release payload: $OutDir"
