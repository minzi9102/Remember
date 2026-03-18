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
$wdKeys = @{
  Enter = [string][char]0xE007
  Shift = [string][char]0xE008
  Alt = [string][char]0xE00A
  Escape = [string][char]0xE00C
}
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
    [string]$Phase = "precheck"
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
    Append-Text -TxtPath $txtPath -Line "- visual_evidence: not produced because precheck blocked before case execution"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: BLOCKED"
    Set-CaseResult -CaseId $caseId -Result "BLOCKED" -Evidence "`$txt"
  }
}

function Stop-ProcessSafe {
  param($Process)

  if ($null -ne $Process) {
    try {
      if ($Process -is [System.Diagnostics.Process]) {
        if (-not $Process.HasExited) {
          Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        }
      } elseif ($Process.PSObject.Properties.Name -contains "Id") {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
      }
    } catch {
    }
  }
}

function Stop-RememberProcesses {
  Get-Process -Name "tauri-app" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

function Stop-NotepadProcesses {
  Get-Process -Name "notepad" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
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
  Stop-ViteProcesses
  New-Dir -Path $runtimeLogDir

  $viteOut = Join-Path $runtimeLogDir "p4t1-vite.out.log"
  $viteErr = Join-Path $runtimeLogDir "p4t1-vite.err.log"
  if (Test-Path $viteOut) { Remove-Item $viteOut -Force }
  if (Test-Path $viteErr) { Remove-Item $viteErr -Force }

  $global:ViteProcess = Start-Process `
    -FilePath "npm.cmd" `
    -ArgumentList @("run", "dev", "--", "--host", "127.0.0.1", "--port", "$vitePort") `
    -WorkingDirectory $root `
    -PassThru `
    -RedirectStandardOutput $viteOut `
    -RedirectStandardError $viteErr

  Wait-HttpReady -Url $viteUrl
}

function Stop-ViteServer {
  if ($global:ViteProcess -and -not $global:ViteProcess.HasExited) {
    Stop-Process -Id $global:ViteProcess.Id -Force -ErrorAction SilentlyContinue
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
    if ($ErrorRecord.Exception.Response) {
      $stream = $ErrorRecord.Exception.Response.GetResponseStream()
      if ($stream) {
        $reader = New-Object System.IO.StreamReader($stream)
        $body = $reader.ReadToEnd()
        if (-not [string]::IsNullOrWhiteSpace($body)) {
          return "$message | body: $body"
        }
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

  $actions = [System.Collections.Generic.List[object]]::new()
  foreach ($key in $Keys) {
    $actions.Add(@{ type = "keyDown"; value = $key })
  }
  for ($index = $Keys.Count - 1; $index -ge 0; $index--) {
    $actions.Add(@{ type = "keyUp"; value = $Keys[$index] })
  }

  Invoke-WebDriverActions -SessionId $SessionId -Actions $actions
  Start-Sleep -Milliseconds 500
}

function Send-WebDriverText {
  param(
    [string]$SessionId,
    [string]$Text
  )

  $actions = [System.Collections.Generic.List[object]]::new()
  foreach ($char in $Text.ToCharArray()) {
    $value = [string]$char
    $actions.Add(@{ type = "keyDown"; value = $value })
    $actions.Add(@{ type = "keyUp"; value = $value })
  }

  Invoke-WebDriverActions -SessionId $SessionId -Actions $actions
  Start-Sleep -Milliseconds 300
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
        $lines.Add("{0}: {1}" -f (Split-Path $path -Leaf), $match.Line.Trim())
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
    }
  } catch {
  }
}

function Start-NotepadWindow {
  $process = Start-Process -FilePath "notepad.exe" -PassThru
  $window = Wait-ProcessWindow -ProcessId $process.Id
  Focus-ProcessWindow -Process $window
  return $window
}

function Prepare-CleanEnvironment {
  Stop-RememberProcesses
  Stop-NotepadProcesses
  Stop-ViteServer
  Reset-SqliteDatabase
  Write-AppConfig
  Start-ViteServer
}

function Start-CaseAppWindow {
  param(
    [string]$CaseKey,
    [string[]]$AcceptedTitles
  )

  $app = Start-App -CaseKey $CaseKey
  $window = Wait-WindowTitle -ProcessId $app.Process.Id -AcceptedTitles $AcceptedTitles
  Start-Sleep -Seconds 2

  return [pscustomobject]@{
    Process = $window
    StdoutPath = $app.StdoutPath
    StderrPath = $app.StderrPath
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
  }
}

function Run-VGPassCase {
  $caseId = "P4-T1-VG-PASS"
  $txtPath = (Get-CaseEvidenceBase -CaseId $caseId) + ".txt"
  $pngPath = (Get-CaseEvidenceBase -CaseId $caseId) + ".png"
  Write-CaseHeader -TxtPath $txtPath -CaseId $caseId -TargetMode "desktop_window"

  $rootSession = $null
  $appSession = $null
  $app = $null
  $notepad = $null
  $result = "PASS"
  $note = "global hotkey toggled hidden -> shown -> hidden on the real desktop window"

  try {
    Prepare-CleanEnvironment
    $app = Start-CaseAppWindow -CaseKey "p4t1-vg-pass" -AcceptedTitles @($appTitle)
    $rootSession = Start-RootSession
    $appSession = Start-AttachedWindowSession -WindowHandle $app.Process.MainWindowHandle
    $null = $appSession

    $notepad = Start-NotepadWindow
    Send-WebDriverChord -SessionId $rootSession -Keys @($wdKeys.Alt, " ")
    Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $false | Out-Null

    Focus-ProcessWindow -Process $notepad
    Send-WebDriverChord -SessionId $rootSession -Keys @($wdKeys.Alt, " ")
    $refreshed = Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $true
    Wait-ForegroundHandle -ExpectedHandle $refreshed.MainWindowHandle
    Take-WindowShot -Path $pngPath

    Send-WebDriverChord -SessionId $rootSession -Keys @($wdKeys.Alt, " ")
    Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $false | Out-Null

    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "environment:"
    Append-Text -TxtPath $txtPath -Line "- webdriver_url: $WebDriverUrl"
    Append-Text -TxtPath $txtPath -Line "- vite_url: $viteUrl"
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
    Stop-WebDriverSession -SessionId $appSession
    Stop-WebDriverSession -SessionId $rootSession
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

  $rootSession = $null
  $appSession = $null
  $app = $null
  $notepad = $null
  $recording = $null
  $seriesName = "P4T1 Inbox"
  $commitContent = "p4t1-first-note"
  $result = "PASS"

  try {
    Prepare-CleanEnvironment
    $app = Start-CaseAppWindow -CaseKey "p4t1-ig-pass" -AcceptedTitles @($appTitle)
    $rootSession = Start-RootSession
    $appSession = Start-AttachedWindowSession -WindowHandle $app.Process.MainWindowHandle
    $recording = Start-ScreenRecording -Path $mp4Path

    Focus-ProcessWindow -Process $app.Process
    Send-WebDriverChord -SessionId $rootSession -Keys @($wdKeys.Alt, " ")
    Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $false | Out-Null

    $notepad = Start-NotepadWindow
    Send-WebDriverChord -SessionId $rootSession -Keys @($wdKeys.Alt, " ")
    $refreshed = Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $true
    Wait-ForegroundHandle -ExpectedHandle $refreshed.MainWindowHandle

    Send-WebDriverChord -SessionId $appSession -Keys @($wdKeys.Shift, "n")
    Send-WebDriverText -SessionId $appSession -Text $seriesName
    Send-WebDriverChord -SessionId $appSession -Keys @($wdKeys.Enter)
    Send-WebDriverText -SessionId $appSession -Text $commitContent
    Send-WebDriverChord -SessionId $appSession -Keys @($wdKeys.Enter)
    Start-Sleep -Seconds 1

    $sqliteEvidence = Assert-SqliteSeriesAndCommit -SeriesName $seriesName -CommitContent $commitContent

    Send-WebDriverChord -SessionId $rootSession -Keys @($wdKeys.Alt, " ")
    Wait-WindowVisibility -ProcessId $app.Process.Id -ExpectedVisible $false | Out-Null

    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "environment:"
    Append-Text -TxtPath $txtPath -Line "- webdriver_url: $WebDriverUrl"
    Append-Text -TxtPath $txtPath -Line "- vite_url: $viteUrl"
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
    Stop-WebDriverSession -SessionId $appSession
    Stop-WebDriverSession -SessionId $rootSession
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

  $rootSession = $null
  $app = $null
  $notepad = $null
  $helper = $null
  $recoveryRoot = $null
  $recoveryApp = $null
  $result = "PASS"

  try {
    Prepare-CleanEnvironment
    $helper = Start-HotkeyConflictHelper
    $app = Start-CaseAppWindow -CaseKey "p4t1-vg-fail" -AcceptedTitles @($hotkeyDisabledTitle)
    Wait-LogContains -Paths @($app.StdoutPath, $app.StderrPath, $helper.LogFile) -Pattern "global hotkey disabled"
    $rootSession = Start-RootSession
    $notepad = Start-NotepadWindow
    $beforeHandle = [Win32Native]::GetForegroundWindow()

    Send-WebDriverChord -SessionId $rootSession -Keys @($wdKeys.Alt, " ")
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

    Stop-WebDriverSession -SessionId $rootSession
    $rootSession = $null
    Stop-ProcessSafe -Process $notepad
    $notepad = $null
    Stop-ProcessSafe -Process $app.Process
    $app = $null
    Stop-HotkeyConflictHelper -Helper $helper
    $helper = $null

    Prepare-CleanEnvironment
    $recoveryApp = Start-CaseAppWindow -CaseKey "p4t1-vg-fail-recovery" -AcceptedTitles @($appTitle)
    $recoveryRoot = Start-RootSession
    $notepad = Start-NotepadWindow

    Send-WebDriverChord -SessionId $recoveryRoot -Keys @($wdKeys.Alt, " ")
    Wait-WindowVisibility -ProcessId $recoveryApp.Process.Id -ExpectedVisible $false | Out-Null
    Focus-ProcessWindow -Process $notepad
    Send-WebDriverChord -SessionId $recoveryRoot -Keys @($wdKeys.Alt, " ")
    $recovered = Wait-WindowVisibility -ProcessId $recoveryApp.Process.Id -ExpectedVisible $true
    Wait-ForegroundHandle -ExpectedHandle $recovered.MainWindowHandle

    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    Append-Text -TxtPath $txtPath -Line "- screenshot: $pngPath"
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
    Stop-WebDriverSession -SessionId $rootSession
    Stop-WebDriverSession -SessionId $recoveryRoot
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

  $rootSession = $null
  $app = $null
  $notepad = $null
  $helper = $null
  $recording = $null
  $recoveryRoot = $null
  $recoverySession = $null
  $recoveryApp = $null
  $recoverySeries = "P4T1 Recovery"
  $recoveryCommit = "p4t1-recovery-note"
  $result = "PASS"

  try {
    Prepare-CleanEnvironment
    $helper = Start-HotkeyConflictHelper
    $app = Start-CaseAppWindow -CaseKey "p4t1-ig-fail" -AcceptedTitles @($hotkeyDisabledTitle)
    Wait-LogContains -Paths @($app.StdoutPath, $app.StderrPath, $helper.LogFile) -Pattern "global hotkey disabled"
    $rootSession = Start-RootSession
    $notepad = Start-NotepadWindow
    $recording = Start-ScreenRecording -Path $mp4Path

    for ($index = 0; $index -lt 3; $index++) {
      Send-WebDriverChord -SessionId $rootSession -Keys @($wdKeys.Alt, " ")
      Start-Sleep -Milliseconds 400
    }

    if ([Win32Native]::GetForegroundWindow() -eq $app.Process.MainWindowHandle) {
      Fail-Assert "app unexpectedly became foreground during illegal hotkey spam"
    }

    $logExcerpt = Get-LogExcerpt -Paths @($app.StdoutPath, $app.StderrPath) -Pattern "global hotkey disabled"

    Stop-WebDriverSession -SessionId $rootSession
    $rootSession = $null
    Stop-ProcessSafe -Process $notepad
    $notepad = $null
    Stop-ProcessSafe -Process $app.Process
    $app = $null
    Stop-HotkeyConflictHelper -Helper $helper
    $helper = $null

    Prepare-CleanEnvironment
    $recoveryApp = Start-CaseAppWindow -CaseKey "p4t1-ig-fail-recovery" -AcceptedTitles @($appTitle)
    $recoveryRoot = Start-RootSession
    $recoverySession = Start-AttachedWindowSession -WindowHandle $recoveryApp.Process.MainWindowHandle
    $notepad = Start-NotepadWindow

    Focus-ProcessWindow -Process $recoveryApp.Process
    Send-WebDriverChord -SessionId $recoveryRoot -Keys @($wdKeys.Alt, " ")
    Wait-WindowVisibility -ProcessId $recoveryApp.Process.Id -ExpectedVisible $false | Out-Null
    Focus-ProcessWindow -Process $notepad
    Send-WebDriverChord -SessionId $recoveryRoot -Keys @($wdKeys.Alt, " ")
    $shown = Wait-WindowVisibility -ProcessId $recoveryApp.Process.Id -ExpectedVisible $true
    Wait-ForegroundHandle -ExpectedHandle $shown.MainWindowHandle

    Send-WebDriverChord -SessionId $recoverySession -Keys @($wdKeys.Shift, "n")
    Send-WebDriverText -SessionId $recoverySession -Text $recoverySeries
    Send-WebDriverChord -SessionId $recoverySession -Keys @($wdKeys.Enter)
    Send-WebDriverText -SessionId $recoverySession -Text $recoveryCommit
    Send-WebDriverChord -SessionId $recoverySession -Keys @($wdKeys.Enter)
    Start-Sleep -Seconds 1

    $sqliteEvidence = Assert-SqliteSeriesAndCommit -SeriesName $recoverySeries -CommitContent $recoveryCommit

    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "actual_result:"
    Append-Text -TxtPath $txtPath -Line "- recording: $mp4Path"
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
    Stop-WebDriverSession -SessionId $rootSession
    Stop-WebDriverSession -SessionId $recoverySession
    Stop-WebDriverSession -SessionId $recoveryRoot
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
        foreach ($caseId in $selectedCases) {
          switch ($caseId) {
            "P4-T1-VG-PASS" { Run-VGPassCase }
            "P4-T1-IG-PASS" { Run-IGPassCase }
            "P4-T1-VG-FAIL" { Run-VGFailCase }
            "P4-T1-IG-FAIL" { Run-IGFailCase }
          }
        }
        Update-MatrixStatus -Status (Get-OverallStatus)
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
