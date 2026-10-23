# Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
# Draws Cairn's logo with System.Drawing, so no art tool is needed: three flat stones and one gold pebble, balanced, on a slate tile.
#   assets\cairn-icon.png                      512 px master
#   rust\crates\cairn-launcher\assets\app.ico   multi-size icon (through Build-LauncherIcon.ps1)
#   installer\wizard-*.png                     the installer's side banners
# Re-run after changing the drawing; it always overwrites.
Add-Type -AssemblyName System.Drawing
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
function C([string]$hex) { [Drawing.ColorTranslator]::FromHtml($hex) }

# Draws the emblem into a 0..100 unit square placed at ($ox,$oy) with side $s. Flat fills only.
# Flat slabs of rock, each a hand-shaped irregular polygon (uneven top, ragged underside), stacked and shifted like a hiker's cairn.
# Points are in a 100 x 100 tile, y down; bottom slab first. A thin gap of tile colour is cut around each slab so they stay distinct.
function Draw-Emblem($g, [double]$ox, [double]$oy, [double]$s) {
    $k = $s / 100.0
    # two colours, kept minimal: a moon-gold tile and slate slabs, each a touch lighter than the one below, with a faint edge light
    $gold = C '#E8C872'
    $g.FillRectangle((New-Object Drawing.SolidBrush($gold)), [single]$ox, [single]$oy, [single]$s, [single]$s)
    $g.SmoothingMode = 'AntiAlias'
    $slabs = @(
        @('#14202A', @(12,74, 30,71, 60,72, 86,74.5, 88,80, 80,86, 36,86.5, 14,83)),
        @('#182632', @(22,64, 40,61, 66,62.5, 82,64, 80,71, 72,73, 34,73.5, 21,70)),
        @('#1D2D3B', @(17,53.5, 34,51.5, 58,52.5, 72,54, 73,60, 64,62.5, 30,62, 16,59)),
        @('#233444', @(29,44, 46,42, 70,43, 79,45.5, 77,51.5, 66,53, 38,52.5, 28,50)),
        @('#2A3C4F', @(34,35.5, 50,33.5, 66,35, 70,37.5, 68,43, 58,44, 40,43.5, 33,41)),
        @('#314558', @(39,28, 52,26, 62,27.5, 64,30, 62,35.5, 52,36.5, 42,36, 38,33)),
        @('#38505F', @(43,21, 51,19, 58,20.5, 59,23, 57,27.5, 50,28.5, 44,27.5, 42,24.5))
    )
    $shift = -2.5   # centre the stack in the tile
    foreach ($sl in $slabs) {
        $p = $sl[1]; $pts = New-Object 'System.Collections.Generic.List[Drawing.PointF]'
        for ($i = 0; $i -lt $p.Count; $i += 2) { $pts.Add((New-Object Drawing.PointF([single]($ox + $p[$i] * $k), [single]($oy + ($p[$i + 1] + $shift) * $k)))) }
        $arr = $pts.ToArray(); $col = C $sl[0]
        # the gap: this slab's outline, a little wider, in the tile colour (it trims the slab below)
        $gapBrush = New-Object Drawing.SolidBrush($gold); $gapPen = New-Object Drawing.Pen($gold, [single](4.6 * $k)); $gapPen.LineJoin = 'Round'
        $g.FillPolygon($gapBrush, $arr); $g.DrawPolygon($gapPen, $arr)
        $brush = New-Object Drawing.SolidBrush($col)
        $pen = New-Object Drawing.Pen($col, [single](1.8 * $k)); $pen.LineJoin = 'Round'      # the stroke softens the corners
        $g.FillPolygon($brush, $arr); $g.DrawPolygon($pen, $arr)
        # the edge light: along the top edge, from the first corner to the third
        $edge = New-Object Drawing.Pen([Drawing.Color]::FromArgb(58, 255, 255, 255), [single]([math]::Max(1.0, 1.2 * $k)))
        $edge.StartCap = 'Round'; $edge.EndCap = 'Round'; $edge.LineJoin = 'Round'
        $g.DrawLines($edge, [Drawing.PointF[]]@($arr[0], $arr[1], $arr[2]))
        $gapBrush.Dispose(); $gapPen.Dispose(); $brush.Dispose(); $pen.Dispose(); $edge.Dispose()
    }
}

function New-Canvas([int]$w, [int]$h) {
    $bmp = New-Object Drawing.Bitmap($w, $h, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'; $g.InterpolationMode = 'HighQualityBicubic'; $g.TextRenderingHint = 'AntiAliasGridFit'
    return @{ Bmp = $bmp; G = $g }
}

function Set-RoundClip($g, [double]$x, [double]$y, [double]$w, [double]$h, [double]$r) {
    $p = New-Object Drawing.Drawing2D.GraphicsPath
    $p.AddArc($x, $y, 2*$r, 2*$r, 180, 90); $p.AddArc($x+$w-2*$r, $y, 2*$r, 2*$r, 270, 90)
    $p.AddArc($x+$w-2*$r, $y+$h-2*$r, 2*$r, 2*$r, 0, 90); $p.AddArc($x, $y+$h-2*$r, 2*$r, 2*$r, 90, 90); $p.CloseAllFigures()
    $g.SetClip($p)
}

function Save-Icon([int]$size, [string]$path) {
    $c = New-Canvas $size $size
    Set-RoundClip $c.G 0 0 $size $size ($size * 0.22)
    Draw-Emblem $c.G 0 0 $size
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    $c.Bmp.Save($path, [Drawing.Imaging.ImageFormat]::Png); $c.G.Dispose(); $c.Bmp.Dispose()
    Write-Host "wrote $path"
}

Save-Icon 512 (Join-Path $root 'assets\cairn-icon.png')

# Installer wizard side banners (328 x 628, ratio 164:314 for Inno Setup)
$iw = 328; $ih = 628
$fmtCenter = New-Object Drawing.StringFormat; $fmtCenter.Alignment = 'Center'
$wizTitle = New-Object Drawing.Font('Segoe UI Bold', 34, [Drawing.FontStyle]::Regular, [Drawing.GraphicsUnit]::Pixel)
$wizTag = New-Object Drawing.Font('Segoe UI', 15, [Drawing.FontStyle]::Regular, [Drawing.GraphicsUnit]::Pixel)

# Dark banner
$cd = New-Canvas $iw $ih
$cd.G.Clear((C '#0E151C'))
Set-RoundClip $cd.G 89 105 150 150 33
Draw-Emblem $cd.G 89 105 150
$cd.G.ResetClip()
$cd.G.DrawString('CAIRN', $wizTitle, (New-Object Drawing.SolidBrush((C '#F2BA80'))), [single]($iw / 2), 280, $fmtCenter)
$goldPen = New-Object Drawing.Pen((C '#E8A35A'), 2)
$cd.G.DrawLine($goldPen, [single]($iw / 2 - 24), 332, [single]($iw / 2 + 24), 332)
$cd.G.DrawString('Play single-player games', $wizTag, (New-Object Drawing.SolidBrush((C '#B5BCC0'))), [single]($iw / 2), 350, $fmtCenter)
$cd.G.DrawString('together.', $wizTag, (New-Object Drawing.SolidBrush((C '#B5BCC0'))), [single]($iw / 2), 374, $fmtCenter)
$darkBannerPath = Join-Path $root 'installer\wizard-image-dark.png'
$cd.Bmp.Save($darkBannerPath, [Drawing.Imaging.ImageFormat]::Png); $cd.G.Dispose(); $cd.Bmp.Dispose()
Write-Host "wrote $darkBannerPath"

# Light banner
$cl = New-Canvas $iw $ih
$cl.G.Clear((C '#F4F5F7'))
Set-RoundClip $cl.G 89 105 150 150 33
Draw-Emblem $cl.G 89 105 150
$cl.G.ResetClip()
$cl.G.DrawString('CAIRN', $wizTitle, (New-Object Drawing.SolidBrush((C '#14202A'))), [single]($iw / 2), 280, $fmtCenter)
$cl.G.DrawLine($goldPen, [single]($iw / 2 - 24), 332, [single]($iw / 2 + 24), 332)
$cl.G.DrawString('Play single-player games', $wizTag, (New-Object Drawing.SolidBrush((C '#5A646E'))), [single]($iw / 2), 350, $fmtCenter)
$cl.G.DrawString('together.', $wizTag, (New-Object Drawing.SolidBrush((C '#5A646E'))), [single]($iw / 2), 374, $fmtCenter)
$goldPen.Dispose()
$lightBannerPath = Join-Path $root 'installer\wizard-image-light.png'
$cl.Bmp.Save($lightBannerPath, [Drawing.Imaging.ImageFormat]::Png); $cl.G.Dispose(); $cl.Bmp.Dispose()
Write-Host "wrote $lightBannerPath"

# Installer small top-right icon (147 x 147)
$cs = New-Canvas 147 147
Set-RoundClip $cs.G 12 12 123 123 27
Draw-Emblem $cs.G 12 12 123
$cs.G.ResetClip()
$smallPath = Join-Path $root 'installer\wizard-small.png'
$cs.Bmp.Save($smallPath, [Drawing.Imaging.ImageFormat]::Png); $cs.G.Dispose(); $cs.Bmp.Dispose()
Write-Host "wrote $smallPath"

& (Join-Path $PSScriptRoot 'Build-LauncherIcon.ps1') -SourcePng (Join-Path $root 'assets\cairn-icon.png') -OutputIco (Join-Path $root 'rust\crates\cairn-launcher\assets\app.ico')

