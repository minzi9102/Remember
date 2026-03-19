param(
  [ValidateSet("ENV-SQLITE")]
  [string]$EnvId = "ENV-SQLITE",
  [string]$WebDriverUrl = "http://127.0.0.1:4723",
  [switch]$SkipStatusProbe,
  [string[]]$Cases = @(
    "P4-T1-VG-PASS",
    "P4-T1-IG-PASS",
    "P4-T1-VG-FAIL",
    "P4-T1-IG-FAIL"
  )
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class Win32Native {
  [StructLayout(LayoutKind.Sequential)]
  public struct INPUT {
    public int type;
    public InputUnion U;
  }

  [StructLayout(LayoutKind.Explicit)]
  public struct InputUnion {
    [FieldOffset(0)]
    public MOUSEINPUT mi;

    [FieldOffset(0)]
    public KEYBDINPUT ki;

    [FieldOffset(0)]
    public HARDWAREINPUT hi;
  }

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
  public struct KEYBDINPUT {
    public ushort wVk;
    public ushort wScan;
    public uint dwFlags;
    public uint time;
    public IntPtr dwExtraInfo;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct HARDWAREINPUT {
    public uint uMsg;
    public ushort wParamL;
    public ushort wParamH;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct RECT {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
  }

  [DllImport("user32.dll")]
  public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);

  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);

  [DllImport("user32.dll")]
  public static extern IntPtr GetForegroundWindow();

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowTextLength(IntPtr hWnd);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

  [DllImport("user32.dll", SetLastError = true)]
  public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);
}
"@

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$outputDir = Join-Path $root "qa-gates-codex"
$matrixPath = Join-Path $outputDir "MASTER-TRACE-MATRIX.md"
$appDataDir = Join-Path $env:APPDATA "com.remember.app"
$configPath = Join-Path $appDataDir "config.toml"
$sqlitePath = Join-Path $appDataDir "remember.sqlite3"
$pythonExe = Join-Path $root ".venv\Scripts\python.exe"
$ffmpegExe = (Get-Command ffmpeg -ErrorAction SilentlyContinue).Source
$screenshotScript = Join-Path $env:USERPROFILE ".codex\skills\screenshot\scripts\take_screenshot.ps1"
$exePath = Join-Path $root "src-tauri\target\debug\tauri-app.exe"
$helperScript = Join-Path $PSScriptRoot "hotkey-conflict-helper.ps1"
$hotkeyDiagnosticScript = Join-Path $PSScriptRoot "diagnose-hotkey-injection-limits.ps1"
$runDate = Get-Date -Format "yyyyMMdd"
$tester = "codex"
$vitePort = 1420
$viteUrl = "http://127.0.0.1:$vitePort"
$runtimeMode = "sqlite_only"
$appTitle = "tauri-app [$runtimeMode]"
$hotkeyDisabledTitle = "$appTitle [HOTKEY_DISABLED]"
$runtimeLogDir = Join-Path $env:TEMP ("p4t1-logs-" + [guid]::NewGuid().ToString())
$backupDir = Join-Path $env:TEMP ("p4t1-backup-" + [guid]::NewGuid().ToString())
$validCases = @(
  "P4-T1-VG-PASS",
  "P4-T1-IG-PASS",
  "P4-T1-VG-FAIL",
  "P4-T1-IG-FAIL"
)
$selectedCases = [System.Collections.Generic.List[string]]::new()
$caseResults = [ordered]@{}
$vkKeys = @{
  Enter = [uint16]0x0D
  Shift = [uint16]0x10
  Alt = [uint16]0x12
  Space = [uint16]0x20
  N = [uint16][byte][char]'N'
}
$script:KeyboardBackend = "win32-sendinput"
$script:InputTypeKeyboard = [uint32]1
$script:KeyEventKeyUp = [uint32]0x0002
$script:KeyEventUnicode = [uint32]0x0004
$script:WebDriverHealth = [pscustomobject]@{
  Reachable = $false
  StatusEndpointOk = $false
  RootSessionOk = $false
  AttachWindowOk = $false
  HealthMode = "not-run"
  StatusEndpointResult = "not-run"
  RootSessionProbeResult = "not-run"
  AttachWindowProbeResult = "not-run"
  FailureReason = "not-run"
}
$global:ViteProcess = $null

function New-Dir {
  param([string]$Path)

  if (-not (Test-Path $Path)) {
    New-Item -ItemType Directory -Path $Path | Out-Null
  }
}

function Fail-Assert {
  param([string]$Message)

  throw "ASSERT: $Message"
}

function Get-CaseEvidenceBase {
  param([string]$CaseId)

  return Join-Path $outputDir "${CaseId}_${runDate}_${EnvId}_${tester}"
}

function Write-CaseHeader {
  param(
    [string]$TxtPath,
    [string]$CaseId,
    [string]$TargetMode
  )

  Set-Content -Path $TxtPath -Value @(
    "case_id: $CaseId",
    "target_mode: $TargetMode",
    "env_id: $EnvId",
    "runtime_mode: $runtimeMode",
    "run_date: $runDate",
    "tester: $tester",
    "webdriver_health_mode: $($script:WebDriverHealth.HealthMode)",
    "webdriver_status_endpoint: $($script:WebDriverHealth.StatusEndpointResult)",
    "webdriver_root_session_probe: $($script:WebDriverHealth.RootSessionProbeResult)",
    "structure: environment -> steps -> actual_result -> log_excerpt -> sqlite_proof -> conclusion"
  ) -Encoding ascii
}

function Append-Text {
  param(
    [string]$TxtPath,
    [string]$Line
  )

  Add-Content -Path $TxtPath -Value $Line -Encoding ascii
}

function Set-CaseResult {
  param(
    [string]$CaseId,
    [string]$Result,
    [string]$Evidence
  )

  $caseResults[$CaseId] = [pscustomobject]@{
    CaseId = $CaseId
    Result = $Result
    Evidence = $Evidence
  }
}

function Get-OverallStatus {
  if ($caseResults.Count -eq 0) {
    return "BLOCKED"
  }

  $results = $caseResults.Values.Result
  if ($results -contains "BLOCKED") {
    return "BLOCKED"
  }
  if ($results -contains "FAIL") {
    return "FAIL"
  }
  return "PASS"
}

function Update-MatrixStatus {
  param([string]$Status)

  $content = [System.IO.File]::ReadAllText($matrixPath, [System.Text.Encoding]::UTF8)
  $targetLine = '| P4-T1 | `phase-4/p4-t1-global-hotkey.md` | 4 | 4 | {0} |' -f $Status
  $updated = $content.Replace(
    '| P4-T1 | `phase-4/p4-t1-global-hotkey.md` | 4 | 4 | TODO |',
    $targetLine
  ).Replace(
    '| P4-T1 | `phase-4/p4-t1-global-hotkey.md` | 4 | 4 | BLOCKED |',
    $targetLine
  ).Replace(
    '| P4-T1 | `phase-4/p4-t1-global-hotkey.md` | 4 | 4 | PASS |',
    $targetLine
  ).Replace(
    '| P4-T1 | `phase-4/p4-t1-global-hotkey.md` | 4 | 4 | FAIL |',
    $targetLine
  ).Replace(
    '| P4-T1 | `phase-4/p4-t1-global-hotkey.md` | 4 | 4 | RUNNING |',
    $targetLine
  )

  if (-not $updated.Contains($targetLine)) {
    throw "failed to update P4-T1 status in MASTER-TRACE-MATRIX.md"
  }

  $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($matrixPath, $updated, $utf8NoBom)
}

function Write-BlockedArtifacts {
  param(
    [string]$Reason,
    [string]$Phase = "precheck",
    [string]$DiagnosticPath = "",
    [object]$DiagnosticResult = $null
  )

  foreach ($caseId in $selectedCases) {
    $txtPath = (Get-CaseEvidenceBase -CaseId $caseId) + ".txt"
    Write-CaseHeader -TxtPath $txtPath -CaseId $caseId -TargetMode "desktop_window"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "environment:"
    Append-Text -TxtPath $txtPath -Line "- webdriver_url: $WebDriverUrl"
    Append-Text -TxtPath $txtPath -Line "- vite_url: $viteUrl"
    Append-Text -TxtPath $txtPath -Line "- webdriver_health_mode: $($script:WebDriverHealth.HealthMode)"
    Append-Text -TxtPath $txtPath -Line "- webdriver_status_endpoint: $($script:WebDriverHealth.StatusEndpointResult)"
    Append-Text -TxtPath $txtPath -Line "- webdriver_root_session_probe: $($script:WebDriverHealth.RootSessionProbeResult)"
    Append-Text -TxtPath $txtPath -Line "- webdriver_attach_window_probe: $($script:WebDriverHealth.AttachWindowProbeResult)"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    Append-Text -TxtPath $txtPath -Line "- blocked_phase: $Phase"
    Append-Text -TxtPath $txtPath -Line "- blocked_reason: $Reason"
    Append-Text -TxtPath $txtPath -Line "- manual_gate_required: physical keyboard verification required"
    if (-not [string]::IsNullOrWhiteSpace($DiagnosticPath)) {
      Append-Text -TxtPath $txtPath -Line "- hotkey_injection_diagnostic: $DiagnosticPath"
    }
    if ($null -ne $DiagnosticResult) {
      Append-Text -TxtPath $txtPath -Line "- hotkey_injection_summary: $($DiagnosticResult.summary)"
      foreach ($probe in $DiagnosticResult.probes) {
        Append-Text -TxtPath $txtPath -Line "- hotkey_probe_$($probe.hotkey): triggered=$($probe.triggered)"
      }
    }
    Append-Text -TxtPath $txtPath -Line "- visual_evidence: not produced because Codex injection cannot authoritatively verify real global hotkeys"
    Append-Text -TxtPath $txtPath -Line "- next_step: execute qa-gates/phase-4/p4-t1-global-hotkey.md with a physical keyboard"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: BLOCKED"
    $evidence = if (-not [string]::IsNullOrWhiteSpace($DiagnosticPath)) { "`$txt + `$diag" } else { "`$txt" }
    Set-CaseResult -CaseId $caseId -Result "BLOCKED" -Evidence $evidence
  }
}

function Stop-ProcessSafe {
  param($Process)

  if ($null -ne $Process) {
    $processId = $null
    try {
      if ($Process -is [System.Diagnostics.Process]) {
        $processId = $Process.Id
        if (-not $Process.HasExited) {
          Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        }
      } elseif ($Process.PSObject.Properties.Name -contains "Id") {
        $processId = $Process.Id
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
      }
    } catch {
    }

    if ($null -ne $processId) {
      Wait-ProcessExit -ProcessId $processId | Out-Null
    }
  }
}

function Wait-ProcessExit {
  param(
    [int]$ProcessId,
    [int]$Attempts = 40,
    [int]$DelayMs = 250
  )

  for ($index = 0; $index -lt $Attempts; $index++) {
    if (-not (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)) {
      return $true
    }
    Start-Sleep -Milliseconds $DelayMs
  }

  return $false
}

function Stop-RememberProcesses {
  Get-Process -Name "tauri-app" -ErrorAction SilentlyContinue | ForEach-Object {
    Stop-ProcessSafe -Process $_
  }
}

function Stop-NotepadProcesses {
  Get-Process -Name "notepad" -ErrorAction SilentlyContinue | ForEach-Object {
    Stop-ProcessSafe -Process $_
  }
}

function Stop-ViteProcesses {
  Get-CimInstance Win32_Process |
    Where-Object {
      ($_.Name -eq "node.exe" -or $_.Name -eq "cmd.exe") -and
      $_.CommandLine -match "vite" -and
      $_.CommandLine -match "$vitePort"
    } |
    ForEach-Object {
      try {
        Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop
        Wait-ProcessExit -ProcessId $_.ProcessId | Out-Null
      } catch {
      }
    }
}

function Wait-HttpReady {
  param(
    [string]$Url,
    [int]$Attempts = 60
  )

  for ($index = 0; $index -lt $Attempts; $index++) {
    try {
      $response = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 2
      if ($response.StatusCode -ge 200) {
        return
      }
    } catch {
    }
    Start-Sleep -Milliseconds 500
  }

  throw "timed out waiting for $Url"
}

function Start-ViteServer {
  $viteOut = Join-Path $runtimeLogDir "p4t1-vite.out.log"
  $viteErr = Join-Path $runtimeLogDir "p4t1-vite.err.log"

  for ($attempt = 1; $attempt -le 2; $attempt++) {
    Stop-ViteProcesses
    New-Dir -Path $runtimeLogDir
    if (Test-Path $viteOut) { Remove-Item $viteOut -Force }
    if (Test-Path $viteErr) { Remove-Item $viteErr -Force }

    $global:ViteProcess = Start-Process `
      -FilePath "npm.cmd" `
      -ArgumentList @("run", "dev", "--", "--host", "127.0.0.1", "--port", "$vitePort") `
      -WorkingDirectory $root `
      -PassThru `
      -RedirectStandardOutput $viteOut `
      -RedirectStandardError $viteErr

    try {
      Wait-HttpReady -Url $viteUrl
      return
    } catch {
      if ($global:ViteProcess -and -not $global:ViteProcess.HasExited) {
        Stop-Process -Id $global:ViteProcess.Id -Force -ErrorAction SilentlyContinue
        Wait-ProcessExit -ProcessId $global:ViteProcess.Id | Out-Null
      }
      $global:ViteProcess = $null

      if ($attempt -ge 2) {
        throw $_
      }

      Wait-DesktopSettle -Milliseconds 1000
    }
  }
}

function Stop-ViteServer {
  if ($global:ViteProcess -and -not $global:ViteProcess.HasExited) {
    Stop-Process -Id $global:ViteProcess.Id -Force -ErrorAction SilentlyContinue
    Wait-ProcessExit -ProcessId $global:ViteProcess.Id | Out-Null
  }

  Stop-ViteProcesses
  $global:ViteProcess = $null
}

function Backup-AppDataState {
  New-Dir -Path $backupDir
  if (Test-Path $configPath) {
    Copy-Item $configPath (Join-Path $backupDir "config.toml.bak") -Force
  }
  if (Test-Path $sqlitePath) {
    Copy-Item $sqlitePath (Join-Path $backupDir "remember.sqlite3.bak") -Force
  }
}

function Restore-AppDataState {
  Stop-RememberProcesses

  $configBackup = Join-Path $backupDir "config.toml.bak"
  $sqliteBackup = Join-Path $backupDir "remember.sqlite3.bak"

  if (Test-Path $configBackup) {
    Copy-Item $configBackup $configPath -Force
  } elseif (Test-Path $configPath) {
    Remove-Item $configPath -Force
  }

  if (Test-Path $sqliteBackup) {
    Copy-Item $sqliteBackup $sqlitePath -Force
  } elseif (Test-Path $sqlitePath) {
    Remove-Item $sqlitePath -Force
  }
}

function Write-AppConfig {
  New-Dir -Path $appDataDir
  $lines = @(
    'runtime_mode = "sqlite_only"',
    'hotkey = "Alt+Space"',
    'silent_days_threshold = 7'
  )
  Set-Content -Path $configPath -Value $lines -Encoding utf8
}

function Reset-SqliteDatabase {
  if (Test-Path $sqlitePath) {
    Remove-Item $sqlitePath -Force
  }
}

function Start-App {
  param([string]$CaseKey)

  New-Dir -Path $runtimeLogDir
  $stdout = Join-Path $runtimeLogDir "${CaseKey}-app.out.log"
  $stderr = Join-Path $runtimeLogDir "${CaseKey}-app.err.log"
  if (Test-Path $stdout) { Remove-Item $stdout -Force }
  if (Test-Path $stderr) { Remove-Item $stderr -Force }

  $process = Start-Process `
    -FilePath $exePath `
    -WorkingDirectory (Split-Path $exePath) `
    -PassThru `
    -RedirectStandardOutput $stdout `
    -RedirectStandardError $stderr

  return [pscustomobject]@{
    Process = $process
    StdoutPath = $stdout
    StderrPath = $stderr
  }
}

function Wait-ProcessWindow {
  param(
    [int]$ProcessId,
    [int]$Attempts = 60
  )

  for ($index = 0; $index -lt $Attempts; $index++) {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process) {
      throw "process $ProcessId exited before window became ready"
    }

    if ($process.MainWindowHandle -ne 0) {
      return $process
    }

    Start-Sleep -Milliseconds 500
  }

  throw "timed out waiting for main window of process $ProcessId"
}

function Wait-WindowTitle {
  param(
    [int]$ProcessId,
    [string[]]$AcceptedTitles,
    [int]$Attempts = 80
  )

  for ($index = 0; $index -lt $Attempts; $index++) {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process) {
      throw "process $ProcessId exited before window title became ready"
    }

    if ($process.MainWindowHandle -ne 0 -and $AcceptedTitles -contains $process.MainWindowTitle) {
      return $process
    }

    Start-Sleep -Milliseconds 500
  }

  throw "timed out waiting for window title $($AcceptedTitles -join ', ')"
}

function Focus-ProcessWindow {
  param([System.Diagnostics.Process]$Process)

  [Win32Native]::ShowWindow($Process.MainWindowHandle, 5) | Out-Null
  [Win32Native]::SetForegroundWindow($Process.MainWindowHandle) | Out-Null
  Start-Sleep -Milliseconds 500
}

function Wait-WindowVisibility {
  param(
    [int]$ProcessId,
    [bool]$ExpectedVisible,
    [int]$Attempts = 40
  )

  for ($index = 0; $index -lt $Attempts; $index++) {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process) {
      throw "process $ProcessId exited while waiting for visibility=$ExpectedVisible"
    }

    if ($process.MainWindowHandle -ne 0) {
      $visible = [Win32Native]::IsWindowVisible($process.MainWindowHandle)
      if ([bool]$visible -eq $ExpectedVisible) {
        return $process
      }
    }

    Start-Sleep -Milliseconds 250
  }

  Fail-Assert "window visibility did not become $ExpectedVisible for process $ProcessId"
}

function Wait-ForegroundHandle {
  param(
    [IntPtr]$ExpectedHandle,
    [int]$Attempts = 40
  )

  for ($index = 0; $index -lt $Attempts; $index++) {
    if ([Win32Native]::GetForegroundWindow() -eq $ExpectedHandle) {
      return
    }
    Start-Sleep -Milliseconds 250
  }

  Fail-Assert "foreground window did not switch to handle $ExpectedHandle"
}

function Take-WindowShot {
  param(
    [string]$Path,
    [switch]$FullScreen
  )

  if (Test-Path $Path) {
    Remove-Item $Path -Force
  }

  if ($FullScreen) {
    powershell -ExecutionPolicy Bypass -File $screenshotScript -Path $Path | Out-Null
  } else {
    powershell -ExecutionPolicy Bypass -File $screenshotScript -Path $Path -ActiveWindow | Out-Null
  }
}

function Start-ScreenRecording {
  param(
    [string]$Path,
    [int]$Seconds = 18,
    [int]$FrameIntervalMs = 1000
  )

  if (Test-Path $Path) {
    Remove-Item $Path -Force
  }

  $framesDir = Join-Path $env:TEMP ("p4t1-frames-" + [guid]::NewGuid().ToString())
  $workerScript = Join-Path $env:TEMP ("p4t1-record-worker-" + [guid]::NewGuid().ToString() + ".ps1")
  $frameCount = [Math]::Max(3, [int][Math]::Ceiling(($Seconds * 1000) / $FrameIntervalMs))
  New-Dir -Path $framesDir

  $workerCode = @"
param(
  [string]`$FramesDir,
  [int]`$FrameCount,
  [int]`$SleepMs,
  [string]`$ShotScript
)

for (`$index = 0; `$index -lt `$FrameCount; `$index++) {
  `$framePath = Join-Path `$FramesDir ("frame-{0:D4}.png" -f `$index)
  try {
    powershell -ExecutionPolicy Bypass -File `$ShotScript -Path `$framePath -ActiveWindow | Out-Null
  } catch {
  }

  Start-Sleep -Milliseconds `$SleepMs
}
"@
  Set-Content -Path $workerScript -Value $workerCode -Encoding ascii

  $process = Start-Process `
    -FilePath "powershell.exe" `
    -ArgumentList @(
      "-NoProfile",
      "-ExecutionPolicy", "Bypass",
      "-File", $workerScript,
      "-FramesDir", $framesDir,
      "-FrameCount", "$frameCount",
      "-SleepMs", "$FrameIntervalMs",
      "-ShotScript", $screenshotScript
    ) `
    -PassThru `
    -WindowStyle Hidden

  return [pscustomobject]@{
    Process = $process
    FramesDir = $framesDir
    WorkerScript = $workerScript
    OutputPath = $Path
    FrameRate = [Math]::Round(1000 / $FrameIntervalMs, 2)
  }
}

function Wait-ScreenRecording {
  param($Recording)

  if ($null -eq $Recording) {
    return
  }

  try {
    if ($Recording.Process) {
      Wait-Process -Id $Recording.Process.Id -Timeout 15
    }
  } catch {
    if ($Recording.Process) {
      Stop-Process -Id $Recording.Process.Id -Force -ErrorAction SilentlyContinue
    }
  }

  try {
    $framePattern = Join-Path $Recording.FramesDir "frame-%04d.png"
    $frames = Get-ChildItem -Path $Recording.FramesDir -Filter "frame-*.png" -ErrorAction SilentlyContinue |
      Sort-Object Name
    if (-not $frames) {
      throw "screen recording captured no frames"
    }

    & $ffmpegExe `
      -y `
      -hide_banner `
      -loglevel "error" `
      -framerate "$($Recording.FrameRate)" `
      -i $framePattern `
      -vf "pad=ceil(iw/2)*2:ceil(ih/2)*2" `
      -c:v "libx264" `
      -pix_fmt "yuv420p" `
      $Recording.OutputPath | Out-Null

    if ($LASTEXITCODE -ne 0) {
      throw "failed to encode screen recording (exit code $LASTEXITCODE)"
    }
  } finally {
    if (Test-Path $Recording.WorkerScript) {
      Remove-Item $Recording.WorkerScript -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path $Recording.FramesDir) {
      Remove-Item $Recording.FramesDir -Recurse -Force -ErrorAction SilentlyContinue
    }
  }
}

function Invoke-Python {
  param([string]$Code)

  $Code | & $pythonExe -
  if ($LASTEXITCODE -ne 0) {
    throw "python command failed with exit code $LASTEXITCODE"
  }
}

function Get-SqliteEvidence {
  param(
    [string]$SeriesName,
    [string]$CommitContent
  )

  $escapedPath = $sqlitePath.Replace("\", "\\")
  $escapedSeries = $SeriesName.Replace("'", "''")
  $escapedCommit = $CommitContent.Replace("'", "''")
  $code = @"
import sqlite3
path = r"$escapedPath"
conn = sqlite3.connect(path)
series_rows = conn.execute("select name, status, latest_excerpt from series where name = '$escapedSeries'").fetchall()
commit_rows = conn.execute("select content from commits where content = '$escapedCommit'").fetchall()
all_series = conn.execute("select name, status from series order by created_at asc, id asc").fetchall()
print("sqlite_target_series=", series_rows)
print("sqlite_target_commits=", commit_rows)
print("sqlite_all_series=", all_series)
"@
  return (Invoke-Python -Code $code | Out-String).Trim()
}

function Assert-SqliteSeriesAndCommit {
  param(
    [string]$SeriesName,
    [string]$CommitContent
  )

  $evidence = Get-SqliteEvidence -SeriesName $SeriesName -CommitContent $CommitContent
  if ($evidence -notmatch [regex]::Escape($SeriesName)) {
    Fail-Assert "sqlite evidence missing series $SeriesName"
  }
  if ($evidence -notmatch [regex]::Escape($CommitContent)) {
    Fail-Assert "sqlite evidence missing commit $CommitContent"
  }
  return $evidence
}

function Get-WebExceptionDetails {
  param($ErrorRecord)

  $message = $ErrorRecord.Exception.Message
  try {
    if ($ErrorRecord.ErrorDetails.Message) {
      return "$message | body: $($ErrorRecord.ErrorDetails.Message)"
    }

    if ($ErrorRecord.Exception.Response -and $ErrorRecord.Exception.Response.Content) {
      $body = $ErrorRecord.Exception.Response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
      if (-not [string]::IsNullOrWhiteSpace($body)) {
        return "$message | body: $body"
      }
    }
  } catch {
  }

  return $message
}

function Invoke-WebDriverJson {
  param(
    [string]$Method,
    [string]$Path,
    $Body = $null
  )

  $uri = $WebDriverUrl.TrimEnd("/") + $Path
  try {
    if ($null -eq $Body) {
      return Invoke-RestMethod -Method $Method -Uri $uri -TimeoutSec 10
    }

    $json = $Body | ConvertTo-Json -Depth 12
    return Invoke-RestMethod -Method $Method -Uri $uri -Body $json -ContentType "application/json; charset=utf-8" -TimeoutSec 15
  } catch {
    throw "webdriver $Method $Path failed: $(Get-WebExceptionDetails -ErrorRecord $_)"
  }
}

function Get-WebDriverSessionId {
  param($Response)

  if ($Response.PSObject.Properties.Name -contains "sessionId" -and $Response.sessionId) {
    return [string]$Response.sessionId
  }
  if ($Response.PSObject.Properties.Name -contains "value") {
    if ($Response.value.PSObject.Properties.Name -contains "sessionId" -and $Response.value.sessionId) {
      return [string]$Response.value.sessionId
    }
  }

  throw "webdriver session response did not contain sessionId"
}

function Start-RootSession {
  $body = @{
    capabilities = @{
      alwaysMatch = @{
        platformName = "windows"
        "appium:automationName" = "windows"
        "appium:app" = "Root"
      }
    }
    desiredCapabilities = @{
      platformName = "Windows"
      deviceName = "WindowsPC"
      app = "Root"
    }
  }

  $response = Invoke-WebDriverJson -Method Post -Path "/session" -Body $body
  return Get-WebDriverSessionId -Response $response
}

function Start-AttachedWindowSession {
  param([IntPtr]$WindowHandle)

  $hexHandle = "0x{0:X}" -f ([int64]$WindowHandle)
  $body = @{
    capabilities = @{
      alwaysMatch = @{
        platformName = "windows"
        "appium:automationName" = "windows"
        "appium:appTopLevelWindow" = $hexHandle
      }
    }
    desiredCapabilities = @{
      platformName = "Windows"
      deviceName = "WindowsPC"
      appTopLevelWindow = $hexHandle
    }
  }

  $response = Invoke-WebDriverJson -Method Post -Path "/session" -Body $body
  return Get-WebDriverSessionId -Response $response
}

function Stop-WebDriverSession {
  param([string]$SessionId)

  if ([string]::IsNullOrWhiteSpace($SessionId)) {
    return
  }

  try {
    Invoke-WebDriverJson -Method Delete -Path "/session/$SessionId" | Out-Null
  } catch {
  }

  Wait-WebDriverSessionClosed -SessionId $SessionId | Out-Null
}

function Wait-WebDriverSessionClosed {
  param(
    [string]$SessionId,
    [int]$Attempts = 12,
    [int]$DelayMs = 250
  )

  if ([string]::IsNullOrWhiteSpace($SessionId)) {
    return $true
  }

  for ($index = 0; $index -lt $Attempts; $index++) {
    try {
      $uri = $WebDriverUrl.TrimEnd("/") + "/session/$SessionId/window/handles"
      Invoke-RestMethod -Method Get -Uri $uri -TimeoutSec 2 | Out-Null
    } catch {
      return $true
    }
    Start-Sleep -Milliseconds $DelayMs
  }

  return $false
}

function Get-WebDriverHealth {
  $health = [ordered]@{
    Reachable = $false
    StatusEndpointOk = $false
    RootSessionOk = $false
    AttachWindowOk = $false
    HealthMode = "failed"
    StatusEndpointResult = if ($SkipStatusProbe) { "skipped" } else { "not-run" }
    RootSessionProbeResult = "not-run"
    AttachWindowProbeResult = "not-run"
    FailureReason = ""
  }

  if (-not $SkipStatusProbe) {
    try {
      $null = Invoke-RestMethod -Method Get -Uri ($WebDriverUrl.TrimEnd("/") + "/status") -TimeoutSec 3
      $health.StatusEndpointOk = $true
      $health.StatusEndpointResult = "ok"
    } catch {
      $health.StatusEndpointResult = "failed: $(Get-WebExceptionDetails -ErrorRecord $_)"
    }
  }

  $probeSessionId = $null
  try {
    $probeSessionId = Start-RootSession
    $health.RootSessionOk = $true
    $health.RootSessionProbeResult = "ok"
  } catch {
    $health.RootSessionProbeResult = "failed: $($_.Exception.Message)"
    $health.FailureReason = "webdriver root-session probe failed: $($_.Exception.Message)"
  } finally {
    Stop-WebDriverSession -SessionId $probeSessionId
  }

  if ($health.RootSessionOk) {
    $health.Reachable = $true
    $health.HealthMode = if ($health.StatusEndpointOk) { "status" } else { "root-session" }
    if (-not $health.StatusEndpointOk) {
      $health.FailureReason = "webdriver status failed, but root-session probe succeeded"
    }
  } elseif (-not $health.StatusEndpointOk -and [string]::IsNullOrWhiteSpace($health.FailureReason)) {
    $health.FailureReason = "webdriver status failed and root-session probe did not recover the service"
  }

  return [pscustomobject]$health
}

function Invoke-WebDriverActions {
  param(
    [string]$SessionId,
    [object[]]$Actions
  )

  $body = @{
    actions = @(
      @{
        type = "key"
        id = "keyboard"
        actions = $Actions
      }
    )
  }

  Invoke-WebDriverJson -Method Post -Path "/session/$SessionId/actions" -Body $body | Out-Null
  Invoke-WebDriverJson -Method Delete -Path "/session/$SessionId/actions" | Out-Null
}

function Send-WebDriverChord {
  param(
    [string]$SessionId,
    [string[]]$Keys
  )

  throw "webdriver keyboard input is disabled for P4-T1; use win32 input helpers instead"
}

function Send-WebDriverText {
  param(
    [string]$SessionId,
    [string]$Text
  )

  throw "webdriver keyboard input is disabled for P4-T1; use win32 input helpers instead"
}

function New-KeyInputRecord {
  param(
    [uint16]$VirtualKey = 0,
    [uint16]$ScanCode = 0,
    [uint32]$Flags = 0
  )

  $input = New-Object Win32Native+INPUT
  $input.type = $script:InputTypeKeyboard
  $input.U.ki.wVk = $VirtualKey
  $input.U.ki.wScan = $ScanCode
  $input.U.ki.dwFlags = $Flags
  $input.U.ki.time = 0
  $input.U.ki.dwExtraInfo = [IntPtr]::Zero
  return $input
}

function Invoke-Win32KeyboardInput {
  param(
    [Win32Native+INPUT[]]$Inputs,
    [string]$Description = "keyboard input"
  )

  if ($null -eq $Inputs -or $Inputs.Length -eq 0) {
    return
  }

  $inputSize = [System.Runtime.InteropServices.Marshal]::SizeOf([type]"Win32Native+INPUT")
  $sent = [Win32Native]::SendInput([uint32]$Inputs.Length, $Inputs, $inputSize)
  if ($sent -ne $Inputs.Length) {
    throw "win32 SendInput failed for $Description (sent $sent of $($Inputs.Length))"
  }
}

function Send-KeyChordWin32 {
  param(
    [uint16[]]$Keys,
    [string]$Description = "key chord",
    [int]$PostDelayMs = 500
  )

  $records = [System.Collections.Generic.List[object]]::new()
  foreach ($key in $Keys) {
    $records.Add((New-KeyInputRecord -VirtualKey $key))
  }
  for ($index = $Keys.Count - 1; $index -ge 0; $index--) {
    $records.Add((New-KeyInputRecord -VirtualKey $Keys[$index] -Flags $script:KeyEventKeyUp))
  }

  $inputArray = New-Object "Win32Native+INPUT[]" $records.Count
  for ($index = 0; $index -lt $records.Count; $index++) {
    $inputArray[$index] = $records[$index]
  }

  Invoke-Win32KeyboardInput -Inputs $inputArray -Description $Description
  Start-Sleep -Milliseconds $PostDelayMs
}

function Send-HotkeyWin32 {
  param([int]$PostDelayMs = 700)

  Send-KeyChordWin32 -Keys @($vkKeys.Alt, $vkKeys.Space) -Description "Alt+Space hotkey" -PostDelayMs $PostDelayMs
}

function Send-TextWin32 {
  param(
    [string]$Text,
    [int]$PostDelayMs = 300
  )

  $records = [System.Collections.Generic.List[object]]::new()
  foreach ($char in $Text.ToCharArray()) {
    $scanCode = [uint16][char]$char
    $records.Add((New-KeyInputRecord -ScanCode $scanCode -Flags $script:KeyEventUnicode))
    $records.Add((New-KeyInputRecord -ScanCode $scanCode -Flags ($script:KeyEventUnicode -bor $script:KeyEventKeyUp)))
  }

  $inputArray = New-Object "Win32Native+INPUT[]" $records.Count
  for ($index = 0; $index -lt $records.Count; $index++) {
    $inputArray[$index] = $records[$index]
  }

  Invoke-Win32KeyboardInput -Inputs $inputArray -Description "text input"
  Start-Sleep -Milliseconds $PostDelayMs
}

function Get-WindowInventory {
  $windows = Get-Process -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 -and -not [string]::IsNullOrWhiteSpace($_.MainWindowTitle) } |
    Sort-Object ProcessName, Id

  if (-not $windows) {
    return @("no visible windows")
  }

  return $windows | ForEach-Object {
    "$($_.ProcessName)#$($_.Id) [$($_.MainWindowTitle)]"
  }
}

function Get-FilePreview {
  param(
    [string]$Path,
    [int]$Tail = 40
  )

  if (-not (Test-Path $Path)) {
    return @("missing: $Path")
  }

  $lines = Get-Content -Path $Path -Tail $Tail -ErrorAction SilentlyContinue
  if (-not $lines) {
    return @("empty: $Path")
  }

  return $lines
}

function Write-StartupDiagnostic {
  param(
    [string]$CaseKey,
    [string[]]$AcceptedTitles,
    [string]$StdoutPath,
    [string]$StderrPath,
    [string]$FailureMessage,
    [int]$Attempt,
    $Process
  )

  $diagPath = Join-Path $runtimeLogDir "${CaseKey}-startup-diagnostic-attempt${Attempt}.txt"
  $exitCode = "unavailable"
  $processState = "missing"

  if ($Process) {
    try {
      $Process.Refresh()
      $processState = if ($Process.HasExited) { "exited" } else { "running" }
      if ($Process.HasExited) {
        $exitCode = $Process.ExitCode
      }
    } catch {
      $processState = "refresh-failed: $($_.Exception.Message)"
    }
  }

  $lines = [System.Collections.Generic.List[string]]::new()
  $lines.Add("case_key: $CaseKey")
  $lines.Add("attempt: $Attempt")
  $lines.Add("accepted_titles: $($AcceptedTitles -join ' | ')")
  $lines.Add("failure: $FailureMessage")
  $lines.Add("process_state: $processState")
  $lines.Add("process_exit_code: $exitCode")
  $lines.Add("stdout_path: $StdoutPath")
  $lines.Add("stderr_path: $StderrPath")
  $lines.Add("")
  $lines.Add("window_inventory:")
  foreach ($line in (Get-WindowInventory)) {
    $lines.Add("- $line")
  }
  $lines.Add("")
  $lines.Add("stdout_tail:")
  foreach ($line in (Get-FilePreview -Path $StdoutPath)) {
    $lines.Add("- $line")
  }
  $lines.Add("")
  $lines.Add("stderr_tail:")
  foreach ($line in (Get-FilePreview -Path $StderrPath)) {
    $lines.Add("- $line")
  }

  Set-Content -Path $diagPath -Value $lines -Encoding ascii
  return $diagPath
}

function Wait-DesktopSettle {
  param([int]$Milliseconds = 750)

  Start-Sleep -Milliseconds $Milliseconds
}

function Wait-LogContains {
  param(
    [string[]]$Paths,
    [string]$Pattern,
    [int]$Attempts = 40
  )

  for ($index = 0; $index -lt $Attempts; $index++) {
    foreach ($path in $Paths) {
      if ((Test-Path $path) -and (Select-String -Path $path -Pattern $Pattern -SimpleMatch -Quiet)) {
        return
      }
    }
    Start-Sleep -Milliseconds 250
  }

  Fail-Assert "log pattern '$Pattern' not found in $($Paths -join ', ')"
}

function Get-LogExcerpt {
  param(
    [string[]]$Paths,
    [string]$Pattern
  )

  $lines = [System.Collections.Generic.List[string]]::new()
  foreach ($path in $Paths) {
    if (Test-Path $path) {
      $matches = Select-String -Path $path -Pattern $Pattern -SimpleMatch
      foreach ($match in $matches) {
        $lines.Add("$(Split-Path $path -Leaf): $($match.Line.Trim())")
      }
    }
  }

  if ($lines.Count -eq 0) {
    return @("no matching log lines")
  }

  return $lines
}

function Start-HotkeyConflictHelper {
  $readyFile = Join-Path $runtimeLogDir ("hotkey-helper-" + [guid]::NewGuid().ToString() + ".ready")
  $logFile = Join-Path $runtimeLogDir ("hotkey-helper-" + [guid]::NewGuid().ToString() + ".log")
  $process = Start-Process `
    -FilePath "powershell.exe" `
    -ArgumentList @(
      "-NoProfile",
      "-ExecutionPolicy", "Bypass",
      "-File", $helperScript,
      "-ReadyFile", $readyFile,
      "-LogFile", $logFile
    ) `
    -PassThru `
    -WindowStyle Hidden

  for ($index = 0; $index -lt 30; $index++) {
    if (Test-Path $readyFile) {
      return [pscustomobject]@{
        Process = $process
        ReadyFile = $readyFile
        LogFile = $logFile
      }
    }

    if ($process.HasExited) {
      throw "hotkey conflict helper exited before becoming ready"
    }

    Start-Sleep -Milliseconds 250
  }

  throw "timed out waiting for hotkey conflict helper readiness"
}

function Stop-HotkeyConflictHelper {
  param($Helper)

  if ($null -eq $Helper) {
    return
  }

  try {
    if ($Helper.Process -and -not $Helper.Process.HasExited) {
      Stop-Process -Id $Helper.Process.Id -Force -ErrorAction SilentlyContinue
      Wait-ProcessExit -ProcessId $Helper.Process.Id | Out-Null
    }
  } catch {
  }

  Wait-DesktopSettle -Milliseconds 500
}

function Invoke-HotkeyInjectionDiagnostic {
  $diagnosticPath = Join-Path $outputDir "P4-T1-HOTKEY-DIAG_${runDate}_${EnvId}_${tester}.txt"
  if (Test-Path $diagnosticPath) {
    Remove-Item $diagnosticPath -Force
  }

  $json = & powershell.exe `
    -NoProfile `
    -ExecutionPolicy Bypass `
    -File $hotkeyDiagnosticScript `
    -HelperScript $helperScript `
    -OutputPath $diagnosticPath
  if ($LASTEXITCODE -ne 0) {
    throw "hotkey injection diagnostic failed with exit code $LASTEXITCODE"
  }

  $payload = ($json | Out-String).Trim()
  if ([string]::IsNullOrWhiteSpace($payload)) {
    throw "hotkey injection diagnostic produced no output"
  }

  try {
    $result = $payload | ConvertFrom-Json
  } catch {
    throw "failed to parse hotkey injection diagnostic output: $payload"
  }

  return [pscustomobject]@{
    Path = $diagnosticPath
    Result = $result
  }
}

function Start-NotepadWindow {
  $windowScript = Join-Path $env:TEMP ("p4t1-background-window-" + [guid]::NewGuid().ToString() + ".ps1")
  $windowCode = @"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
`$form = New-Object System.Windows.Forms.Form
`$form.Text = 'P4T1 Background'
`$form.Width = 420
`$form.Height = 180
`$form.StartPosition = 'CenterScreen'
`$form.TopMost = `$false
`$label = New-Object System.Windows.Forms.Label
`$label.Dock = 'Fill'
`$label.TextAlign = 'MiddleCenter'
`$label.Font = New-Object System.Drawing.Font('Segoe UI', 12)
`$label.Text = 'P4-T1 background window'
`$form.Controls.Add(`$label)
[void]`$form.ShowDialog()
"@
  Set-Content -Path $windowScript -Value $windowCode -Encoding ascii

  $process = Start-Process -FilePath "powershell.exe" -ArgumentList @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-STA",
    "-File", $windowScript
  ) -PassThru
  $window = Wait-ProcessWindow -ProcessId $process.Id
  Focus-ProcessWindow -Process $window
  return $window
}

function Prepare-CleanEnvironment {
  Stop-RememberProcesses
  Stop-NotepadProcesses
  Stop-ViteServer
  Wait-DesktopSettle -Milliseconds 500
  Reset-SqliteDatabase
  Write-AppConfig
  Start-ViteServer
  Wait-DesktopSettle -Milliseconds 300
}

function Start-CaseAppWindow {
  param(
    [string]$CaseKey,
    [string[]]$AcceptedTitles,
    [int]$Attempts = 2
  )

  for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
    $app = Start-App -CaseKey $CaseKey
    try {
      $window = Wait-WindowTitle -ProcessId $app.Process.Id -AcceptedTitles $AcceptedTitles
      Start-Sleep -Seconds 2

      return [pscustomobject]@{
        Process = $window
        StdoutPath = $app.StdoutPath
        StderrPath = $app.StderrPath
      }
    } catch {
      $diagPath = Write-StartupDiagnostic `
        -CaseKey $CaseKey `
        -AcceptedTitles $AcceptedTitles `
        -StdoutPath $app.StdoutPath `
        -StderrPath $app.StderrPath `
        -FailureMessage $_.Exception.Message `
        -Attempt $attempt `
        -Process $app.Process

      Stop-ProcessSafe -Process $app.Process
      if ($attempt -ge $Attempts) {
        throw "$($_.Exception.Message) (startup diagnostics: $diagPath)"
      }

      Wait-DesktopSettle
    }
  }
}

function Test-AttachedWindowProbe {
  param([System.Diagnostics.Process]$Process)

  $probeSessionId = $null
  try {
    $probeSessionId = Start-AttachedWindowSession -WindowHandle $Process.MainWindowHandle
    return [pscustomobject]@{
      Ok = $true
      FailureReason = ""
    }
  } catch {
    return [pscustomobject]@{
      Ok = $false
      FailureReason = $_.Exception.Message
    }
  } finally {
    Stop-WebDriverSession -SessionId $probeSessionId
  }
}

function Invoke-AttachWindowProbe {
  $probeApp = $null
  $attachResult = $null

  try {
    Prepare-CleanEnvironment
    $probeApp = Start-CaseAppWindow -CaseKey "p4t1-attach-probe" -AcceptedTitles @($appTitle)
    $attachResult = Test-AttachedWindowProbe -Process $probeApp.Process

    if ($attachResult.Ok) {
      $script:WebDriverHealth.AttachWindowOk = $true
      $script:WebDriverHealth.AttachWindowProbeResult = "ok"
      return
    }

    $script:WebDriverHealth.AttachWindowOk = $false
    $script:WebDriverHealth.AttachWindowProbeResult = "failed: $($attachResult.FailureReason)"
    $script:WebDriverHealth.FailureReason = "attach-window probe failed: $($attachResult.FailureReason)"
    throw "attach-window probe failed: $($attachResult.FailureReason)"
  } finally {
    if ($probeApp) { Stop-ProcessSafe -Process $probeApp.Process }
    Stop-ViteServer
    Wait-DesktopSettle
  }
}

function Run-VGPassCase {
  $caseId = "P4-T1-VG-PASS"
  $txtPath = (Get-CaseEvidenceBase -CaseId $caseId) + ".txt"
  $pngPath = (Get-CaseEvidenceBase -CaseId $caseId) + ".png"
  Write-CaseHeader -TxtPath $txtPath -CaseId $caseId -TargetMode "desktop_window"

  $app = $null
  $notepad = $null
  $result = "PASS"
  $note = "global hotkey toggled hidden -> shown -> hidden on the real desktop window"

  try {
    Prepare-CleanEnvironment
    $app = Start-CaseAppWindow -CaseKey "p4t1-vg-pass" -AcceptedTitles @($appTitle)

    $notepad = Start-NotepadWindow
    Send-HotkeyWin32
    Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $false | Out-Null

    Focus-ProcessWindow -Process $notepad
    Send-HotkeyWin32
    $refreshed = Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $true
    Wait-ForegroundHandle -ExpectedHandle $refreshed.MainWindowHandle
    Take-WindowShot -Path $pngPath

    Send-HotkeyWin32
    Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $false | Out-Null

    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "environment:"
    Append-Text -TxtPath $txtPath -Line "- webdriver_url: $WebDriverUrl"
    Append-Text -TxtPath $txtPath -Line "- vite_url: $viteUrl"
    Append-Text -TxtPath $txtPath -Line "- keyboard_backend: $script:KeyboardBackend"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    Append-Text -TxtPath $txtPath -Line "- screenshot: $pngPath"
    Append-Text -TxtPath $txtPath -Line "- shown_title: $($refreshed.MainWindowTitle)"
    Append-Text -TxtPath $txtPath -Line "- final_visibility: hidden"
    Append-Text -TxtPath $txtPath -Line "- note: $note"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: PASS"
  } catch {
    $message = $_.Exception.Message
    $result = if ($message.StartsWith("ASSERT:")) { "FAIL" } else { "BLOCKED" }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    Append-Text -TxtPath $txtPath -Line "- failure: $message"
    if (Test-Path $pngPath) {
      Append-Text -TxtPath $txtPath -Line "- screenshot: $pngPath"
    }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  } finally {
    Stop-ProcessSafe -Process $notepad
    if ($app) { Stop-ProcessSafe -Process $app.Process }
    Stop-ViteServer
  }

  $evidence = if (Test-Path $pngPath) { "`$png + `$txt" } else { "`$txt" }
  Set-CaseResult -CaseId $caseId -Result $result -Evidence $evidence
}

function Run-IGPassCase {
  $caseId = "P4-T1-IG-PASS"
  $txtPath = (Get-CaseEvidenceBase -CaseId $caseId) + ".txt"
  $mp4Path = (Get-CaseEvidenceBase -CaseId $caseId) + ".mp4"
  Write-CaseHeader -TxtPath $txtPath -CaseId $caseId -TargetMode "desktop_window"

  $app = $null
  $notepad = $null
  $recording = $null
  $seriesName = "P4T1 Inbox"
  $commitContent = "p4t1-first-note"
  $result = "PASS"

  try {
    Prepare-CleanEnvironment
    $app = Start-CaseAppWindow -CaseKey "p4t1-ig-pass" -AcceptedTitles @($appTitle)
    $recording = Start-ScreenRecording -Path $mp4Path

    Focus-ProcessWindow -Process $app.Process
    Send-HotkeyWin32
    Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $false | Out-Null

    $notepad = Start-NotepadWindow
    Send-HotkeyWin32
    $refreshed = Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $true
    Wait-ForegroundHandle -ExpectedHandle $refreshed.MainWindowHandle

    Send-KeyChordWin32 -Keys @($vkKeys.Shift, $vkKeys.N) -Description "Shift+N create series"
    Send-TextWin32 -Text $seriesName
    Send-KeyChordWin32 -Keys @($vkKeys.Enter) -Description "confirm series create" -PostDelayMs 400
    Send-TextWin32 -Text $commitContent
    Send-KeyChordWin32 -Keys @($vkKeys.Enter) -Description "confirm commit create" -PostDelayMs 400
    Start-Sleep -Seconds 1

    $sqliteEvidence = Assert-SqliteSeriesAndCommit -SeriesName $seriesName -CommitContent $commitContent

    Send-HotkeyWin32
    Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $false | Out-Null

    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "environment:"
    Append-Text -TxtPath $txtPath -Line "- webdriver_url: $WebDriverUrl"
    Append-Text -TxtPath $txtPath -Line "- vite_url: $viteUrl"
    Append-Text -TxtPath $txtPath -Line "- keyboard_backend: $script:KeyboardBackend"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    Append-Text -TxtPath $txtPath -Line "- recording: $mp4Path"
    Append-Text -TxtPath $txtPath -Line "- action_chain: hide -> background -> hotkey show -> Shift+N -> create series -> create commit -> hotkey hide"
    Append-Text -TxtPath $txtPath -Line "- final_visibility: hidden"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "sqlite_proof:"
    foreach ($line in ($sqliteEvidence -split "`r?`n")) {
      Append-Text -TxtPath $txtPath -Line "- $line"
    }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: PASS"
  } catch {
    $message = $_.Exception.Message
    $result = if ($message.StartsWith("ASSERT:")) { "FAIL" } else { "BLOCKED" }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    if (Test-Path $mp4Path) {
      Append-Text -TxtPath $txtPath -Line "- recording: $mp4Path"
    }
    Append-Text -TxtPath $txtPath -Line "- failure: $message"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  } finally {
    try {
      Wait-ScreenRecording -Recording $recording
    } catch {
      if ($result -eq "PASS") {
        $result = "BLOCKED"
        Append-Text -TxtPath $txtPath -Line ""
        Append-Text -TxtPath $txtPath -Line "recording_error: $($_.Exception.Message)"
      }
    }
    Stop-ProcessSafe -Process $notepad
    if ($app) { Stop-ProcessSafe -Process $app.Process }
    Stop-ViteServer
  }

  $evidence = if (Test-Path $mp4Path) { "`$mp4 + `$txt" } else { "`$txt" }
  Set-CaseResult -CaseId $caseId -Result $result -Evidence $evidence
}

function Run-VGFailCase {
  $caseId = "P4-T1-VG-FAIL"
  $txtPath = (Get-CaseEvidenceBase -CaseId $caseId) + ".txt"
  $pngPath = (Get-CaseEvidenceBase -CaseId $caseId) + ".png"
  Write-CaseHeader -TxtPath $txtPath -CaseId $caseId -TargetMode "desktop_window"

  $app = $null
  $notepad = $null
  $helper = $null
  $recoveryApp = $null
  $result = "PASS"

  try {
    Prepare-CleanEnvironment
    $helper = Start-HotkeyConflictHelper
    $app = Start-CaseAppWindow -CaseKey "p4t1-vg-fail" -AcceptedTitles @($hotkeyDisabledTitle)
    Wait-LogContains -Paths @($app.StdoutPath, $app.StderrPath, $helper.LogFile) -Pattern "global hotkey disabled"
    $notepad = Start-NotepadWindow
    $beforeHandle = [Win32Native]::GetForegroundWindow()

    Send-HotkeyWin32 -PostDelayMs 1000
    Start-Sleep -Seconds 1
    $afterHandle = [Win32Native]::GetForegroundWindow()
    if ($afterHandle -eq $app.Process.MainWindowHandle) {
      Fail-Assert "app window was raised even though hotkey registration was disabled"
    }
    if ($afterHandle -ne $beforeHandle) {
      Start-Sleep -Milliseconds 500
    }

    Take-WindowShot -Path $pngPath
    $logExcerpt = Get-LogExcerpt -Paths @($app.StdoutPath, $app.StderrPath) -Pattern "global hotkey disabled"

    Stop-ProcessSafe -Process $notepad
    $notepad = $null
    Stop-ProcessSafe -Process $app.Process
    $app = $null
    Stop-HotkeyConflictHelper -Helper $helper
    $helper = $null

    Prepare-CleanEnvironment
    $recoveryApp = Start-CaseAppWindow -CaseKey "p4t1-vg-fail-recovery" -AcceptedTitles @($appTitle)
    $notepad = Start-NotepadWindow

    Send-HotkeyWin32
    Wait-WindowVisibility -ProcessId $recoveryApp.Process.Id -ExpectedVisible $false | Out-Null
    Focus-ProcessWindow -Process $notepad
    Send-HotkeyWin32
    $recovered = Wait-WindowVisibility -ProcessId $recoveryApp.Process.Id -ExpectedVisible $true
    Wait-ForegroundHandle -ExpectedHandle $recovered.MainWindowHandle

    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    Append-Text -TxtPath $txtPath -Line "- screenshot: $pngPath"
    Append-Text -TxtPath $txtPath -Line "- keyboard_backend: $script:KeyboardBackend"
    Append-Text -TxtPath $txtPath -Line "- disabled_title: $hotkeyDisabledTitle"
    Append-Text -TxtPath $txtPath -Line "- recovery_title: $($recovered.MainWindowTitle)"
    Append-Text -TxtPath $txtPath -Line "- note: conflict helper prevented app raise, and recovery pass succeeded after helper shutdown"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "log_excerpt:"
    foreach ($line in $logExcerpt) {
      Append-Text -TxtPath $txtPath -Line "- $line"
    }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: PASS"
  } catch {
    $message = $_.Exception.Message
    $result = if ($message.StartsWith("ASSERT:")) { "FAIL" } else { "BLOCKED" }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    if (Test-Path $pngPath) {
      Append-Text -TxtPath $txtPath -Line "- screenshot: $pngPath"
    }
    Append-Text -TxtPath $txtPath -Line "- failure: $message"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  } finally {
    Stop-ProcessSafe -Process $notepad
    if ($app) { Stop-ProcessSafe -Process $app.Process }
    if ($recoveryApp) { Stop-ProcessSafe -Process $recoveryApp.Process }
    Stop-HotkeyConflictHelper -Helper $helper
    Stop-ViteServer
  }

  $evidence = if (Test-Path $pngPath) { "`$png + `$txt" } else { "`$txt" }
  Set-CaseResult -CaseId $caseId -Result $result -Evidence $evidence
}

function Run-IGFailCase {
  $caseId = "P4-T1-IG-FAIL"
  $txtPath = (Get-CaseEvidenceBase -CaseId $caseId) + ".txt"
  $mp4Path = (Get-CaseEvidenceBase -CaseId $caseId) + ".mp4"
  Write-CaseHeader -TxtPath $txtPath -CaseId $caseId -TargetMode "desktop_window"

  $app = $null
  $notepad = $null
  $helper = $null
  $recording = $null
  $recoveryApp = $null
  $recoverySeries = "P4T1 Recovery"
  $recoveryCommit = "p4t1-recovery-note"
  $result = "PASS"

  try {
    Prepare-CleanEnvironment
    $helper = Start-HotkeyConflictHelper
    $app = Start-CaseAppWindow -CaseKey "p4t1-ig-fail" -AcceptedTitles @($hotkeyDisabledTitle)
    Wait-LogContains -Paths @($app.StdoutPath, $app.StderrPath, $helper.LogFile) -Pattern "global hotkey disabled"
    $notepad = Start-NotepadWindow
    $recording = Start-ScreenRecording -Path $mp4Path

    for ($index = 0; $index -lt 3; $index++) {
      Send-HotkeyWin32 -PostDelayMs 400
      Start-Sleep -Milliseconds 400
    }

    if ([Win32Native]::GetForegroundWindow() -eq $app.Process.MainWindowHandle) {
      Fail-Assert "app unexpectedly became foreground during illegal hotkey spam"
    }

    $logExcerpt = Get-LogExcerpt -Paths @($app.StdoutPath, $app.StderrPath) -Pattern "global hotkey disabled"

    Stop-ProcessSafe -Process $notepad
    $notepad = $null
    Stop-ProcessSafe -Process $app.Process
    $app = $null
    Stop-HotkeyConflictHelper -Helper $helper
    $helper = $null

    Prepare-CleanEnvironment
    $recoveryApp = Start-CaseAppWindow -CaseKey "p4t1-ig-fail-recovery" -AcceptedTitles @($appTitle)
    $notepad = Start-NotepadWindow

    Focus-ProcessWindow -Process $recoveryApp.Process
    Send-HotkeyWin32
    Wait-WindowVisibility -ProcessId $recoveryApp.Process.Id -ExpectedVisible $false | Out-Null
    Focus-ProcessWindow -Process $notepad
    Send-HotkeyWin32
    $shown = Wait-WindowVisibility -ProcessId $recoveryApp.Process.Id -ExpectedVisible $true
    Wait-ForegroundHandle -ExpectedHandle $shown.MainWindowHandle

    Send-KeyChordWin32 -Keys @($vkKeys.Shift, $vkKeys.N) -Description "Shift+N recovery create series"
    Send-TextWin32 -Text $recoverySeries
    Send-KeyChordWin32 -Keys @($vkKeys.Enter) -Description "confirm recovery series create" -PostDelayMs 400
    Send-TextWin32 -Text $recoveryCommit
    Send-KeyChordWin32 -Keys @($vkKeys.Enter) -Description "confirm recovery commit create" -PostDelayMs 400
    Start-Sleep -Seconds 1

    $sqliteEvidence = Assert-SqliteSeriesAndCommit -SeriesName $recoverySeries -CommitContent $recoveryCommit

    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    Append-Text -TxtPath $txtPath -Line "- recording: $mp4Path"
    Append-Text -TxtPath $txtPath -Line "- keyboard_backend: $script:KeyboardBackend"
    Append-Text -TxtPath $txtPath -Line "- action_chain: illegal hotkey spam under conflict -> helper removed -> hotkey recovery -> create recovery series and commit"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "log_excerpt:"
    foreach ($line in $logExcerpt) {
      Append-Text -TxtPath $txtPath -Line "- $line"
    }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "sqlite_proof:"
    foreach ($line in ($sqliteEvidence -split "`r?`n")) {
      Append-Text -TxtPath $txtPath -Line "- $line"
    }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: PASS"
  } catch {
    $message = $_.Exception.Message
    $result = if ($message.StartsWith("ASSERT:")) { "FAIL" } else { "BLOCKED" }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    if (Test-Path $mp4Path) {
      Append-Text -TxtPath $txtPath -Line "- recording: $mp4Path"
    }
    Append-Text -TxtPath $txtPath -Line "- failure: $message"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  } finally {
    try {
      Wait-ScreenRecording -Recording $recording
    } catch {
      if ($result -eq "PASS") {
        $result = "BLOCKED"
        Append-Text -TxtPath $txtPath -Line ""
        Append-Text -TxtPath $txtPath -Line "recording_error: $($_.Exception.Message)"
      }
    }
    Stop-ProcessSafe -Process $notepad
    if ($app) { Stop-ProcessSafe -Process $app.Process }
    if ($recoveryApp) { Stop-ProcessSafe -Process $recoveryApp.Process }
    Stop-HotkeyConflictHelper -Helper $helper
    Stop-ViteServer
  }

  $evidence = if (Test-Path $mp4Path) { "`$mp4 + `$txt" } else { "`$txt" }
  Set-CaseResult -CaseId $caseId -Result $result -Evidence $evidence
}

foreach ($caseId in $Cases) {
  if ($validCases -notcontains $caseId) {
    throw "unsupported case id: $caseId"
  }
  $selectedCases.Add($caseId)
}

try {
  New-Dir -Path $outputDir
  New-Dir -Path $runtimeLogDir
  Backup-AppDataState

  $precheckFailures = [System.Collections.Generic.List[string]]::new()
  if (-not (Test-Path $exePath)) {
    $precheckFailures.Add("tauri app binary missing: $exePath")
  }
  if (-not (Test-Path $screenshotScript)) {
    $precheckFailures.Add("screenshot script missing: $screenshotScript")
  }
  if (-not (Test-Path $pythonExe)) {
    $precheckFailures.Add("uv-managed python missing: $pythonExe")
  }
  if (-not (Test-Path $helperScript)) {
    $precheckFailures.Add("hotkey conflict helper missing: $helperScript")
  }
  if (-not (Test-Path $hotkeyDiagnosticScript)) {
    $precheckFailures.Add("hotkey diagnostic script missing: $hotkeyDiagnosticScript")
  }
  if ([string]::IsNullOrWhiteSpace($ffmpegExe)) {
    $precheckFailures.Add("ffmpeg not available on PATH")
  }
  if (-not (Get-Command npm.cmd -ErrorAction SilentlyContinue)) {
    $precheckFailures.Add("npm.cmd not available")
  }

  if ($precheckFailures.Count -gt 0) {
    $reason = $precheckFailures -join "; "
    Write-BlockedArtifacts -Reason $reason
    Update-MatrixStatus -Status "BLOCKED"
  } else {
    $script:WebDriverHealth = Get-WebDriverHealth

    if (-not $script:WebDriverHealth.RootSessionOk) {
      Write-BlockedArtifacts -Reason $script:WebDriverHealth.FailureReason -Phase "root-session"
      Update-MatrixStatus -Status "BLOCKED"
    } else {
      try {
        Invoke-AttachWindowProbe
      } catch {
        Write-BlockedArtifacts -Reason $_.Exception.Message -Phase "attach-window"
        Update-MatrixStatus -Status "BLOCKED"
      }

      if ($script:WebDriverHealth.AttachWindowOk) {
        $diagnostic = Invoke-HotkeyInjectionDiagnostic
        $reason = "physical keyboard verification required; injected SendInput hotkeys are diagnostic-only and cannot authoritatively verify RegisterHotKey behavior in this environment"
        Write-BlockedArtifacts `
          -Reason $reason `
          -Phase "manual-verification-required" `
          -DiagnosticPath $diagnostic.Path `
          -DiagnosticResult $diagnostic.Result
        Update-MatrixStatus -Status "BLOCKED"
      }
    }
  }

  Write-Output ("overall={0}" -f (Get-OverallStatus))
  Write-Output ("webdriver_health={0}" -f $script:WebDriverHealth.HealthMode)
  foreach ($entry in $caseResults.Values) {
    Write-Output ("{0} {1}" -f $entry.CaseId, $entry.Result)
  }
}
finally {
  Restore-AppDataState
  Stop-ViteServer
  Stop-RememberProcesses
  Stop-NotepadProcesses
}
