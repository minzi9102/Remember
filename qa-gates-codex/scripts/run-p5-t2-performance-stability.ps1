param(
  [ValidateSet("ENV-SQLITE")]
  [string]$EnvId = "ENV-SQLITE",
  [string]$Tester = "codex",
  [int]$SampleCount = 20,
  [int]$RegressionWindow = 1,
  [int]$HotkeyTimeoutMs = 2000,
  [int]$CommitTimeoutMs = 1200,
  [bool]$UpdateState = $true
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
$outputDir = Join-Path $root "qa-gates-codex"
$matrixPath = Join-Path $outputDir "MASTER-TRACE-MATRIX.md"
$taskPath = Join-Path $root "task.jsonl"
$runDate = Get-Date -Format "yyyyMMdd"
$reportPath = Join-Path $outputDir ("P5-T2-PERF-BASELINE_{0}_{1}_{2}.txt" -f $runDate, $EnvId, $Tester)
$logDir = Join-Path $env:TEMP ("p5t2-perf-baseline-" + [guid]::NewGuid().ToString("N"))
$appDataDir = Join-Path $env:APPDATA "com.remember.app"
$configPath = Join-Path $appDataDir "config.toml"
$sqlitePath = Join-Path $appDataDir "remember.sqlite3"
$exePath = Join-Path $root "src-tauri\target\debug\tauri-app.exe"
$pythonExe = Join-Path $root ".venv\Scripts\python.exe"

$hotkeyThresholds = [ordered]@{
  p75 = 250.0
  p95 = 450.0
}
$commitThresholds = [ordered]@{
  p75 = 350.0
  p95 = 800.0
}
$regressionLimitPct = 20.0

Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class Win32PerfNative {
  [StructLayout(LayoutKind.Sequential)]
  public struct INPUT {
    public int type;
    public InputUnion U;
  }

  [StructLayout(LayoutKind.Explicit)]
  public struct InputUnion {
    [FieldOffset(0)]
    public KEYBDINPUT ki;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct KEYBDINPUT {
    public ushort wVk;
    public ushort wScan;
    public uint dwFlags;
    public uint time;
    public IntPtr dwExtraInfo;
  }

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);

  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);

  [DllImport("user32.dll")]
  public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

  [DllImport("user32.dll", SetLastError=true)]
  public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);
}
"@

$script:InputTypeKeyboard = [uint32]1
$script:KeyEventKeyUp = [uint32]0x0002
$vk = @{
  Alt = [uint16]0x12
  Space = [uint16]0x20
}

function New-Dir {
  param([string]$Path)

  if (-not (Test-Path $Path)) {
    New-Item -ItemType Directory -Path $Path | Out-Null
  }
}

function New-KeyInputRecord {
  param(
    [uint16]$VirtualKey,
    [uint32]$Flags
  )

  $input = New-Object Win32PerfNative+INPUT
  $input.type = $script:InputTypeKeyboard
  $input.U.ki.wVk = $VirtualKey
  $input.U.ki.wScan = 0
  $input.U.ki.dwFlags = $Flags
  $input.U.ki.time = 0
  $input.U.ki.dwExtraInfo = [IntPtr]::Zero
  return $input
}

function Send-KeyChordAltSpace {
  $inputs = @(
    (New-KeyInputRecord -VirtualKey $vk.Alt -Flags 0),
    (New-KeyInputRecord -VirtualKey $vk.Space -Flags 0),
    (New-KeyInputRecord -VirtualKey $vk.Space -Flags $script:KeyEventKeyUp),
    (New-KeyInputRecord -VirtualKey $vk.Alt -Flags $script:KeyEventKeyUp)
  )

  $sent = [Win32PerfNative]::SendInput([uint32]$inputs.Length, $inputs, [Runtime.InteropServices.Marshal]::SizeOf([type][Win32PerfNative+INPUT]))
  if ($sent -eq $inputs.Length) {
    return
  }

  # Fallback path for environments where SendInput is blocked by UIPI/integrity constraints.
  try {
    $shell = New-Object -ComObject WScript.Shell
    $shell.SendKeys("% ")
  } catch {
    throw "SendInput sent $sent/$($inputs.Length) events and SendKeys fallback failed: $($_.Exception.Message)"
  }
}

function Get-Percentile {
  param(
    [double[]]$Values,
    [double]$Percentile
  )

  if ($null -eq $Values -or $Values.Count -eq 0) {
    return 0.0
  }

  $sorted = $Values | Sort-Object
  $rank = [Math]::Ceiling(($Percentile / 100.0) * $sorted.Count) - 1
  if ($rank -lt 0) { $rank = 0 }
  if ($rank -ge $sorted.Count) { $rank = $sorted.Count - 1 }
  return [Math]::Round([double]$sorted[$rank], 2)
}

function Get-MaxValue {
  param([double[]]$Values)

  if ($null -eq $Values -or $Values.Count -eq 0) {
    return 0.0
  }

  return [Math]::Round(([double]($Values | Measure-Object -Maximum).Maximum), 2)
}

function Wait-ProcessWindow {
  param(
    [int]$ProcessId,
    [int]$TimeoutMs = 30000
  )

  $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
  while ((Get-Date) -lt $deadline) {
    $proc = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $proc) {
      throw "process $ProcessId exited before window was ready"
    }

    if ($proc.MainWindowHandle -ne 0) {
      return $proc
    }

    Start-Sleep -Milliseconds 200
  }

  throw "timed out waiting for process $ProcessId window"
}

function Wait-WindowVisibility {
  param(
    [IntPtr]$Handle,
    [bool]$ExpectedVisible,
    [int]$TimeoutMs
  )

  $watch = [System.Diagnostics.Stopwatch]::StartNew()
  while ($watch.ElapsedMilliseconds -lt $TimeoutMs) {
    $visible = [bool][Win32PerfNative]::IsWindowVisible($Handle)
    if ($visible -eq $ExpectedVisible) {
      $watch.Stop()
      return [double]$watch.Elapsed.TotalMilliseconds
    }
    Start-Sleep -Milliseconds 10
  }

  $watch.Stop()
  return $null
}

function Stop-ProcessSafe {
  param($Process)

  if ($null -eq $Process) {
    return
  }

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

function Stop-RememberProcesses {
  Get-Process -Name "tauri-app" -ErrorAction SilentlyContinue | ForEach-Object {
    Stop-ProcessSafe -Process $_
  }
}

function Start-App {
  New-Dir -Path $logDir
  $stdoutPath = Join-Path $logDir "tauri-app.out.log"
  $stderrPath = Join-Path $logDir "tauri-app.err.log"
  if (Test-Path $stdoutPath) { Remove-Item $stdoutPath -Force }
  if (Test-Path $stderrPath) { Remove-Item $stderrPath -Force }

  $app = Start-Process -FilePath $exePath -WorkingDirectory (Split-Path $exePath) -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
  return [pscustomobject]@{
    Process = $app
    StdoutPath = $stdoutPath
    StderrPath = $stderrPath
  }
}

function Ensure-Config {
  New-Dir -Path $appDataDir
  $lines = @(
    'runtime_mode = "sqlite_only"',
    'hotkey = "Alt+Space"',
    'silent_days_threshold = 7'
  )
  Set-Content -Path $configPath -Value $lines -Encoding utf8
}

function Measure-HotkeyLatencies {
  param(
    [System.Diagnostics.Process]$AppProcess,
    [int]$Samples,
    [int]$TimeoutMs
  )

  $latencies = [System.Collections.Generic.List[double]]::new()
  $timeouts = 0
  $crashes = 0

  $appHandle = $AppProcess.MainWindowHandle
  [Win32PerfNative]::ShowWindow($appHandle, 5) | Out-Null

  for ($index = 0; $index -lt $Samples; $index++) {
    $proc = Get-Process -Id $AppProcess.Id -ErrorAction SilentlyContinue
    if ($null -eq $proc) {
      $crashes++
      break
    }

    [Win32PerfNative]::SetForegroundWindow($appHandle) | Out-Null
    Start-Sleep -Milliseconds 120

    Send-KeyChordAltSpace
    $hideMs = Wait-WindowVisibility -Handle $appHandle -ExpectedVisible $false -TimeoutMs $TimeoutMs
    if ($null -eq $hideMs) {
      $timeouts++
      [Win32PerfNative]::ShowWindow($appHandle, 0) | Out-Null
    } else {
      $latencies.Add([Math]::Round($hideMs, 2))
    }

    Start-Sleep -Milliseconds 120

    Send-KeyChordAltSpace
    $showMs = Wait-WindowVisibility -Handle $appHandle -ExpectedVisible $true -TimeoutMs $TimeoutMs
    if ($null -eq $showMs) {
      $timeouts++
      [Win32PerfNative]::ShowWindow($appHandle, 5) | Out-Null
    } else {
      $latencies.Add([Math]::Round($showMs, 2))
    }

    Start-Sleep -Milliseconds 120
  }

  return [pscustomobject]@{
    latencies = @($latencies.ToArray())
    timeoutCount = $timeouts
    crashCount = $crashes
    expectedEvents = $Samples * 2
  }
}

function Invoke-PythonJson {
  param(
    [string]$Code,
    [string[]]$Arguments
  )

  $tmpScript = Join-Path $logDir ("py-" + [guid]::NewGuid().ToString("N") + ".py")
  [System.IO.File]::WriteAllText($tmpScript, $Code, (New-Object System.Text.UTF8Encoding($false)))

  try {
    $raw = & $pythonExe $tmpScript @Arguments
    if ($LASTEXITCODE -ne 0) {
      throw "python exited with $LASTEXITCODE"
    }

    $jsonText = ($raw | Out-String).Trim()
    if ([string]::IsNullOrWhiteSpace($jsonText)) {
      throw "python returned empty output"
    }
    return $jsonText | ConvertFrom-Json
  } finally {
    if (Test-Path $tmpScript) {
      Remove-Item $tmpScript -Force -ErrorAction SilentlyContinue
    }
  }
}

function Measure-CommitLatencies {
  param(
    [string]$DbPath,
    [int]$Samples,
    [int]$TimeoutMs
  )

  $code = @"
import datetime
import json
import sqlite3
import sys
import time
import uuid

path = sys.argv[1]
samples = int(sys.argv[2])
timeout_ms = int(sys.argv[3])

conn = sqlite3.connect(path, timeout=5.0, isolation_level=None)
conn.execute("PRAGMA foreign_keys = ON")

def now_iso():
    return datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")

latencies = []
timeout_count = 0
failure_count = 0
messages = []

series_id = str(uuid.uuid4())
created_at = now_iso()
conn.execute(
    "INSERT INTO series (id, name, status, latest_excerpt, last_updated_at, created_at, archived_at) VALUES (?, ?, 'active', '', ?, ?, NULL)",
    (series_id, f"perf-baseline-{series_id[:8]}", created_at, created_at),
)

for idx in range(samples):
    commit_id = str(uuid.uuid4())
    content = f"perf-sample-{idx + 1}"
    ts = now_iso()
    started = time.perf_counter_ns()
    try:
        conn.execute("BEGIN IMMEDIATE")
        conn.execute(
            "INSERT INTO commits (id, series_id, content, created_at) VALUES (?, ?, ?, ?)",
            (commit_id, series_id, content, ts),
        )
        conn.execute(
            "UPDATE series SET latest_excerpt = ?, last_updated_at = ? WHERE id = ?",
            (content[:200], ts, series_id),
        )
        conn.execute("COMMIT")
        elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
        latencies.append(round(elapsed_ms, 2))
        if elapsed_ms > timeout_ms:
            timeout_count += 1
    except Exception as exc:
        failure_count += 1
        messages.append(str(exc))
        conn.execute("ROLLBACK")

print(json.dumps({
    "latencies": latencies,
    "timeoutCount": timeout_count,
    "failureCount": failure_count,
    "messages": messages,
}))
"@

  return Invoke-PythonJson -Code $code -Arguments @($DbPath, "$Samples", "$TimeoutMs")
}

function Get-PreviousBaselineMetrics {
  param(
    [string]$OutputDir,
    [string]$CurrentReportPath,
    [int]$Window
  )

  $pattern = "P5-T2-PERF-BASELINE_*_ENV-SQLITE_*.txt"
  $reports = Get-ChildItem -Path $OutputDir -Filter $pattern -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -ne $CurrentReportPath } |
    Sort-Object LastWriteTime -Descending

  if ($null -eq $reports -or $reports.Count -eq 0) {
    return $null
  }

  $selected = $reports | Select-Object -First ([Math]::Max(1, $Window)) | Select-Object -First 1
  $text = Get-Content $selected.FullName -Raw

  function Get-Metric([string]$content, [string]$metric) {
    $match = [regex]::Match($content, "^" + [regex]::Escape($metric) + ":\s*([0-9]+(?:\.[0-9]+)?)", [System.Text.RegularExpressions.RegexOptions]::Multiline)
    if ($match.Success) {
      return [double]$match.Groups[1].Value
    }
    return $null
  }

  $hotkeyP75 = Get-Metric $text "hotkey_p75_ms"
  $hotkeyP95 = Get-Metric $text "hotkey_p95_ms"
  $commitP75 = Get-Metric $text "commit_p75_ms"
  $commitP95 = Get-Metric $text "commit_p95_ms"

  if ($null -eq $hotkeyP75 -or $null -eq $hotkeyP95 -or $null -eq $commitP75 -or $null -eq $commitP95) {
    return $null
  }

  return [pscustomobject]@{
    path = $selected.FullName
    hotkeyP75 = [double]$hotkeyP75
    hotkeyP95 = [double]$hotkeyP95
    commitP75 = [double]$commitP75
    commitP95 = [double]$commitP95
  }
}

function Get-DeltaPct {
  param(
    [double]$Current,
    [double]$Previous
  )

  if ($Previous -le 0) {
    return 0.0
  }

  return [Math]::Round((($Current - $Previous) / $Previous) * 100.0, 2)
}

function Update-MatrixStatus {
  param([string]$Status)

  $content = [System.IO.File]::ReadAllText($matrixPath, [System.Text.Encoding]::UTF8)
  $targetLine = "| P5-T2 | ``phase-5/p5-t2-performance-stability.md`` | 5 | 4 | {0} |" -f $Status

  $updated = [regex]::Replace(
    $content,
    '\| P5-T2 \| `phase-5/p5-t2-performance-stability\.md` \| 5 \| 4 \| (TODO|RUNNING|PASS|FAIL|BLOCKED) \|',
    [System.Text.RegularExpressions.MatchEvaluator]{ param($m) $targetLine }
  )

  if ($updated -eq $content) {
    $updated = [regex]::Replace(
      $content,
      '\| P5-T3 \| `phase-5/p5-t3-performance-stability\.md` \| 5 \| 4 \| (TODO|RUNNING|PASS|FAIL|BLOCKED) \|',
      [System.Text.RegularExpressions.MatchEvaluator]{ param($m) $targetLine }
    )
  }

  if (-not ([regex]::IsMatch($updated, '\| P5-T2 \| `phase-5/p5-t2-performance-stability\.md` \| 5 \| 4 \| (TODO|RUNNING|PASS|FAIL|BLOCKED) \|'))) {
    throw "failed to update P5-T2 status in MASTER-TRACE-MATRIX.md"
  }

  [System.IO.File]::WriteAllText($matrixPath, $updated, (New-Object System.Text.UTF8Encoding($false)))
}

function Update-TaskStatus {
  param([bool]$Completed)

  $lines = Get-Content $taskPath
  $next = [System.Collections.Generic.List[string]]::new()

  foreach ($line in $lines) {
    if ([string]::IsNullOrWhiteSpace($line)) {
      continue
    }

    $obj = $line | ConvertFrom-Json
    if ($obj.task_name -like "P5-T2 *") {
      $obj.completed = $Completed
    }
    $next.Add(($obj | ConvertTo-Json -Compress))
  }

  [System.IO.File]::WriteAllLines($taskPath, $next, (New-Object System.Text.UTF8Encoding($false)))
}

function New-PrecheckResult {
  param(
    [string]$Id,
    [string]$Result,
    [string]$Note
  )

  return [pscustomobject]@{
    Id = $Id
    Result = $Result
    Note = $Note
  }
}

New-Dir -Path $outputDir
New-Dir -Path $logDir

$prechecks = @()
$prechecks += if (Test-Path $exePath) { New-PrecheckResult -Id "tauri_executable" -Result "PASS" -Note $exePath } else { New-PrecheckResult -Id "tauri_executable" -Result "FAIL" -Note "missing $exePath" }
$prechecks += if (Test-Path $pythonExe) { New-PrecheckResult -Id "uv_python" -Result "PASS" -Note $pythonExe } else { New-PrecheckResult -Id "uv_python" -Result "FAIL" -Note "missing $pythonExe" }
$prechecks += if (Test-Path $matrixPath) { New-PrecheckResult -Id "trace_matrix" -Result "PASS" -Note $matrixPath } else { New-PrecheckResult -Id "trace_matrix" -Result "FAIL" -Note "missing $matrixPath" }
$prechecks += if (Test-Path $taskPath) { New-PrecheckResult -Id "task_jsonl" -Result "PASS" -Note $taskPath } else { New-PrecheckResult -Id "task_jsonl" -Result "FAIL" -Note "missing $taskPath" }

$overall = "FAIL"
$hotkeyResult = $null
$commitResult = $null
$previousBaseline = $null
$appCrashCount = 0
$logPaths = @()

if (($prechecks | Where-Object { $_.Result -eq "FAIL" }).Count -eq 0) {
  $app = $null
  try {
    Ensure-Config
    Stop-RememberProcesses

    $app = Start-App
    $logPaths += $app.StdoutPath
    $logPaths += $app.StderrPath

    $appProc = Wait-ProcessWindow -ProcessId $app.Process.Id -TimeoutMs 30000
    Start-Sleep -Milliseconds 1200

    $logText = ""
    if (Test-Path $app.StdoutPath) { $logText += (Get-Content $app.StdoutPath -Raw) }
    if (Test-Path $app.StderrPath) { $logText += "`n" + (Get-Content $app.StderrPath -Raw) }
    if ($logText -match "global hotkey disabled") {
      throw "global hotkey disabled detected in startup logs"
    }

    $hotkeyResult = Measure-HotkeyLatencies -AppProcess $appProc -Samples $SampleCount -TimeoutMs $HotkeyTimeoutMs

    $alive = Get-Process -Id $app.Process.Id -ErrorAction SilentlyContinue
    if ($null -eq $alive) {
      $appCrashCount = 1
    }

    Stop-ProcessSafe -Process $app.Process
    Start-Sleep -Milliseconds 500

    if (-not (Test-Path $sqlitePath)) {
      throw "sqlite database not found at $sqlitePath"
    }

    $commitResult = Measure-CommitLatencies -DbPath $sqlitePath -Samples $SampleCount -TimeoutMs $CommitTimeoutMs
    $previousBaseline = Get-PreviousBaselineMetrics -OutputDir $outputDir -CurrentReportPath $reportPath -Window $RegressionWindow
  } finally {
    if ($null -ne $app) {
      Stop-ProcessSafe -Process $app.Process
    }
  }
}

$hotkeyLatencies = if ($null -ne $hotkeyResult) { [double[]]$hotkeyResult.latencies } else { @() }
$commitLatencies = if ($null -ne $commitResult) { [double[]]$commitResult.latencies } else { @() }

$hotkeyP75 = Get-Percentile -Values $hotkeyLatencies -Percentile 75
$hotkeyP95 = Get-Percentile -Values $hotkeyLatencies -Percentile 95
$hotkeyMax = Get-MaxValue -Values $hotkeyLatencies
$commitP75 = Get-Percentile -Values $commitLatencies -Percentile 75
$commitP95 = Get-Percentile -Values $commitLatencies -Percentile 95
$commitMax = Get-MaxValue -Values $commitLatencies

$hotkeyTimeoutCount = if ($null -ne $hotkeyResult) { [int]$hotkeyResult.timeoutCount } else { 0 }
$commitTimeoutCount = if ($null -ne $commitResult) { [int]$commitResult.timeoutCount } else { 0 }
$commitFailureCount = if ($null -ne $commitResult) { [int]$commitResult.failureCount } else { 0 }

$expectedHotkeyEvents = if ($null -ne $hotkeyResult) { [int]$hotkeyResult.expectedEvents } else { $SampleCount * 2 }
$observedHotkeyEvents = $hotkeyLatencies.Count
$observedCommitEvents = $commitLatencies.Count

$totalExpected = $expectedHotkeyEvents + $SampleCount
$totalObserved = $observedHotkeyEvents + $observedCommitEvents
$totalTimeouts = $hotkeyTimeoutCount + $commitTimeoutCount
$passRate = if ($totalExpected -gt 0) { [Math]::Round(($totalObserved / $totalExpected) * 100.0, 2) } else { 0.0 }

$hotkeyGate = $false
$commitGate = $false
$stabilityGate = $false
$regressionGate = $true

$hotkeyGate = (
  $hotkeyP75 -le $hotkeyThresholds.p75 -and
  $hotkeyP95 -le $hotkeyThresholds.p95 -and
  $observedHotkeyEvents -ge [Math]::Ceiling($expectedHotkeyEvents * 0.9) -and
  $hotkeyTimeoutCount -le [Math]::Max(1, [Math]::Floor($expectedHotkeyEvents * 0.1))
)

$commitGate = (
  $commitP75 -le $commitThresholds.p75 -and
  $commitP95 -le $commitThresholds.p95 -and
  $commitFailureCount -eq 0 -and
  $observedCommitEvents -eq $SampleCount
)

$stabilityGate = (
  $appCrashCount -eq 0 -and
  $passRate -ge 95.0 -and
  $totalTimeouts -le [Math]::Max(2, [Math]::Floor($totalExpected * 0.05))
)

$regressionDelta = [ordered]@{}
$regressionSource = "N/A(first-baseline)"
if ($null -ne $previousBaseline) {
  $regressionSource = $previousBaseline.path
  $regressionDelta.hotkey_p75_delta_pct = Get-DeltaPct -Current $hotkeyP75 -Previous $previousBaseline.hotkeyP75
  $regressionDelta.hotkey_p95_delta_pct = Get-DeltaPct -Current $hotkeyP95 -Previous $previousBaseline.hotkeyP95
  $regressionDelta.commit_p75_delta_pct = Get-DeltaPct -Current $commitP75 -Previous $previousBaseline.commitP75
  $regressionDelta.commit_p95_delta_pct = Get-DeltaPct -Current $commitP95 -Previous $previousBaseline.commitP95

  foreach ($value in $regressionDelta.Values) {
    if ($value -gt $regressionLimitPct) {
      $regressionGate = $false
      break
    }
  }
} else {
  $regressionDelta.hotkey_p75_delta_pct = 0.0
  $regressionDelta.hotkey_p95_delta_pct = 0.0
  $regressionDelta.commit_p75_delta_pct = 0.0
  $regressionDelta.commit_p95_delta_pct = 0.0
}

if (
  ($prechecks | Where-Object { $_.Result -eq "FAIL" }).Count -eq 0 -and
  $hotkeyGate -and
  $commitGate -and
  $stabilityGate -and
  $regressionGate
) {
  $overall = "PASS"
}

$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add("task_id: P5-T2")
$lines.Add("task_name: performance and stability baseline")
$lines.Add("target_mode: automated_performance_stability_baseline")
$lines.Add("env_id: $EnvId")
$lines.Add("runtime_mode: sqlite_only")
$lines.Add("run_date: $runDate")
$lines.Add("tester: $Tester")
$lines.Add("sample_count: $SampleCount")
$lines.Add("regression_window: $RegressionWindow")
$lines.Add("script: qa-gates-codex/scripts/run-p5-t2-performance-stability.ps1")
$lines.Add("threshold_profile: balanced")
$lines.Add("")
$lines.Add("prechecks:")
foreach ($check in $prechecks) {
  $lines.Add("- id: $($check.Id)")
  $lines.Add("  result: $($check.Result)")
  $lines.Add("  note: $($check.Note)")
}
$lines.Add("")
$lines.Add("hotkey_latency_ms:")
$lines.Add("hotkey_sample_events: $expectedHotkeyEvents")
$lines.Add("hotkey_observed_events: $observedHotkeyEvents")
$lines.Add("hotkey_p75_ms: $hotkeyP75")
$lines.Add("hotkey_p95_ms: $hotkeyP95")
$lines.Add("hotkey_max_ms: $hotkeyMax")
$lines.Add("hotkey_timeout_count: $hotkeyTimeoutCount")
$lines.Add("")
$lines.Add("commit_latency_ms:")
$lines.Add("commit_sample_events: $SampleCount")
$lines.Add("commit_observed_events: $observedCommitEvents")
$lines.Add("commit_p75_ms: $commitP75")
$lines.Add("commit_p95_ms: $commitP95")
$lines.Add("commit_max_ms: $commitMax")
$lines.Add("commit_timeout_count: $commitTimeoutCount")
$lines.Add("commit_failure_count: $commitFailureCount")
if ($null -ne $commitResult -and $commitResult.messages.Count -gt 0) {
  foreach ($msg in $commitResult.messages) {
    $lines.Add("commit_failure_message: $msg")
  }
}
$lines.Add("")
$lines.Add("stability:")
$lines.Add("pass_rate_pct: $passRate")
$lines.Add("crash_count: $appCrashCount")
$lines.Add("timeout_count: $totalTimeouts")
$lines.Add("")
$lines.Add("regression_delta:")
$lines.Add("previous_report: $regressionSource")
$lines.Add("hotkey_p75_delta_pct: $($regressionDelta.hotkey_p75_delta_pct)")
$lines.Add("hotkey_p95_delta_pct: $($regressionDelta.hotkey_p95_delta_pct)")
$lines.Add("commit_p75_delta_pct: $($regressionDelta.commit_p75_delta_pct)")
$lines.Add("commit_p95_delta_pct: $($regressionDelta.commit_p95_delta_pct)")
$lines.Add("")
$lines.Add("gates:")
$lines.Add("hotkey_gate: $(if($hotkeyGate){'PASS'}else{'FAIL'})")
$lines.Add("commit_gate: $(if($commitGate){'PASS'}else{'FAIL'})")
$lines.Add("stability_gate: $(if($stabilityGate){'PASS'}else{'FAIL'})")
$lines.Add("regression_gate: $(if($regressionGate){'PASS'}else{'FAIL'})")
$lines.Add("")
$lines.Add("logs:")
foreach ($path in $logPaths) {
  $lines.Add("- $path")
}
$lines.Add("log_dir: $logDir")
$lines.Add("")
$lines.Add("conclusion: $overall")
$lines.Add("overall: $overall")

[System.IO.File]::WriteAllLines($reportPath, $lines, (New-Object System.Text.UTF8Encoding($false)))

if ($UpdateState) {
  if ($overall -eq "PASS") {
    Update-MatrixStatus -Status "PASS"
    Update-TaskStatus -Completed $true
  } else {
    Update-MatrixStatus -Status "FAIL"
  }
}

Write-Output ("report_path={0}" -f $reportPath)
Write-Output ("log_dir={0}" -f $logDir)
Write-Output ("overall={0}" -f $overall)

if ($overall -ne "PASS") {
  exit 1
}
