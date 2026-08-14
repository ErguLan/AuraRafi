param(
    [string]$OutputRoot = (Join-Path $PSScriptRoot '..\..\editor\assets\ui_icons\top')
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$size = 256
$foreground = [System.Drawing.Color]::FromArgb(255, 236, 240, 244)
$muted = [System.Drawing.Color]::FromArgb(255, 166, 174, 184)
$accent = [System.Drawing.Color]::FromArgb(255, 232, 133, 28)

function New-Icon {
    param([string]$Name, [scriptblock]$Draw)

    [System.IO.Directory]::CreateDirectory($OutputRoot) | Out-Null
    $path = Join-Path $OutputRoot $Name
    $bitmap = [System.Drawing.Bitmap]::new($size, $size)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.Clear([System.Drawing.Color]::Transparent)
    $pen = [System.Drawing.Pen]::new($foreground, 14)
    $mutedPen = [System.Drawing.Pen]::new($muted, 11)
    $accentPen = [System.Drawing.Pen]::new($accent, 14)
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $mutedPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $mutedPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $accentPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $accentPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    try {
        & $Draw $graphics $pen $mutedPen $accentPen
        $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $accentPen.Dispose()
        $mutedPen.Dispose()
        $pen.Dispose()
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

New-Icon 'file.png' {
    param($g, $pen, $mutedPen, $accentPen)
    $g.DrawRectangle($pen, 56, 38, 144, 184)
    $g.DrawLine($accentPen, 96, 38, 96, 86)
    $g.DrawLine($accentPen, 96, 86, 146, 86)
    $g.DrawLine($mutedPen, 86, 132, 174, 132)
    $g.DrawLine($mutedPen, 86, 174, 174, 174)
}

New-Icon 'edit.png' {
    param($g, $pen, $mutedPen, $accentPen)
    $g.DrawLine($pen, 62, 194, 188, 68)
    $g.DrawLine($accentPen, 170, 50, 206, 86)
    $g.DrawLine($mutedPen, 48, 208, 88, 198)
}

New-Icon 'view.png' {
    param($g, $pen, $mutedPen, $accentPen)
    $g.DrawEllipse($pen, 38, 82, 180, 92)
    $g.FillEllipse([System.Drawing.SolidBrush]::new($accent), 106, 116, 44, 44)
}

New-Icon 'project.png' {
    param($g, $pen, $mutedPen, $accentPen)
    $g.DrawRectangle($pen, 38, 72, 180, 142)
    $g.DrawLine($accentPen, 38, 112, 218, 112)
    $g.DrawLine($mutedPen, 70, 154, 180, 154)
}

New-Icon 'help.png' {
    param($g, $pen, $mutedPen, $accentPen)
    $g.DrawEllipse($pen, 42, 42, 172, 172)
    $g.DrawArc($accentPen, 94, 72, 68, 74, 205, 230)
    $g.FillEllipse([System.Drawing.SolidBrush]::new($foreground), 122, 168, 12, 12)
}

New-Icon 'save.png' {
    param($g, $pen, $mutedPen, $accentPen)
    $g.DrawRectangle($pen, 46, 36, 164, 184)
    $g.DrawRectangle($accentPen, 86, 42, 84, 58)
    $g.DrawRectangle($mutedPen, 86, 142, 84, 56)
}

New-Icon 'minimize.png' {
    param($g, $pen, $mutedPen, $accentPen)
    $g.DrawLine($pen, 50, 184, 206, 184)
}

New-Icon 'maximize.png' {
    param($g, $pen, $mutedPen, $accentPen)
    $g.DrawRectangle($pen, 48, 48, 160, 160)
    $g.DrawLine($accentPen, 76, 76, 180, 76)
}

New-Icon 'close.png' {
    param($g, $pen, $mutedPen, $accentPen)
    $g.DrawLine($pen, 62, 62, 194, 194)
    $g.DrawLine($pen, 194, 62, 62, 194)
}
