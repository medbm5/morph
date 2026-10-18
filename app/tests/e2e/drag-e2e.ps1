# End-to-end ring test on Windows: a real OLE file drag, a real Shift tap, a real drop.
#
# Opens a small WinForms window that starts DoDragDrop(FileDrop) on mouse down
# (the same drag Explorer starts), then drives the mouse and keyboard with
# SendInput: press on the source, move, tap the trigger key, move onto a
# segment, release. Checks the expected output file appears and the source is
# unchanged.
#
#   powershell -ExecutionPolicy Bypass -File app\tests\e2e\drag-e2e.ps1 -Exe target\debug\metamorphosis.exe
#
# WARNING: this script TAKES OVER THE MOUSE AND KEYBOARD for ~10 seconds. Run it only on a
# dedicated test machine or VM, never on a desktop someone is using.
param(
  [switch]$TakeOverInput,
  [string]$Exe = "target\debug\metamorphosis.exe",
  [string]$Key = "Shift",        # Shift | Ctrl | Alt
  [int]$Segment = 1,             # 1-based segment to drop on (picked with the number key)
  [string]$Expect = "photo.png"  # file expected beside the source
)
$ErrorActionPreference = "Stop"
if (-not $TakeOverInput) {
  Write-Host "Refusing to run: this test drives the real mouse and keyboard. Pass -TakeOverInput on a dedicated test machine."
  exit 2
}
Add-Type -Namespace Dpi -Name Native -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);'
[void][Dpi.Native]::SetProcessDpiAwarenessContext([IntPtr](-4))   # per-monitor v2: physical pixels everywhere
Add-Type -AssemblyName System.Windows.Forms, System.Drawing

$work = Join-Path $env:TEMP ("morph-e2e-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory $work | Out-Null
$src = Join-Path $work "photo.jpg"
$bmp = New-Object System.Drawing.Bitmap 320, 200
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::FromArgb(255, 79, 227, 255)); $g.Dispose()
$bmp.Save($src, [System.Drawing.Imaging.ImageFormat]::Jpeg); $bmp.Dispose()
$hashBefore = (Get-FileHash $src).Hash

$app = Start-Process -FilePath $Exe -ArgumentList "--background" -PassThru
Start-Sleep -Seconds 4

# Drag source window.
$form = New-Object System.Windows.Forms.Form
$form.Text = "morph drag source"; $form.Width = 260; $form.Height = 160; $form.TopMost = $true
$form.StartPosition = "Manual"; $form.Left = 200; $form.Top = 200
$label = New-Object System.Windows.Forms.Label
$label.Text = "photo.jpg"; $label.Dock = "Fill"; $label.TextAlign = "MiddleCenter"
$label.Add_MouseDown({
  $data = New-Object System.Windows.Forms.DataObject
  $files = New-Object System.Collections.Specialized.StringCollection; [void]$files.Add($src)
  $data.SetFileDropList($files)
  [void]$label.DoDragDrop($data, [System.Windows.Forms.DragDropEffects]::Copy)
})
$form.Controls.Add($label)
$form.Show(); [System.Windows.Forms.Application]::DoEvents()

$vk = @{ Shift = 0x10; Ctrl = 0x11; Alt = 0x12 }[$Key]
$sx = $form.Left + 130; $sy = $form.Top + 90
$cx = 700; $cy = 450

$driverScript = Join-Path $PSScriptRoot "drag-driver.ps1"
Start-Process powershell -WindowStyle Hidden -ArgumentList @("-ExecutionPolicy", "Bypass", "-File", $driverScript, $sx, $sy, $cx, $cy, $vk, $Segment) | Out-Null

$deadline = (Get-Date).AddSeconds(10)
$out = Join-Path $work $Expect
while (-not (Test-Path $out) -and (Get-Date) -lt $deadline) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 50 }
Start-Sleep -Milliseconds 300
$form.Close()

$ok = $true
if (Test-Path $out) { Write-Host "OK  output saved: $out" } else { Write-Host "FAIL no $Expect in $work"; $ok = $false }
if ((Get-FileHash $src).Hash -eq $hashBefore) { Write-Host "OK  source unchanged" } else { Write-Host "FAIL source changed"; $ok = $false }
Get-ChildItem $work | ForEach-Object { Write-Host "    $($_.Name)  $($_.Length) B" }

Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
if (-not $ok) { exit 1 }
