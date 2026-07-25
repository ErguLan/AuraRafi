param(
    [string]$OutputPath = (Join-Path $PSScriptRoot '..\..\editor\assets\ui_icons\folder.png')
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$directory = Split-Path -Parent $OutputPath
[System.IO.Directory]::CreateDirectory($directory) | Out-Null

$bitmap = [System.Drawing.Bitmap]::new(
    256,
    256,
    [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
$graphics.Clear([System.Drawing.Color]::Transparent)
$graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceOver

$orange = [System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(255, 224, 116, 24), 12)
$white = [System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(255, 245, 245, 245), 8)
$muted = [System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(255, 184, 184, 184), 8)
foreach ($pen in @($orange, $white, $muted)) {
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
}

try {
    $outline = [System.Drawing.PointF[]]@(
        [System.Drawing.PointF]::new(32, 80),
        [System.Drawing.PointF]::new(32, 200),
        [System.Drawing.PointF]::new(224, 200),
        [System.Drawing.PointF]::new(224, 80),
        [System.Drawing.PointF]::new(124, 80),
        [System.Drawing.PointF]::new(104, 56),
        [System.Drawing.PointF]::new(32, 56),
        [System.Drawing.PointF]::new(32, 80)
    )
    $graphics.DrawLines($orange, $outline)
    $graphics.DrawLine($white, 40, 96, 216, 96)
    $graphics.DrawLine($muted, 56, 176, 200, 176)
    $bitmap.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)
}
finally {
    $muted.Dispose()
    $white.Dispose()
    $orange.Dispose()
    $graphics.Dispose()
    $bitmap.Dispose()
}
