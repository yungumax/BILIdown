# Event-driven window watcher: catches anything that appears and disappears too
# fast for polling. SetWinEventHook reports CREATE/SHOW/HIDE/DESTROY/FOREGROUND
# for every top-level window the moment it happens.
# ASCII-only on purpose (PowerShell decodes .ps1 as ANSI without a BOM).
param(
  [string]$Exe = "D:\Zcode\BILIdown\target\release\bilidown.exe",
  [int]$Seconds = 7,
  [int]$LaunchAtMs = 700
)

$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
using System.Collections.Generic;

public class WinHook {
  public delegate void Cb(IntPtr hHook, uint ev, IntPtr hwnd, int idObject, int idChild, uint tid, uint time);
  [DllImport("user32.dll")] static extern IntPtr SetWinEventHook(uint a, uint b, IntPtr mod, Cb cb, uint pid, uint tid, uint flags);
  [DllImport("user32.dll")] static extern bool UnhookWinEvent(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern bool PeekMessage(out MSG m, IntPtr h, uint a, uint b, uint flags);
  [DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG m);
  [DllImport("user32.dll")] static extern IntPtr DispatchMessage(ref MSG m);

  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct MSG { public IntPtr hwnd; public uint msg; public IntPtr wParam; public IntPtr lParam; public uint time; public int x; public int y; }

  public static List<string> Log = new List<string>();
  static long _t0 = 0;
  static Cb _cb;
  static IntPtr _hook = IntPtr.Zero;

  public static string Text(IntPtr h) { var sb = new StringBuilder(512); GetWindowTextW(h, sb, 512); return sb.ToString(); }
  public static string Cls(IntPtr h) { var sb = new StringBuilder(256); GetClassNameW(h, sb, 256); return sb.ToString(); }

  public static void Start() {
    Log.Clear();
    _t0 = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
    _cb = new Cb(On);
    // WINEVENT_OUTOFCONTEXT(0) | WINEVENT_SKIPOWNPROCESS(2) = 2
    _hook = SetWinEventHook(0x0003, 0x800B, IntPtr.Zero, _cb, 0, 0, 2);
  }

  public static void Stop() {
    if (_hook != IntPtr.Zero) { UnhookWinEvent(_hook); _hook = IntPtr.Zero; }
  }

  public static void Pump() {
    MSG m;
    int guard = 0;
    while (PeekMessage(out m, IntPtr.Zero, 0, 0, 1) && guard < 5000) {
      TranslateMessage(ref m);
      DispatchMessage(ref m);
      guard += 1;
    }
  }

  static void On(IntPtr hHook, uint ev, IntPtr hwnd, int idObject, int idChild, uint tid, uint time) {
    if (idObject != 0) return;                     // OBJID_WINDOW only
    if (hwnd == IntPtr.Zero) return;
    if (!IsWindow(hwnd)) return;
    if (GetParent(hwnd) != IntPtr.Zero) return;    // top-level only
    uint pid; GetWindowThreadProcessId(hwnd, out pid);
    RECT r; GetWindowRect(hwnd, out r);
    long now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() - _t0;
    Log.Add(now + " " + EvName(ev) + " pid=" + pid + " hwnd=" + hwnd +
            " vis=" + (IsWindowVisible(hwnd) ? 1 : 0) +
            " rect=" + r.L + "," + r.T + " " + (r.R - r.L) + "x" + (r.B - r.T) +
            " cls=" + Cls(hwnd) + " title='" + Text(hwnd) + "'");
  }

  static string EvName(uint ev) {
    switch (ev) {
      case 0x0003: return "FOREGROUND";
      case 0x8000: return "CREATE    ";
      case 0x8001: return "DESTROY   ";
      case 0x8002: return "SHOW      ";
      case 0x8003: return "HIDE      ";
      default: return "EV" + ev.ToString("X");
    }
  }
}
"@

[WinHook]::Start()
$sw = [Diagnostics.Stopwatch]::StartNew()
$launched = 0

while ($sw.Elapsed.TotalMilliseconds -lt $Seconds * 1000) {
  if ($launched -eq 0 -and $sw.Elapsed.TotalMilliseconds -ge $LaunchAtMs) {
    $p = Start-Process -FilePath $Exe -PassThru
    $launched = $p.Id
    Write-Output "== LAUNCH pid=$launched at $([int]$sw.Elapsed.TotalMilliseconds)ms =="
  }
  [WinHook]::Pump()
  Start-Sleep -Milliseconds 5
}

[WinHook]::Stop()

Write-Output "== events for launched pid $launched =="
foreach ($line in [WinHook]::Log) {
  if ($line -match "pid=$launched ") { Write-Output $line }
}
Write-Output "== events mentioning bilidown / tauri / webview =="
foreach ($line in [WinHook]::Log) {
  if ($line -match "(?i)bilidown|tauri|webview") { Write-Output $line }
}
Write-Output "== all SHOW/DESTROY of sizable top-level windows during startup =="
foreach ($line in [WinHook]::Log) {
  if ($line -match "SHOW|DESTROY|HIDE") { Write-Output $line }
}
