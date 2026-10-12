<#
.SYNOPSIS
    Enforces the Cairn versioning policy (docs/VERSIONING.md). Used by CI and runnable locally.

.DESCRIPTION
    Always: VERSION is strict MAJOR.MINOR.PATCH.
    Against the latest release tag: VERSION is either unchanged or exactly one valid bump
    (one component up, everything to its right reset to 0) of the latest tag.
    With -Tag (a release): the tag is the bare version, equals VERSION, and is greater than every existing release tag.
#>
param([string]$Tag)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$strict = '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'

function Fail($m) { Write-Host "VERSION POLICY: $m" -ForegroundColor Red; exit 1 }

$version = (Get-Content (Join-Path $root "VERSION") -TotalCount 1).Trim()
if ($version -cnotmatch $strict) { Fail "VERSION '$version' is not strict MAJOR.MINOR.PATCH" }

$tags = @(& git -C $root tag --list | Where-Object { $_ -cmatch $strict -and $_ -ne $Tag } |
    Sort-Object { [version]$_ })
$latest = if ($tags.Count) { $tags[-1] } else { $null }

if ($Tag) {
    if ($Tag -cnotmatch $strict) { Fail "tag '$Tag' must be the bare version X.Y.Z (no 'v' prefix, no suffix)" }
    if ($Tag -ne $version) { Fail "tag '$Tag' does not match VERSION '$version'" }
}

if ($latest) {
    $c = [version]$version; $l = [version]$latest
    $isBump = ($c.Major -eq $l.Major + 1 -and $c.Minor -eq 0 -and $c.Build -eq 0) -or
              ($c.Major -eq $l.Major -and $c.Minor -eq $l.Minor + 1 -and $c.Build -eq 0) -or
              ($c.Major -eq $l.Major -and $c.Minor -eq $l.Minor -and $c.Build -eq $l.Build + 1)
    if ($c -lt $l) { Fail "VERSION $version is lower than the latest release $latest" }
    if ($Tag -and $c -le $l) { Fail "release $version must be greater than the latest release $latest" }
    if ($c -ne $l -and -not $isBump) { Fail "VERSION $version is not a single valid bump of the latest release $latest" }
}

Write-Host "Version policy OK: VERSION=$version latest-release=$(if ($latest) { $latest } else { 'none' })"
