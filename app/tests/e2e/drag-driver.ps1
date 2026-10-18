# Sends the mouse/keyboard sequence for drag-e2e.ps1 (runs in its own process so the
# drag source's UI thread can sit in DoDragDrop's modal loop).
param([int]$sx, [int]$sy, [int]$cx, [int]$cy, [int]$vk, [int]$segment)
Add-Type -Namespace Dpi -Name Native -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);'
[void][Dpi.Native]::SetProcessDpiAwarenessContext([IntPtr](-4))   # per-monitor v2: physical pixels everywhere
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class In {
  [StructLayout(LayoutKind.Sequential)] public struct MI { public int dx, dy, data, flags, time; public IntPtr extra; }
  [StructLayout(LayoutKind.Sequential)] public struct KI { public short vk, scan; public int flags, time; public IntPtr extra; }
  [StructLayout(LayoutKind.Explicit)] public struct INPUT { [FieldOffset(0)] public int type; [FieldOffset(8)] public MI mi; [FieldOffset(8)] public KI ki; }
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] i, int size);
  public static void Move(int x, int y) {
    var i = new INPUT[1]; i[0].type = 0; i[0].mi.flags = 0x0001 | 0x8000 | 0x4000;
    i[0].mi.dx = (int)(x * 65535.0 / (GetSystemMetrics(78) - 1) + 0) ; i[0].mi.dy = (int)(y * 65535.0 / (GetSystemMetrics(79) - 1));
    SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
  }
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  public static void Mouse(int flags) { var i = new INPUT[1]; i[0].type = 0; i[0].mi.flags = flags; SendInput(1, i, Marshal.SizeOf(typeof(INPUT))); }
  public static void Key(short vk, bool up) { var i = new INPUT[1]; i[0].type = 1; i[0].ki.vk = vk; i[0].ki.flags = up ? 2 : 0; SendInput(1, i, Marshal.SizeOf(typeof(INPUT))); }
}
"@
Start-Sleep -Milliseconds 500
[In]::Move($sx, $sy); Start-Sleep -Milliseconds 150
[In]::Mouse(0x0002)                                    # left down -> DoDragDrop starts
Start-Sleep -Milliseconds 100
for ($i = 1; $i -le 25; $i++) { [In]::Move([int]($sx + ($cx - $sx) * $i / 25), [int]($sy + ($cy - $sy) * $i / 25)); Start-Sleep -Milliseconds 15 }
Start-Sleep -Milliseconds 150
[In]::Key($vk, $false); Start-Sleep -Milliseconds 60; [In]::Key($vk, $true)     # tap
Start-Sleep -Milliseconds 400
[In]::Move($cx + 3, $cy + 3); Start-Sleep -Milliseconds 150                    # DragEnter on the ring
if ($env:MORPH_E2E_SHOT) {
  Add-Type -AssemblyName System.Windows.Forms, System.Drawing
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save($env:MORPH_E2E_SHOT)
}
[In]::Move($cx + 6, $cy); Start-Sleep -Milliseconds 400
$d = 0x30 + $segment; [In]::Key($d, $false); Start-Sleep -Milliseconds 40; [In]::Key($d, $true)
Start-Sleep -Milliseconds 250
[In]::Move($cx + 8, $cy + 2); Start-Sleep -Milliseconds 200
[In]::Mouse(0x0004)                                    # left up -> drop
