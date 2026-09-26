# Capture the app window with PrintWindow(PW_RENDERFULLCONTENT=2), which does
# capture GPU-composited WebView2 content (unlike GDI CopyFromScreen, which
# returns black). Records what the user actually sees during startup.
# ASCII-only: Windows PowerShell decodes .ps1 as ANSI when there is no BOM.
param(
  [string]$Exe = "D:\Zcode\BILIdown\target\release\bilidown.exe",
  [int]$Seconds = 8,
  [int]$LaunchAtMs = 600,
  [int]$EveryMs = 40,
  [string]$OutDir = "D:\Zcode\BILIdown\tools\startshots"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Get-ChildItem -Path $OutDir -Filter *.png -ErrorAction SilentlyContinue | Remove-Item -Force

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Cap {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }

  public static string Cls(IntPtr h) { var sb = new StringBuilder(256); GetClassNameW(h, sb, 256); return sb.ToString(); }

  public static IntPtr FindMain(uint target) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid == target && Cls(h).StartsWith("Tauri Window")) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }

  // List every visible top-level window of the process (to spot a second window)
  public static string VisibleList(uint target) {
    var sb = new StringBuilder();
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid == target && IsWindowVisible(h)) {
        RECT r; GetWindowRect(h, out r);
        sb.Append("[").Append(Cls(h)).Append(" ").Append(r.R - r.L).Append("x").Append(r.B - r.T).Append("]");
      }
      return true;
    }, IntPtr.Zero);
    return sb.ToString();
  }
}
"@

$sw = [Diagnostics.Stopwatch]::StartNew()
$launched = 0
$nextShot = 0
$n = 0
$log = @()
$prevVis = ""

while ($sw.Elapsed.TotalMilliseconds -lt $Seconds * 1000) {
  $t = [int]$sw.Elapsed.TotalMilliseconds

  if ($launched -eq 0 -and $t -ge $LaunchAtMs) {
    $p = Start-Process -FilePath $Exe -PassThru
    $launched = $p.Id
    $log += "$t LAUNCH pid=$launched"
  }

  if ($launched -ne 0 -and $t -ge $nextShot) {
    $nextShot = $t + $EveryMs
    $h = [Cap]::FindMain([uint32]$launched)
    if ($h -ne [IntPtr]::Zero) {
      $vis = [Cap]::IsWindowVisible($h)
      if ($vis -ne $prevVis) {
        $prevVis = $vis
        $log += "$t visible=$vis windows=$([Cap]::VisibleList([uint32]$launched))"
      }
      if ($vis) {
        $r = New-Object Cap+RECT
        [void][Cap]::GetWindowRect($h, [ref]$r)
        $w = $r.R - $r.L; $ht = $r.B - $r.T
        if ($w -gt 50 -and $ht -gt 50) {
          $bmp = New-Object System.Drawing.Bitmap $w, $ht
          $g = [System.Drawing.Graphics]::FromImage($bmp)
          $hdc = $g.GetHdc()
          $ok = [Cap]::PrintWindow($h, $hdc, 2)
          $g.ReleaseHdc($hdc)
          $g.Dispose()
          $file = Join-Path $OutDir ("f{0:d3}_t{1:d5}.png" -f $n, $t)
          $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
          $bmp.Dispose()
          $n += 1
        }
      }
    }
  }
  Start-Sleep -Milliseconds 8
}

$log | ForEach-Object { $_ }
Write-Output "captured=$n frames into $OutDir"
