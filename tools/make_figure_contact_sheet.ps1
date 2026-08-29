param(
    [Parameter(Mandatory = $true)]
    [string]$SourceDirectory,
    [Parameter(Mandatory = $true)]
    [string]$OutputPath,
    [int]$Columns = 4,
    [int]$CellWidth = 320,
    [int]$CellHeight = 230
)

Add-Type -AssemblyName System.Drawing
$files = @(Get-ChildItem -LiteralPath $SourceDirectory -Filter '*.png' | Sort-Object Name)
if ($files.Count -eq 0) { throw "No PNG figures found in $SourceDirectory" }

$rows = [Math]::Ceiling($files.Count / [double]$Columns)
$canvas = [System.Drawing.Bitmap]::new($Columns * $CellWidth, $rows * $CellHeight)
$graphics = [System.Drawing.Graphics]::FromImage($canvas)
$graphics.Clear([System.Drawing.Color]::FromArgb(20, 24, 32))
$graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$font = [System.Drawing.Font]::new('Segoe UI', 9)
$brush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(232, 236, 242))

for ($index = 0; $index -lt $files.Count; $index++) {
    $column = $index % $Columns
    $row = [Math]::Floor($index / $Columns)
    $x = $column * $CellWidth
    $y = $row * $CellHeight
    $image = [System.Drawing.Image]::FromFile($files[$index].FullName)
    $scale = [Math]::Min(($CellWidth - 8) / $image.Width, ($CellHeight - 26) / $image.Height)
    $width = [int]($image.Width * $scale)
    $height = [int]($image.Height * $scale)
    $graphics.DrawImage($image, $x + (($CellWidth - $width) / 2), $y + 20 + (($CellHeight - 26 - $height) / 2), $width, $height)
    $graphics.DrawString($files[$index].BaseName, $font, $brush, $x + 4, $y + 3)
    $image.Dispose()
}

$canvas.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)
$brush.Dispose()
$font.Dispose()
$graphics.Dispose()
$canvas.Dispose()
