# Window-level trace at startup. Samples every 15ms.
# Purpose: determine whether the "flash" on double-click is the main window
# repainting, or a second instance's window appearing.
# NOTE: keep this file ASCII-only. Windows PowerShell reads .ps1 as ANSI when
# there is no BOM, which corrupts non-ASCII comments and breaks parsing.
param(
  [string]$Exe = "D:\Zcode\BILIdown\target\release\bilidown.exe",
  [int]$Seconds = 6,
  [int]$Launches = 1,
  [int]$LaunchGapMs = 80
)

$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class WinTrace {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();

  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }

  public static string Text(IntPtr h) { var sb = new StringBuilder(512); GetWindowTextW(h, sb, 512); return sb.ToString(); }
  public static string Cls(IntPtr h) { var sb = new StringBuilder(256); GetClassNameW(h, sb, 256); return sb.ToString(); }

  public static string DumpPid(uint target) {
    var sb = new StringBuilder();
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid == target) {
        RECT r; GetWindowRect(h, out r);
        sb.Append("[").Append(h).Append(" vis=").Append(IsWindowVisible(h) ? 1 : 0)
          .Append(" ").Append(r.L).Append(",").Append(r.T)
          .Append(" ").Append(r.R - r.L).Append("x").Append(r.B - r.T)
          .Append(" ").Append(Cls(h)).Append(" '").Append(Text(h)).Append("']");
      }
      return true;
    }, IntPtr.Zero);
    return sb.ToString();
  }

  public static string DumpWidgets() {
    var sb = new StringBuilder();
    EnumWindows((h, l) => {
      string c = Cls(h);
      if (c.StartsWith("Chrome_WidgetWin")) {
        uint pid; GetWindowThreadProcessId(h, out pid);
        RECT r; GetWindowRect(h, out r);
        sb.Append("{").Append(pid).Append(" ").Append(h)
          .Append(" vis=").Append(IsWindowVisible(h) ? 1 : 0)
          .Append(" ").Append(r.L).Append(",").Append(r.T)
          .Append(" ").Append(r.R - r.L).Append("x").Append(r.B - r.T)
          .Append(" '").Append(Text(h)).Append("'}");
      }
      return true;
    }, IntPtr.Zero);
    return sb.ToString();
  }

  public static uint FgPid() {
    uint pid; GetWindowThreadProcessId(GetForegroundWindow(), out pid); return pid;
  }
}
"@

function Name-Of($targetPid) {
  try { return (Get-Process -Id $targetPid -ErrorAction Stop).ProcessName } catch { return "?" }
}

$sw = [Diagnostics.Stopwatch]::StartNew()
$launched = @()
$events = New-Object System.Collections.Generic.List[string]
$prevWidgets = @{}
$prevByPid = @{}
$startDelay = 900

while ($sw.Elapsed.TotalMilliseconds -lt $Seconds * 1000) {
  $t = [int]$sw.Elapsed.TotalMilliseconds

  if ($t -ge $startDelay -and $launched.Count -lt $Launches) {
    $p = Start-Process -FilePath $Exe -PassThru
    $launched += $p.Id
    $events.Add("$t LAUNCH#$($launched.Count) pid=$($p.Id)")
    $startDelay += $LaunchGapMs
  }

  foreach ($lp in $launched) {
    $alive = $null -ne (Get-Process -Id $lp -ErrorAction SilentlyContinue)
    $dump = [WinTrace]::DumpPid([uint32]$lp)
    $key = "$lp|$dump|$alive"
    if ($prevByPid[$lp] -ne $key) {
      $prevByPid[$lp] = $key
      $winCount = 0
      if ($dump.Length -gt 0) { $winCount = ([regex]::Matches($dump, "\[")).Count }
      $events.Add("$t pid=$lp alive=$alive nwin=$winCount fg=$([WinTrace]::FgPid()) :: $dump")
    }
  }

  $widgets = [WinTrace]::DumpWidgets()
  $now = @{}
  foreach ($m in [regex]::Matches($widgets, "\{(\d+) (\d+) vis=(\d) (-?\d+),(-?\d+) (\d+)x(\d+) '([^']*)'\}")) {
    $h = $m.Groups[2].Value
    $now[$h] = $m.Value
    if (-not $prevWidgets.ContainsKey($h)) {
      $ownerPid = [int]$m.Groups[1].Value
      $events.Add("$t NEW-WIDGET hwnd=$h pid=$ownerPid proc=$(Name-Of $ownerPid) vis=$($m.Groups[3].Value) rect=$($m.Groups[4].Value),$($m.Groups[5].Value) $($m.Groups[6].Value)x$($m.Groups[7].Value) title='$($m.Groups[8].Value)'")
    }
  }
  foreach ($k in $prevWidgets.Keys) {
    if (-not $now.ContainsKey($k)) { $events.Add("$t GONE-WIDGET hwnd=$k prev=$($prevWidgets[$k])") }
  }
  $prevWidgets = $now

  Start-Sleep -Milliseconds 15
}

$events | ForEach-Object { $_ }
