# Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
<#
.SYNOPSIS
    Assembles release\Cairn: the launcher, the master server (in its own MasterServer\ folder) and the setup helper, self-contained
    win-x64, so nothing has to be installed beside them. The programs that run a game's session come with that game's mod pack,
    never with the launcher.

.DESCRIPTION
    The master server goes into its own folder: flat-merging it let a later copy overwrite one of its dependency DLLs with an
    incompatible version from another project's bundle ("Could not load Microsoft.Extensions.Configuration.Abstractions").
#>
param([string]$OutDir = (Join-Path $PSScriptRoot "..\release\Cairn"))

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Path $OutDir | Out-Null

function Publish-Project($csproj, $publishSubdir) {
    Write-Output "Publishing $csproj ..."
    & dotnet publish $csproj -c Release -p:PublishProfile=FolderProfile 2>&1 | ForEach-Object { Write-Output $_ }
    if ($LASTEXITCODE -ne 0) { throw "publish failed: $csproj" }
    $dir = Join-Path (Split-Path $csproj -Parent) $publishSubdir
    if (-not (Test-Path $dir)) { throw "expected publish output not found: $dir" }
    $script:PublishDir = $dir
}

Publish-Project (Join-Path $root "Cairn.Launcher\Cairn.Launcher.csproj") "bin\Release\net8.0-windows\publish"
Copy-Item "$($script:PublishDir)\*" $OutDir -Recurse -Force

Publish-Project (Join-Path $root "dotnet\Cairn.MasterServer\Cairn.MasterServer.csproj") "bin\Release\net8.0\publish"
$masterOut = Join-Path $OutDir "MasterServer"
New-Item -ItemType Directory -Path $masterOut -Force | Out-Null
Copy-Item "$($script:PublishDir)\*" $masterOut -Recurse -Force


Write-Output "release payload: $OutDir"
