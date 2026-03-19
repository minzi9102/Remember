param(
  [string]$HelperScript = "",
  [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

if ([string]::IsNullOrWhiteSpace($HelperScript)) {
  $HelperScript = Join-Path $PSScriptRoot "hotkey-conflict-helper.ps1"
}

if (-not (Test-Path $HelperScript)) {
  throw "hotkey helper script missing: $HelperScript"
}

Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class Win32DiagnosticNative {
  [StructLayout(LayoutKind.Sequential)]
  public struct MOUSEINPUT {
    public int dx;
    public int dy;
    public uint mouseData;
    public uint dwFlags;
    public uint time;
    public IntPtr dwExtraInfo;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct INPUT {
    public int type;
    public InputUnion U;
  }

  [StructLayout(LayoutKind.Explicit)]
  public struct InputUnion {
    [FieldOffset(0)]
    public KEYBDINPUT ki;

    [FieldOffset(0)]
    public MOUSEINPUT mi;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct KEYBDINPUT {
    public ushort wVk;
    public ushort wScan;
    public uint dwFlags;
    public uint time;
    public IntPtr dwExtraInfo;
  }

  [DllImport("user32.dll", SetLastError = true)]
  public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);
}
"@

$inputTypeKeyboard = [uint32]1
$keyEventKeyUp = [uint32]0x0002

function New-KeyInputRecord {
  param(
    [uint16]$VirtualKey,
    [uint32]$Flags = 0
  )

  $input = New-Object Win32DiagnosticNative+INPUT
  $input.type = $inputTypeKeyboard
  $input.U.ki.wVk = $VirtualKey
  $input.U.ki.time = 0
  $input.U.ki.dwExtraInfo = [IntPtr]::Zero
  $input.U.ki.dwFlags = $Flags
  return $input
}

function Invoke-KeyChord {
  param([uint16[]]$Keys)

  $records = [System.Collections.Generic.List[object]]::new()
  foreach ($key in $Keys) {
    $records.Add((New-KeyInputRecord -VirtualKey $key))
  }
  for ($index = $Keys.Count - 1; $index -ge 0; $index--) {
    $records.Add((New-KeyInputRecord -VirtualKey $Keys[$index] -Flags $keyEventKeyUp))
  }

  $inputArray = New-Object "Win32DiagnosticNative+INPUT[]" $records.Count
  for ($index = 0; $index -lt $records.Count; $index++) {
    $inputArray[$index] = $records[$index]
  }

  $inputSize = [System.Runtime.InteropServices.Marshal]::SizeOf([type]"Win32DiagnosticNative+INPUT")
  $sent = [Win32DiagnosticNative]::SendInput([uint32]$inputArray.Length, $inputArray, $inputSize)
  $lastError = if ($sent -ne $inputArray.Length) {
    [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
  } else {
    0
  }

  return [pscustomobject]@{
    Sent = [int]$sent
    Expected = [int]$inputArray.Length
    LastError = [int]$lastError
    Ok = ($sent -eq $inputArray.Length)
  }
}

function Wait-ForPath {
  param(
    [string]$Path,
    [int]$Attempts = 40,
    [int]$DelayMs = 250
  )

  for ($index = 0; $index -lt $Attempts; $index++) {
    if (Test-Path $Path) {
      return
    }
    Start-Sleep -Milliseconds $DelayMs
  }

  throw "timed out waiting for $Path"
}

function Stop-ProcessSafe {
  param($Process)

  if ($null -eq $Process) {
    return
  }

  try {
    if (-not $Process.HasExited) {
      Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
    }
  } catch {
  }
}

function Invoke-Probe {
  param(
    [string]$Hotkey,
    [uint16[]]$Keys,
    [string]$Workspace
  )

  $probeDir = Join-Path $Workspace ($Hotkey -replace '[^A-Za-z0-9]+', '-')
  New-Item -ItemType Directory -Path $probeDir -Force | Out-Null
  $readyFile = Join-Path $probeDir "ready.txt"
  $logFile = Join-Path $probeDir "helper.log"
  $helper = $null
  $notepad = $null
  $received = $false
  $logLines = @()
  $sendResult = $null

  try {
    $helper = Start-Process `
      -FilePath "powershell.exe" `
      -ArgumentList @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $HelperScript,
        "-Hotkey", $Hotkey,
        "-ReadyFile", $readyFile,
        "-LogFile", $logFile
      ) `
      -PassThru `
      -WindowStyle Hidden

    Wait-ForPath -Path $readyFile
    $notepad = Start-Process -FilePath "notepad.exe" -PassThru
    Start-Sleep -Milliseconds 750
    $sendResult = Invoke-KeyChord -Keys $Keys
    Start-Sleep -Seconds 2

    if (Test-Path $logFile) {
      $logLines = @([string[]](Get-Content -Path $logFile))
      $received = $logLines -match "WM_HOTKEY received" | Measure-Object | Select-Object -ExpandProperty Count
      $received = [bool]($received -gt 0)
    }
  } finally {
    Stop-ProcessSafe -Process $notepad
    Stop-ProcessSafe -Process $helper
    Start-Sleep -Milliseconds 250
    if (Test-Path $logFile) {
      $logLines = @([string[]](Get-Content -Path $logFile))
    }
  }

  return [pscustomobject]@{
    hotkey = $Hotkey
    triggered = $received
    sendInputOk = if ($null -ne $sendResult) { $sendResult.Ok } else { $false }
    sendInputSent = if ($null -ne $sendResult) { $sendResult.Sent } else { 0 }
    sendInputExpected = if ($null -ne $sendResult) { $sendResult.Expected } else { 0 }
    sendInputLastError = if ($null -ne $sendResult) { $sendResult.LastError } else { -1 }
    logPath = $logFile
    logLines = $logLines
  }
}

$workspace = Join-Path $env:TEMP ("p4t1-hotkey-diag-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $workspace -Force | Out-Null

$probes = @(
  @{ Hotkey = "Alt+Space"; Keys = @([uint16]0x12, [uint16]0x20) },
  @{ Hotkey = "Ctrl+Shift+R"; Keys = @([uint16]0x11, [uint16]0x10, [uint16][byte][char]'R') }
)

$results = foreach ($probe in $probes) {
  Invoke-Probe -Hotkey $probe.Hotkey -Keys $probe.Keys -Workspace $workspace
}

$result = [pscustomobject]@{
  summary = if (($results | Where-Object { $_.triggered }).Count -gt 0) {
    "injected SendInput probes showed environment-specific WM_HOTKEY behavior, so they are not authoritative proof of real physical keyboard hotkeys"
  } else {
    "injected SendInput probes did not trigger RegisterHotKey WM_HOTKEY here, so physical keyboard verification is still required"
  }
  physicalKeyboardRequired = $true
  probes = $results
}

if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
  $lines = @(
    "summary: $($result.summary)",
    "physical_keyboard_required: true",
    "helper_script: $HelperScript",
    "workspace: $workspace",
    ""
  )

  foreach ($probe in $result.probes) {
    $lines += "probe: $($probe.hotkey)"
    $lines += "- triggered: $($probe.triggered)"
    $lines += "- send_input_ok: $($probe.sendInputOk)"
    $lines += "- send_input_sent: $($probe.sendInputSent)"
    $lines += "- send_input_expected: $($probe.sendInputExpected)"
    $lines += "- send_input_last_error: $($probe.sendInputLastError)"
    $lines += "- helper_log: $($probe.logPath)"
    if ($probe.logLines.Count -gt 0) {
      foreach ($line in $probe.logLines) {
        $lines += "- log: $line"
      }
    } else {
      $lines += "- log: no helper log lines captured"
    }
    $lines += ""
  }

  Set-Content -Path $OutputPath -Value $lines -Encoding ascii
}

Write-Output ($result | ConvertTo-Json -Depth 8 -Compress)
