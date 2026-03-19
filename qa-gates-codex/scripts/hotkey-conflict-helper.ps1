param(
  [ValidateSet("Alt+Space", "Ctrl+Shift+R")]
  [string]$Hotkey = "Alt+Space",
  [string]$ReadyFile = "",
  [string]$LogFile = ""
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class HotkeyNative {
  [StructLayout(LayoutKind.Sequential)]
  public struct POINT {
    public int x;
    public int y;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct MSG {
    public IntPtr hwnd;
    public uint message;
    public UIntPtr wParam;
    public IntPtr lParam;
    public uint time;
    public POINT pt;
  }

  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool UnregisterHotKey(IntPtr hWnd, int id);

  [DllImport("user32.dll")]
  public static extern sbyte GetMessage(out MSG lpMsg, IntPtr hWnd, uint wMsgFilterMin, uint wMsgFilterMax);

  [DllImport("user32.dll")]
  public static extern bool TranslateMessage([In] ref MSG lpMsg);

  [DllImport("user32.dll")]
  public static extern IntPtr DispatchMessage([In] ref MSG lpMsg);
}
"@

$hotkeyId = 1
$wmHotkey = 0x0312
$modNoRepeat = 0x4000
$isRegistered = $false

switch ($Hotkey) {
  "Alt+Space" {
    $modifiers = 0x0001
    $virtualKey = 0x20
  }
  "Ctrl+Shift+R" {
    $modifiers = 0x0002 -bor 0x0004
    $virtualKey = [uint32][byte][char]'R'
  }
  default {
    throw "unsupported hotkey: $Hotkey"
  }
}

function Write-HelperLog {
  param([string]$Message)

  $line = "[{0}] {1}" -f (Get-Date -Format "o"), $Message
  Write-Output $line
  if (-not [string]::IsNullOrWhiteSpace($LogFile)) {
    Add-Content -Path $LogFile -Value $line -Encoding ascii
  }
}

try {
  if (-not [string]::IsNullOrWhiteSpace($LogFile)) {
    $logDir = Split-Path -Parent $LogFile
    if ($logDir -and -not (Test-Path $logDir)) {
      New-Item -ItemType Directory -Path $logDir | Out-Null
    }
    if (Test-Path $LogFile) {
      Remove-Item $LogFile -Force
    }
  }

  if (-not [HotkeyNative]::RegisterHotKey([IntPtr]::Zero, $hotkeyId, ($modifiers -bor $modNoRepeat), $virtualKey)) {
    $errorCode = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    throw "failed to register $Hotkey conflict helper (GetLastError=$errorCode)"
  }

  $isRegistered = $true
  Write-HelperLog "registered $Hotkey conflict helper"

  if (-not [string]::IsNullOrWhiteSpace($ReadyFile)) {
    $readyDir = Split-Path -Parent $ReadyFile
    if ($readyDir -and -not (Test-Path $readyDir)) {
      New-Item -ItemType Directory -Path $readyDir | Out-Null
    }
    Set-Content -Path $ReadyFile -Value "ready" -Encoding ascii
  }

  $msg = New-Object HotkeyNative+MSG
  while ([HotkeyNative]::GetMessage([ref]$msg, [IntPtr]::Zero, 0, 0) -gt 0) {
    if ($msg.message -eq $wmHotkey) {
      Write-HelperLog "WM_HOTKEY received for $Hotkey"
    }

    [HotkeyNative]::TranslateMessage([ref]$msg) | Out-Null
    [HotkeyNative]::DispatchMessage([ref]$msg) | Out-Null
  }
}
finally {
  if ($isRegistered) {
    [HotkeyNative]::UnregisterHotKey([IntPtr]::Zero, $hotkeyId) | Out-Null
    Write-HelperLog "released $Hotkey conflict helper"
  }
}
