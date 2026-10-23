# Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
# GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
# of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
<#
.SYNOPSIS
    Draws a plain typographic logo (a transparent PNG) for a game that has no logo art of its own.

.DESCRIPTION
    A big first line and a letter-spaced second line, centred, sized to fill the canvas so it stays readable in the game list's
    small logo box. It is only the game's name in a clean typeface, not a copy of anyone's artwork.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File tools\New-TextLogo.ps1 -Big "007" -Small "FIRST LIGHT" -Out fl-logo.png
#>
param(
    [Parameter(Mandatory)] [string] $Big,
    [Parameter(Mandatory)] [string] $Small,
    [Parameter(Mandatory)] [string] $Out,
    [string] $BigColor = '#E8C872',
    [string] $SmallColor = '#F2EFE6',
    [int] $Width = 640,
    [int] $Height = 360
)

Add-Type -AssemblyName System.Drawing
$bmp = New-Object Drawing.Bitmap($Width, $Height, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [Drawing.Graphics]::FromImage($bmp)
$g.Clear([Drawing.Color]::Transparent)
$g.SmoothingMode = 'AntiAlias'; $g.InterpolationMode = 'HighQualityBicubic'

function Pick-Family([string[]] $names) {
    foreach ($n in $names) { try { $f = New-Object Drawing.FontFamily($n); return $f } catch { } }
    return [Drawing.FontFamily]::GenericSansSerif
}
$bigFamily = Pick-Family @('Segoe UI Black', 'Arial Black', 'Arial')
$smallFamily = Pick-Family @('Segoe UI Semibold', 'Segoe UI', 'Arial')
$fmt = [Drawing.StringFormat]::GenericTypographic

# everything is laid out by its ink (the glyph outlines), so the result is centred and fills the canvas whatever the font's metrics
$bigPath = New-Object Drawing.Drawing2D.GraphicsPath
$bigPath.AddString($Big, $bigFamily, [int][Drawing.FontStyle]::Bold, 200, [Drawing.PointF]::new(0, 0), $fmt)
$b1 = $bigPath.GetBounds()

# the second line: one glyph path per letter, spread so the line is as wide as the first
$emSmall = 70
$glyphs = @()
foreach ($ch in $Small.ToCharArray()) {
    if ($ch -eq ' ') { $glyphs += [pscustomobject]@{ Path = $null; Width = $emSmall * 0.32 }; continue }
    $p = New-Object Drawing.Drawing2D.GraphicsPath
    $p.AddString([string]$ch, $smallFamily, [int][Drawing.FontStyle]::Regular, $emSmall, [Drawing.PointF]::new(0, 0), $fmt)
    $gb = $p.GetBounds()
    $glyphs += [pscustomobject]@{ Path = $p; Width = $gb.Width; Left = $gb.Left; Top = $gb.Top; Height = $gb.Height }
}
$natural = ($glyphs | Measure-Object -Property Width -Sum).Sum
$tracking = [math]::Max(0, [math]::Min($emSmall * 0.40, ($b1.Width - $natural) / [math]::Max(1, $glyphs.Count - 1)))
$lineWidth = $natural + $tracking * ($glyphs.Count - 1)
$capHeight = ($glyphs | Where-Object { $_.Path } | Measure-Object -Property Height -Maximum).Maximum

$gap = 34
$compW = [math]::Max($b1.Width, $lineWidth); $compH = $b1.Height + $gap + $capHeight
$scale = [math]::Min(($Width - 28) / $compW, ($Height - 28) / $compH)
$ox = ($Width - $compW * $scale) / 2; $oy = ($Height - $compH * $scale) / 2

$bigBrush = New-Object Drawing.SolidBrush([Drawing.ColorTranslator]::FromHtml($BigColor))
$smallBrush = New-Object Drawing.SolidBrush([Drawing.ColorTranslator]::FromHtml($SmallColor))

$m = New-Object Drawing.Drawing2D.Matrix
$m.Translate([single]($ox + ($compW - $b1.Width) / 2 * $scale - $b1.Left * $scale), [single]($oy - $b1.Top * $scale))
$m.Scale([single]$scale, [single]$scale)
$bp = $bigPath.Clone(); $bp.Transform($m); $g.FillPath($bigBrush, $bp)

$x = ($compW - $lineWidth) / 2
$baseY = $b1.Height + $gap
foreach ($gl in $glyphs) {
    if ($gl.Path) {
        $mm = New-Object Drawing.Drawing2D.Matrix
        $mm.Translate([single]($ox + ($x - $gl.Left) * $scale), [single]($oy + ($baseY - $gl.Top) * $scale))
        $mm.Scale([single]$scale, [single]$scale)
        $sp = $gl.Path.Clone(); $sp.Transform($mm); $g.FillPath($smallBrush, $sp)
    }
    $x += $gl.Width + $tracking
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
$bmp.Save($Out, [Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Host "wrote $Out"
