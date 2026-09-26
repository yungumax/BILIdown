# Resize the app's main window from outside, so the check does not depend on the
# app's own window API (and its ACL).
# ASCII-only on purpose.
param([int]$Width = 2000, [int]$Height = 1200)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win {
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int ht, bool repaint);
}
"@
$p = Get-Process bilidown -ErrorAction Stop | Select-Object -First 1
$h = $p.MainWindowHandle
if ($h -eq 0) { Write-Output "no window"; exit 1 }
[void][Win]::MoveWindow($h, 40, 40, $Width, $Height, $true)
Write-Output "resized to $Width x $Height"
