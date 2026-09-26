# Resize the app's main window from outside, so the check does not depend on the
# app's own window API (and its ACL).
#
# Pitfalls (both hit for real):
#   - Do NOT use MainWindowHandle: the process also hosts Tao's
#     "Tao Thread Event Target" window and it can be returned instead, so the
#     resize seems to work while the UI does not move.
#   - A minimized window sits at -32000,-32000 with size 162x28, so restore it
#     before moving/resizing.
#
# ASCII-only on purpose: Windows PowerShell decodes a BOM-less .ps1 as ANSI, and
# non-ASCII inside the C# here-string corrupts the following lines.
param([int]$Width = 2000, [int]$Height = 1200)

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Win {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int ht, bool repaint);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);

  const int SW_RESTORE = 9;
  public static string Cls(IntPtr h) { var sb = new StringBuilder(256); GetClassNameW(h, sb, 256); return sb.ToString(); }

  // The top-level window whose class starts with "Tauri Window" is the main one.
  public static string Find(uint target) {
    string found = "";
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid == target && Cls(h).StartsWith("Tauri Window")) { found = h.ToString(); return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }

  public static void RestoreAndResize(IntPtr h, int w, int ht) {
    ShowWindow(h, SW_RESTORE);
    MoveWindow(h, 40, 40, w, ht, true);
  }
}
"@

$p = Get-Process bilidown -ErrorAction Stop | Select-Object -First 1
$hStr = [Win]::Find([uint32]$p.Id)
if ($hStr -eq "") { Write-Output "main window not found"; exit 1 }
$h = [IntPtr][long]$hStr
[Win]::RestoreAndResize($h, $Width, $Height)
Write-Output "restored and resized to $Width x $Height"
