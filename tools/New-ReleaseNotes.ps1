<#
.SYNOPSIS
    Writes release notes that contain only the changelog: one bullet per user-facing change since the previous release.

.DESCRIPTION
    Takes commit subjects between the previous release tag and HEAD, keeps feat/fix/perf/refactor/revert commits,
    drops the type and scope prefix and any "X.Y.Z: " release prefix, and writes plain bullets; a "* " line in a commit body is one more bullet. Nothing else is written.
#>
param(
    [Parameter(Mandatory)][string]$OutFile,
    [string]$Tag
)

$root = Split-Path $PSScriptRoot -Parent
$strict = '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
$prev = @(& git -C $root tag --list | Where-Object { $_ -cmatch $strict -and $_ -ne $Tag } |
    Sort-Object { [version]$_ }) | Select-Object -Last 1

$range = if ($prev) { "$prev..HEAD" } else { "HEAD" }
# One record per commit: the subject, then the body. A commit that qualifies by its subject contributes its subject; any body line
# that starts with "* " is an extra changelog entry (so one commit can carry several user-facing changes).
$raw = (& git -C $root log $range --no-merges --pretty=format:"%x1e%s%x1f%b") -join "`n"   # git prints one array element per line: join before splitting into commits
$records = $raw -split [char]0x1e | Where-Object { $_.Trim() }

$lines = foreach ($r in $records) {
    $subject, $body = $r -split [char]0x1f, 2
    $t = ($subject.Trim()) -replace '^\d+\.\d+\.\d+:\s*', ''
    $qualifies = $t -notmatch '^\d+\.\d+\.\d+$'   # the bare version commit that starts a repository is no change
    if ($t -match '^(?<type>[a-z]+)(\((?<scope>[^)]*)\))?!?:\s*(?<msg>.+)$') {
        if ($Matches.type -notin 'feat', 'fix', 'perf', 'refactor', 'revert') { $qualifies = $false } else { $t = $Matches.msg }
    }
    if ($qualifies) {
        $bullets = @(foreach ($b in ($body -split "`r?`n")) {
            if ($b -match '^\*\s+(?<e>.+)$') { "- " + $Matches.e.Substring(0, 1).ToUpper() + $Matches.e.Substring(1) }
        })
        # a commit that lists its own changes contributes those; otherwise its subject is the entry
        if ($bullets.Count) { $bullets } elseif ($t) { "- " + $t.Substring(0, 1).ToUpper() + $t.Substring(1) }
    }
}
$lines = @($lines | Select-Object -Unique)
if (-not $lines.Count) { $lines = @("- Maintenance and stability improvements") }
Set-Content -Path $OutFile -Value $lines -Encoding utf8
Get-Content $OutFile
