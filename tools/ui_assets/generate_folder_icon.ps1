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
foreach ($pen in @($orange)) {
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
}

try {
    # Matches folder() in tools/ui_assets/generate_editor_icons.py: one clean
    # folder silhouette with a top-left tab and no internal fold lines.
    $outline = [System.Drawing.PointF[]]@(
        [System.Drawing.PointF]::new(36, 84),
        [System.Drawing.PointF]::new(36, 204),
        [System.Drawing.PointF]::new(220, 204),
        [System.Drawing.PointF]::new(220, 84),
        [System.Drawing.PointF]::new(120, 84),
        [System.Drawing.PointF]::new(100, 60),
        [System.Drawing.PointF]::new(36, 60),
        [System.Drawing.PointF]::new(36, 84)
    )
    $graphics.DrawLines($orange, $outline)
    $bitmap.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)
}
finally {
    $orange.Dispose()
    $graphics.Dispose()
    $bitmap.Dispose()
}
