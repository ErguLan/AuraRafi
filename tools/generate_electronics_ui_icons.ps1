param(
    [string]$OutputRoot = (Join-Path $PSScriptRoot '..\editor\assets\electronics')
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$size = 256
$stroke = 16.0
$foreground = [System.Drawing.Color]::FromArgb(255, 236, 240, 244)
$accent = [System.Drawing.Color]::FromArgb(255, 244, 143, 24)

function New-Icon {
    param(
        [string]$RelativePath,
        [scriptblock]$Draw
    )

    $path = Join-Path $OutputRoot $RelativePath
    $directory = Split-Path -Parent $path
    [System.IO.Directory]::CreateDirectory($directory) | Out-Null

    $bitmap = [System.Drawing.Bitmap]::new($size, $size)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.Clear([System.Drawing.Color]::Transparent)

    $pen = [System.Drawing.Pen]::new($foreground, $stroke)
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $accentPen = [System.Drawing.Pen]::new($accent, $stroke)
    $accentPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $accentPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $brush = [System.Drawing.SolidBrush]::new($foreground)
    $accentBrush = [System.Drawing.SolidBrush]::new($accent)

    try {
        & $Draw $graphics $pen $accentPen $brush $accentBrush
        $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $accentBrush.Dispose()
        $brush.Dispose()
        $accentPen.Dispose()
        $pen.Dispose()
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Points([int[]]$values) {
    $points = [System.Collections.Generic.List[System.Drawing.PointF]]::new()
    for ($i = 0; $i -lt $values.Length; $i += 2) {
        $points.Add([System.Drawing.PointF]::new($values[$i], $values[$i + 1]))
    }
    return $points.ToArray()
}

function Draw-RoundedRect {
    param(
        $Graphics,
        $Pen,
        [single]$X,
        [single]$Y,
        [single]$Width,
        [single]$Height,
        [single]$Radius
    )

    $diameter = $Radius * 2
    $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $path.AddArc($X, $Y, $diameter, $diameter, 180, 90)
    $path.AddArc($X + $Width - $diameter, $Y, $diameter, $diameter, 270, 90)
    $path.AddArc($X + $Width - $diameter, $Y + $Height - $diameter, $diameter, $diameter, 0, 90)
    $path.AddArc($X, $Y + $Height - $diameter, $diameter, $diameter, 90, 90)
    $path.CloseFigure()
    try {
        $Graphics.DrawPath($Pen, $path)
    }
    finally {
        $path.Dispose()
    }
}

New-Icon 'toolbar/select.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLines($pen, (Points 48,32,54,208,100,164,132,224,154,212,122,150,204,144,48,32))
}

New-Icon 'toolbar/rotate.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawArc($pen, 44,44,168,168,34,282)
    $g.FillPolygon($brush, (Points 167,34,224,46,191,95))
}

New-Icon 'toolbar/fit.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    foreach ($segment in @(@(38,92,38,38,92,38), @(164,38,218,38,218,92), @(38,164,38,218,92,218), @(164,218,218,218,218,164))) {
        $g.DrawLines($pen, (Points $segment))
    }
}

New-Icon 'toolbar/grid.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    foreach ($line in 48,104,160,216) {
        $g.DrawLine($pen, $line, 34, $line, 222)
        $g.DrawLine($pen, 34, $line, 222, $line)
    }
}

New-Icon 'toolbar/play.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawPolygon($pen, (Points 74,46,74,210,204,128))
}

New-Icon 'toolbar/export.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLine($pen, 128,32,128,164)
    $g.FillPolygon($brush, (Points 76,128,128,182,180,128))
    $g.DrawLine($pen, 48,210,208,210)
}

New-Icon 'toolbar/wire.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLines($pen, (Points 36,172,88,172,88,82,172,82,172,132,220,132))
    foreach ($point in @(@(36,172), @(88,82), @(172,132), @(220,132))) {
        $g.FillEllipse($brush, $point[0]-12, $point[1]-12, 24, 24)
    }
}

New-Icon 'toolbar/outline.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawRectangle($accentPen, 44,44,168,168)
    $g.DrawLine($pen, 80,82,176,82)
    $g.DrawLine($pen, 80,128,176,128)
    $g.DrawLine($pen, 80,174,176,174)
}

New-Icon 'toolbar/airwire.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $pen.DashStyle = [System.Drawing.Drawing2D.DashStyle]::Dash
    $g.DrawBezier($accentPen, 32,172,90,36,166,220,224,84)
    $pen.DashStyle = [System.Drawing.Drawing2D.DashStyle]::Solid
    $g.FillEllipse($brush, 20,160,24,24)
    $g.FillEllipse($brush, 212,72,24,24)
}

New-Icon 'toolbar/layers.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawPolygon($pen, (Points 42,82,128,38,214,82,128,126))
    $g.DrawLines($accentPen, (Points 42,126,128,170,214,126))
    $g.DrawLines($pen, (Points 42,170,128,214,214,170))
}

New-Icon 'toolbar/pan.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLine($pen, 128,32,128,224)
    $g.DrawLine($pen, 32,128,224,128)
    $g.FillPolygon($accentBrush, (Points 104,52,128,28,152,52))
    $g.FillPolygon($accentBrush, (Points 104,204,128,228,152,204))
    $g.FillPolygon($accentBrush, (Points 52,104,28,128,52,152))
    $g.FillPolygon($accentBrush, (Points 204,104,228,128,204,152))
}

New-Icon 'toolbar/zoom-in.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawEllipse($pen, 38,38,128,128)
    $g.DrawLine($pen, 140,140,218,218)
    $g.DrawLine($accentPen, 102,66,102,138)
    $g.DrawLine($accentPen, 66,102,138,102)
}

New-Icon 'toolbar/zoom-out.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawEllipse($pen, 38,38,128,128)
    $g.DrawLine($pen, 140,140,218,218)
    $g.DrawLine($accentPen, 66,102,138,102)
}

New-Icon 'toolbar/delete.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawRectangle($pen, 67,76,122,132)
    $g.DrawLine($pen, 48,60,208,60)
    $g.DrawLine($pen, 98,40,158,40)
    $g.DrawLine($pen, 99,108,99,180)
    $g.DrawLine($pen, 128,108,128,180)
    $g.DrawLine($pen, 157,108,157,180)
}

New-Icon 'navigator/component.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawRectangle($pen, 66,66,124,124)
    foreach ($offset in 78,112,146) {
        $g.DrawLine($pen, $offset,42,$offset,66)
        $g.DrawLine($pen, $offset,190,$offset,214)
        $g.DrawLine($pen,42,$offset,66,$offset)
        $g.DrawLine($pen,190,$offset,214,$offset)
    }
}

New-Icon 'navigator/net.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLines($accentPen, (Points 48,170,104,82,170,142,210,54))
    foreach ($point in @(@(48,170), @(104,82), @(170,142), @(210,54))) {
        $g.FillEllipse($brush, $point[0]-14, $point[1]-14, 28, 28)
    }
}

New-Icon 'navigator/sheet.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawRectangle($pen, 58,34,132,188)
    $g.DrawLines($accentPen, (Points 150,34,190,74,190,222,96,222,58,188))
    $g.DrawLines($accentPen, (Points 150,34,150,74,190,74))
}

New-Icon 'navigator/filter.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLines($accentPen, (Points 36,50,220,50,148,128,148,202,108,222,108,128,36,50))
}

New-Icon 'navigator/inspector.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    Draw-RoundedRect $g $pen 44 36 168 184 16
    $g.DrawLine($accentPen, 76,86,180,86)
    $g.DrawLine($accentPen, 76,128,180,128)
    $g.DrawLine($accentPen, 76,170,146,170)
}

New-Icon 'navigator/console.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    Draw-RoundedRect $g $pen 32 52 192 152 16
    $g.DrawLines($accentPen, (Points 72,96,104,128,72,160))
    $g.DrawLine($accentPen, 124,164,178,164)
}

New-Icon 'navigator/drc.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawEllipse($pen, 40,40,176,176)
    $g.DrawLines($accentPen, (Points 76,132,112,168,184,86))
}

New-Icon 'navigator/simulation.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLine($pen, 32,128,224,128)
    $g.DrawLines($accentPen, (Points 34,128,70,128,94,56,124,198,154,88,182,128,222,128))
}

New-Icon 'navigator/agent.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawEllipse($pen, 46,46,164,164)
    $g.DrawEllipse($accentPen, 94,94,68,68)
    foreach ($point in @(@(62,128), @(194,128), @(128,62), @(128,194))) {
        $g.FillEllipse($brush, $point[0]-10, $point[1]-10, 20, 20)
    }
}

New-Icon 'library/resistor.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLines($accentPen, (Points 28,128,58,128,78,82,104,174,130,82,156,174,178,128,228,128))
}

New-Icon 'library/capacitor.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLine($pen, 28,128,94,128)
    $g.DrawLine($accentPen, 102,64,102,192)
    $g.DrawLine($accentPen, 154,64,154,192)
    $g.DrawLine($pen, 162,128,228,128)
}

New-Icon 'library/led.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLine($pen, 28,128,84,128)
    $g.DrawPolygon($accentBrush, (Points 84,68,84,188,164,128))
    $g.DrawLine($pen, 170,62,170,194)
    $g.DrawLine($pen, 170,128,228,128)
    $g.DrawLine($accentPen, 158,74,202,30)
    $g.DrawLine($accentPen, 174,102,218,58)
}

New-Icon 'library/magnet.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawArc($accentPen, 56,54,144,148,160,220)
    $g.DrawLine($pen, 70,178,70,216)
    $g.DrawLine($pen, 188,178,188,216)
    $g.DrawLine($accentPen, 56,216,84,216)
    $g.DrawLine($accentPen, 174,216,202,216)
}

New-Icon 'library/battery.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLine($pen, 28,128,86,128)
    $g.DrawLine($accentPen, 96,52,96,204)
    $g.DrawLine($pen, 148,78,148,178)
    $g.DrawLine($pen, 158,128,228,128)
}

New-Icon 'library/ground.png' {
    param($g, $pen, $accentPen, $brush, $accentBrush)
    $g.DrawLine($pen, 128,28,128,104)
    $g.DrawLine($accentPen, 58,112,198,112)
    $g.DrawLine($accentPen, 78,146,178,146)
    $g.DrawLine($accentPen, 100,180,156,180)
}

Copy-Item (Join-Path $OutputRoot 'library\resistor.png') (Join-Path $OutputRoot 'symbols\resistor.png') -Force
Copy-Item (Join-Path $OutputRoot 'library\capacitor.png') (Join-Path $OutputRoot 'symbols\capacitor.png') -Force
Copy-Item (Join-Path $OutputRoot 'library\led.png') (Join-Path $OutputRoot 'symbols\led.png') -Force
Copy-Item (Join-Path $OutputRoot 'library\magnet.png') (Join-Path $OutputRoot 'symbols\magnet.png') -Force
Copy-Item (Join-Path $OutputRoot 'library\battery.png') (Join-Path $OutputRoot 'symbols\battery.png') -Force
Copy-Item (Join-Path $OutputRoot 'library\ground.png') (Join-Path $OutputRoot 'symbols\ground.png') -Force
Copy-Item (Join-Path $OutputRoot 'navigator\component.png') (Join-Path $OutputRoot 'symbols\generic.png') -Force
Copy-Item (Join-Path $OutputRoot 'navigator\component.png') (Join-Path $OutputRoot 'library\generic.png') -Force
Copy-Item (Join-Path $OutputRoot 'navigator\component.png') (Join-Path $OutputRoot 'footprints\generic.png') -Force
Copy-Item (Join-Path $OutputRoot 'navigator\component.png') (Join-Path $OutputRoot 'toolbar\library.png') -Force

Write-Output "Generated high-resolution Electronics icons in $OutputRoot"
