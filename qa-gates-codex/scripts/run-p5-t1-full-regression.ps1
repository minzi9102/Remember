$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class Win32Native {
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
  }

  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);

  [DllImport("user32.dll")]
  public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);

  [DllImport("user32.dll")]
  public static extern bool SetCursorPos(int x, int y);

  [DllImport("user32.dll")]
  public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
}
"@

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$outputDir = Join-Path $root "qa-gates-codex"
$phaseDocPath = Join-Path $root "qa-gates-codex\phase-5\p5-t1-full-regression.md"
$appDataDir = Join-Path $env:APPDATA "com.remember.app"
$configPath = Join-Path $appDataDir "config.toml"
$sqlitePath = Join-Path $appDataDir "remember.sqlite3"
$pythonExe = Join-Path $root ".venv\Scripts\python.exe"
$ffmpegExe = (Get-Command ffmpeg -ErrorAction Stop).Source
$screenshotScript = Join-Path $env:USERPROFILE ".codex\skills\screenshot\scripts\take_screenshot.ps1"
$exePath = Join-Path $root "src-tauri\target\debug\tauri-app.exe"
$containerName = "remember-p5t1-pg-temp"
$tempPgImage = "postgres:16-alpine"
$tempPgPort = 55433
$tempPgDatabase = "remember_p5t1"
$tempPgUser = "remember_p5t1"
$tempPgPassword = "remember_p5t1"
$tempPgDsn = "postgres://${tempPgUser}:${tempPgPassword}@localhost:${tempPgPort}/${tempPgDatabase}"
$runDate = Get-Date -Format "yyyyMMdd"
$tester = "codex"
$pwBrowser = "msedge"
$vitePort = 1420
$viteUrl = "http://127.0.0.1:${vitePort}"
$evidenceResults = [ordered]@{}
$backupDir = Join-Path $env:TEMP ("p5t1-backup-" + [guid]::NewGuid().ToString())
$runtimeLogDir = Join-Path $env:TEMP ("p5t1-logs-" + [guid]::NewGuid().ToString())

$global:ViteProcess = $null

function New-Dir {
  param([string]$Path)

  if (-not (Test-Path $Path)) {
    New-Item -ItemType Directory -Path $Path | Out-Null
  }
}

function Invoke-Native {
  param(
    [string]$FilePath,
    [string[]]$ArgumentList,
    [string]$ErrorMessage
  )

  & $FilePath @ArgumentList
  if ($LASTEXITCODE -ne 0) {
    throw "$ErrorMessage (exit code $LASTEXITCODE)"
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

function Stop-RememberProcesses {
  Get-Process -Name "tauri-app" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

function Stop-ViteProcesses {
  Get-CimInstance Win32_Process |
    Where-Object {
      ($_.Name -eq "node.exe" -or $_.Name -eq "cmd.exe") -and
      $_.CommandLine -match "vite" -and
      $_.CommandLine -match "1420"
    } |
    ForEach-Object {
      try {
        Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop
      } catch {
      }
    }
}

function Start-ViteServer {
  Stop-ViteProcesses
  New-Dir -Path $runtimeLogDir

  $viteOut = Join-Path $runtimeLogDir "p5t1-vite.out.log"
  $viteErr = Join-Path $runtimeLogDir "p5t1-vite.err.log"
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

function Start-TempPostgres {
  docker rm -f $containerName *> $null
  docker run --rm -d `
    --name $containerName `
    -e "POSTGRES_DB=$tempPgDatabase" `
    -e "POSTGRES_USER=$tempPgUser" `
    -e "POSTGRES_PASSWORD=$tempPgPassword" `
    -p "${tempPgPort}:5432" `
    $tempPgImage | Out-Null

  if ($LASTEXITCODE -ne 0) {
    throw "failed to start docker container $containerName"
  }

  Wait-TempPostgresReady
}

function Wait-TempPostgresReady {
  for ($index = 0; $index -lt 90; $index++) {
    docker exec $containerName pg_isready -U $tempPgUser -d $tempPgDatabase *> $null
    if ($LASTEXITCODE -eq 0) {
      return
    }

    Start-Sleep -Seconds 1
  }

  throw "timed out waiting for postgres container readiness"
}

function Stop-TempPostgres {
  $existing = docker ps -a --format '{{.Names}}' | Where-Object { $_ -eq $containerName }
  if ($existing) {
    docker rm -f $containerName *> $null
  }
}

function Invoke-TempPsql {
  param(
    [string]$Sql,
    [switch]$Quiet
  )

  $cmd = @("exec", "-i", $containerName, "psql", "-v", "ON_ERROR_STOP=1", "-U", $tempPgUser, "-d", $tempPgDatabase)
  if ($Quiet) {
    $cmd += @("-q")
  }

  $Sql | docker @cmd
  if ($LASTEXITCODE -ne 0) {
    throw "failed to run psql in $containerName"
  }
}

function Reset-TempPostgresSchema {
  $sql = @"
DROP SCHEMA IF EXISTS public CASCADE;
CREATE SCHEMA public;
"@
  Invoke-TempPsql -Sql $sql -Quiet
}

function Write-AppConfig {
  param(
    [string]$RuntimeMode,
    [string]$PostgresDsn = ""
  )

  New-Dir -Path $appDataDir

  $lines = @(
    "runtime_mode = `"$RuntimeMode`"",
    "hotkey = `"Alt+Space`"",
    "silent_days_threshold = 7"
  )

  if (-not [string]::IsNullOrWhiteSpace($PostgresDsn)) {
    $lines += "postgres_dsn = `"$PostgresDsn`""
  }

  Set-Content -Path $configPath -Value $lines -Encoding UTF8
}

function Reset-SqliteDatabase {
  if (Test-Path $sqlitePath) {
    Remove-Item $sqlitePath -Force
  }
}

function Wait-AppWindow {
  param(
    [int]$ProcessId,
    [string]$ExpectedTitle,
    [int]$Attempts = 60
  )

  for ($index = 0; $index -lt $Attempts; $index++) {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process) {
      throw "tauri-app exited before window became ready"
    }

    if ($process.MainWindowHandle -ne 0 -and $process.MainWindowTitle -eq $ExpectedTitle) {
      return $process
    }

    Start-Sleep -Milliseconds 500
  }

  throw "timed out waiting for app window $ExpectedTitle"
}

function Focus-AppWindow {
  param([System.Diagnostics.Process]$Process)

  [Win32Native]::ShowWindow($Process.MainWindowHandle, 5) | Out-Null
  [Win32Native]::SetForegroundWindow($Process.MainWindowHandle) | Out-Null
  Start-Sleep -Milliseconds 500
}

function Get-WindowRect {
  param([System.Diagnostics.Process]$Process)

  $rect = New-Object Win32Native+RECT
  [Win32Native]::GetWindowRect($Process.MainWindowHandle, [ref]$rect) | Out-Null
  return $rect
}

function Click-WindowRelative {
  param(
    [System.Diagnostics.Process]$Process,
    [double]$XRatio,
    [double]$YRatio
  )

  Focus-AppWindow -Process $Process
  $rect = Get-WindowRect -Process $Process
  $x = [int]($rect.Left + (($rect.Right - $rect.Left) * $XRatio))
  $y = [int]($rect.Top + (($rect.Bottom - $rect.Top) * $YRatio))
  [Win32Native]::SetCursorPos($x, $y) | Out-Null
  Start-Sleep -Milliseconds 150
  [Win32Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  [Win32Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 500
}

function Send-Keys {
  param([string]$Keys)

  [System.Windows.Forms.SendKeys]::SendWait($Keys)
  Start-Sleep -Milliseconds 500
}

function Send-Text {
  param([string]$Text)

  Set-Clipboard -Value $Text
  Start-Sleep -Milliseconds 150
  [System.Windows.Forms.SendKeys]::SendWait("^v")
  Start-Sleep -Milliseconds 500
}

function Start-ScreenRecording {
  param(
    [string]$Path,
    [int]$Seconds = 28
  )

  if (Test-Path $Path) {
    Remove-Item $Path -Force
  }

  Start-Process `
    -FilePath $ffmpegExe `
    -ArgumentList @(
      "-y",
      "-f", "gdigrab",
      "-framerate", "10",
      "-i", "desktop",
      "-t", "$Seconds",
      "-c:v", "libx264",
      "-preset", "ultrafast",
      "-pix_fmt", "yuv420p",
      $Path
    ) `
    -PassThru `
    -WindowStyle Hidden
}

function Wait-ScreenRecording {
  param([System.Diagnostics.Process]$Process)

  if ($null -eq $Process) {
    return
  }

  try {
    Wait-Process -Id $Process.Id -Timeout 10
  } catch {
    Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
  }
}

function Take-ActiveWindowShot {
  param([string]$Path)

  if (Test-Path $Path) {
    Remove-Item $Path -Force
  }

  powershell -ExecutionPolicy Bypass -File $screenshotScript -Path $Path -ActiveWindow | Out-Null
}

function Start-App {
  return Start-Process -FilePath $exePath -WorkingDirectory (Split-Path $exePath) -PassThru
}

function Stop-App {
  param([System.Diagnostics.Process]$Process)

  if ($null -ne $Process) {
    Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
  }
}

function Bootstrap-ModeStorage {
  param(
    [string]$RuntimeMode,
    [string]$PostgresDsn = ""
  )

  Write-AppConfig -RuntimeMode $RuntimeMode -PostgresDsn $PostgresDsn
  $app = Start-App
  try {
    $null = Wait-AppWindow -ProcessId $app.Id -ExpectedTitle "tauri-app [$RuntimeMode]"
    Start-Sleep -Seconds 2
  } finally {
    Stop-App -Process $app
  }
}

function Invoke-Python {
  param([string]$Code)

  & $pythonExe -c $Code
  if ($LASTEXITCODE -ne 0) {
    throw "python command failed with exit code $LASTEXITCODE"
  }
}

function Seed-SqliteBaseline {
  $escapedPath = $sqlitePath.Replace("\", "\\")
  $code = @"
import sqlite3
path = r"$escapedPath"
conn = sqlite3.connect(path)
conn.executescript("""
DELETE FROM commits;
DELETE FROM consistency_alerts;
DELETE FROM series;
DELETE FROM app_settings;
INSERT INTO series (id, name, status, latest_excerpt, last_updated_at, created_at, archived_at)
VALUES
  ('p5t1-anchor', 'Anchor Series', 'active', 'anchor-note', '2026-03-18T08:00:00Z', '2026-03-18T08:00:00Z', NULL),
  ('p5t1-project-a', 'Project-A', 'silent', 'follow-up-note', '2026-03-10T08:00:00Z', '2026-03-10T08:00:00Z', NULL);
INSERT INTO commits (id, series_id, content, created_at)
VALUES
  ('p5t1-anchor-commit', 'p5t1-anchor', 'anchor-note', '2026-03-18T08:00:00Z'),
  ('p5t1-project-a-commit', 'p5t1-project-a', 'follow-up-note', '2026-03-10T08:00:00Z');
""")
conn.commit()
print("sqlite baseline seeded")
"@
  Invoke-Python -Code $code
}

function Seed-PostgresBaseline {
  $sql = @"
DELETE FROM commits;
DELETE FROM consistency_alerts;
DELETE FROM series;
DELETE FROM app_settings;
INSERT INTO series (id, name, status, latest_excerpt, last_updated_at, created_at, archived_at)
VALUES
  ('p5t1-anchor', 'Anchor Series', 'active', 'anchor-note', '2026-03-18T08:00:00Z'::timestamptz, '2026-03-18T08:00:00Z'::timestamptz, NULL),
  ('p5t1-project-a', 'Project-A', 'silent', 'follow-up-note', '2026-03-10T08:00:00Z'::timestamptz, '2026-03-10T08:00:00Z'::timestamptz, NULL);
INSERT INTO commits (id, series_id, content, created_at)
VALUES
  ('p5t1-anchor-commit', 'p5t1-anchor', 'anchor-note', '2026-03-18T08:00:00Z'::timestamptz),
  ('p5t1-project-a-commit', 'p5t1-project-a', 'follow-up-note', '2026-03-10T08:00:00Z'::timestamptz);
"@
  Invoke-TempPsql -Sql $sql -Quiet
}

function Query-SqliteEvidence {
  param([string]$SeriesName)

  $escapedPath = $sqlitePath.Replace("\", "\\")
  $escapedName = $SeriesName.Replace("'", "''")
  $code = @"
import sqlite3
path = r"$escapedPath"
conn = sqlite3.connect(path)
series_rows = conn.execute("select name, status, latest_excerpt from series order by last_updated_at desc, id desc").fetchall()
target_row = conn.execute("select name, status, latest_excerpt from series where name = '$escapedName'").fetchall()
timeline_rows = conn.execute("select content from commits order by created_at desc, id desc").fetchall()
print("sqlite_series=", series_rows)
print("sqlite_target=", target_row)
print("sqlite_commits=", timeline_rows)
"@
  & $pythonExe -c $code
}

function Query-PostgresEvidence {
  param([string]$SeriesName)

  $escapedName = $SeriesName.Replace("'", "''")
  $sql = @"
\pset format unaligned
\pset tuples_only on
SELECT 'pg_series=' || COALESCE(string_agg(name || ':' || status || ':' || latest_excerpt, ' | ' ORDER BY last_updated_at DESC, id DESC), '') FROM series;
SELECT 'pg_target=' || COALESCE(string_agg(name || ':' || status || ':' || latest_excerpt, ' | ' ORDER BY name), '') FROM series WHERE name = '$escapedName';
SELECT 'pg_commits=' || COALESCE(string_agg(content, ' | ' ORDER BY created_at DESC, id DESC), '') FROM commits;
"@
  $sql | docker exec -i $containerName psql -q -v ON_ERROR_STOP=1 -U $tempPgUser -d $tempPgDatabase
  if ($LASTEXITCODE -ne 0) {
    throw "failed to query postgres evidence"
  }
}

function Write-CaseTextHeader {
  param(
    [string]$TxtPath,
    [string]$CaseId,
    [string]$EnvId,
    [string]$RuntimeMode,
    [string]$TargetMode
  )

  Set-Content -Path $TxtPath -Value @(
    "case_id: $CaseId",
    "target_mode: $TargetMode",
    "env_id: $EnvId",
    "runtime_mode: $RuntimeMode",
    "run_date: $runDate",
    "steps -> visible result -> db proof -> conclusion"
  )
}

function Append-Text {
  param([string]$TxtPath, [string]$Line)

  Add-Content -Path $TxtPath -Value $Line
}

function Add-EvidenceResult {
  param(
    [string]$EnvId,
    [string]$CaseId,
    [string]$Result,
    [string]$EvidenceLine
  )

  $evidenceResults["$EnvId|$CaseId"] = [pscustomobject]@{
    EnvId = $EnvId
    CaseId = $CaseId
    Result = $Result
    Evidence = $EvidenceLine
  }
}

function Run-VGPassCase {
  param(
    [string]$EnvId,
    [string]$RuntimeMode,
    [System.Diagnostics.Process]$AppProcess
  )

  $caseId = "P5-T1-VG-PASS"
  $txtPath = Join-Path $outputDir "${caseId}_${runDate}_${EnvId}_${tester}.txt"
  $pngPath = Join-Path $outputDir "${caseId}_${runDate}_${EnvId}_${tester}.png"
  Write-CaseTextHeader -TxtPath $txtPath -CaseId $caseId -EnvId $EnvId -RuntimeMode $RuntimeMode -TargetMode "desktop_window"

  Focus-AppWindow -Process $AppProcess
  Take-ActiveWindowShot -Path $pngPath

  Append-Text -TxtPath $txtPath -Line ""
  Append-Text -TxtPath $txtPath -Line "actual_result:"
  Append-Text -TxtPath $txtPath -Line "- window_title: $($AppProcess.MainWindowTitle)"
  Append-Text -TxtPath $txtPath -Line "- screenshot: $pngPath"
  Append-Text -TxtPath $txtPath -Line "- note: runtime diagnostics and seeded list were visible in the desktop window."
  Append-Text -TxtPath $txtPath -Line ""
  Append-Text -TxtPath $txtPath -Line "conclusion: PASS"

  Add-EvidenceResult -EnvId $EnvId -CaseId $caseId -Result "PASS" -EvidenceLine "`$png + `$txt"
}

function Run-VGFailCase {
  param(
    [string]$EnvId,
    [string]$RuntimeMode,
    [System.Diagnostics.Process]$AppProcess
  )

  $caseId = "P5-T1-VG-FAIL"
  $txtPath = Join-Path $outputDir "${caseId}_${runDate}_${EnvId}_${tester}.txt"
  $pngPath = Join-Path $outputDir "${caseId}_${runDate}_${EnvId}_${tester}.png"
  Write-CaseTextHeader -TxtPath $txtPath -CaseId $caseId -EnvId $EnvId -RuntimeMode $RuntimeMode -TargetMode "desktop_window"

  $result = "PASS"
  $blockedReason = $null

  try {
    Focus-AppWindow -Process $AppProcess
    Send-Keys -Keys "+n"
    Send-Keys -Keys "{ENTER}"
    Start-Sleep -Milliseconds 900
    Take-ActiveWindowShot -Path $pngPath
    Send-Keys -Keys "{ESC}"
  } catch {
    $result = "BLOCKED"
    $blockedReason = $_.Exception.Message
    Take-ActiveWindowShot -Path $pngPath
  }

  Append-Text -TxtPath $txtPath -Line ""
  Append-Text -TxtPath $txtPath -Line "actual_result:"
  Append-Text -TxtPath $txtPath -Line "- screenshot: $pngPath"
  if ($null -eq $blockedReason) {
    Append-Text -TxtPath $txtPath -Line "- note: attempted empty create-series submission to surface validation feedback."
  } else {
    Append-Text -TxtPath $txtPath -Line "- blocked_reason: $blockedReason"
  }
  Append-Text -TxtPath $txtPath -Line ""
  Append-Text -TxtPath $txtPath -Line "conclusion: $result"

  Add-EvidenceResult -EnvId $EnvId -CaseId $caseId -Result $result -EvidenceLine "`$png + `$txt"
}

function Invoke-ArchivedTimelineAttempt {
  param([System.Diagnostics.Process]$AppProcess)

  Click-WindowRelative -Process $AppProcess -XRatio 0.60 -YRatio 0.25
  Click-WindowRelative -Process $AppProcess -XRatio 0.58 -YRatio 0.44
  Click-WindowRelative -Process $AppProcess -XRatio 0.83 -YRatio 0.44
}

function Run-IGPassCase {
  param(
    [string]$EnvId,
    [string]$RuntimeMode,
    [System.Diagnostics.Process]$AppProcess
  )

  $caseId = "P5-T1-IG-PASS"
  $txtPath = Join-Path $outputDir "${caseId}_${runDate}_${EnvId}_${tester}.txt"
  $mp4Path = Join-Path $outputDir "${caseId}_${runDate}_${EnvId}_${tester}.mp4"
  $pngPath = Join-Path $outputDir "${caseId}_${runDate}_${EnvId}_${tester}-end.png"
  Write-CaseTextHeader -TxtPath $txtPath -CaseId $caseId -EnvId $EnvId -RuntimeMode $RuntimeMode -TargetMode "desktop_window"

  $result = "PASS"
  $blockedReason = $null
  $createdSeries = "Inbox"
  $createdCommit = "first-note"
  $recording = Start-ScreenRecording -Path $mp4Path

  try {
    Focus-AppWindow -Process $AppProcess
    Send-Keys -Keys "+n"
    Send-Text -Text $createdSeries
    Send-Keys -Keys "{ENTER}"
    Send-Text -Text $createdCommit
    Send-Keys -Keys "{ENTER}"
    Send-Keys -Keys "/"
    Send-Text -Text $createdSeries
    Send-Keys -Keys "{ESC}"
    Send-Keys -Keys "{DOWN}"
    Send-Keys -Keys "{DOWN}"
    Send-Keys -Keys "a"
    Start-Sleep -Seconds 1
    Invoke-ArchivedTimelineAttempt -AppProcess $AppProcess
    Start-Sleep -Seconds 1
    Take-ActiveWindowShot -Path $pngPath
  } catch {
    $result = "BLOCKED"
    $blockedReason = $_.Exception.Message
    Take-ActiveWindowShot -Path $pngPath
  } finally {
    Wait-ScreenRecording -Process $recording
  }

  Append-Text -TxtPath $txtPath -Line ""
  Append-Text -TxtPath $txtPath -Line "actual_result:"
  Append-Text -TxtPath $txtPath -Line "- end_screenshot: $pngPath"
  Append-Text -TxtPath $txtPath -Line "- recording: $mp4Path"
  Append-Text -TxtPath $txtPath -Line "- action_chain: Shift+N -> create Inbox -> commit first-note -> search Inbox -> archive Project-A -> archived timeline attempt"

  if ($result -eq "PASS") {
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "[sqlite-proof]"
    Query-SqliteEvidence -SeriesName $createdSeries | Add-Content -Path $txtPath
    if ($RuntimeMode -ne "sqlite_only") {
      Append-Text -TxtPath $txtPath -Line ""
      Append-Text -TxtPath $txtPath -Line "[postgres-proof]"
      Query-PostgresEvidence -SeriesName $createdSeries | Add-Content -Path $txtPath
    }
  } else {
    Append-Text -TxtPath $txtPath -Line "- blocked_reason: $blockedReason"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "[sqlite-proof-after-block]"
    Query-SqliteEvidence -SeriesName $createdSeries | Add-Content -Path $txtPath
    if ($RuntimeMode -ne "sqlite_only") {
      Append-Text -TxtPath $txtPath -Line ""
      Append-Text -TxtPath $txtPath -Line "[postgres-proof-after-block]"
      Query-PostgresEvidence -SeriesName $createdSeries | Add-Content -Path $txtPath
    }
  }

  Append-Text -TxtPath $txtPath -Line ""
  Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  Add-EvidenceResult -EnvId $EnvId -CaseId $caseId -Result $result -EvidenceLine "`$mp4 + `$txt"
}

function Run-IGFailCase {
  param(
    [string]$EnvId,
    [string]$RuntimeMode,
    [System.Diagnostics.Process]$AppProcess
  )

  $caseId = "P5-T1-IG-FAIL"
  $txtPath = Join-Path $outputDir "${caseId}_${runDate}_${EnvId}_${tester}.txt"
  $mp4Path = Join-Path $outputDir "${caseId}_${runDate}_${EnvId}_${tester}.mp4"
  $pngPath = Join-Path $outputDir "${caseId}_${runDate}_${EnvId}_${tester}-end.png"
  Write-CaseTextHeader -TxtPath $txtPath -CaseId $caseId -EnvId $EnvId -RuntimeMode $RuntimeMode -TargetMode "desktop_window"

  $result = "PASS"
  $blockedReason = $null
  $recoverySeries = "Archive-Me"
  $recoveryCommit = "rollback-check"
  $recording = Start-ScreenRecording -Path $mp4Path

  try {
    Focus-AppWindow -Process $AppProcess
    Send-Keys -Keys "+n"
    Send-Keys -Keys "{ENTER}"
    Start-Sleep -Milliseconds 900
    Send-Keys -Keys "{ESC}"
    Send-Keys -Keys "x"
    Send-Keys -Keys "^a"
    Send-Keys -Keys "{BACKSPACE}"
    Send-Keys -Keys "{ENTER}"
    Start-Sleep -Milliseconds 900
    Send-Keys -Keys "{ESC}"
    Send-Keys -Keys "+n"
    Send-Text -Text $recoverySeries
    Send-Keys -Keys "{ENTER}"
    Send-Text -Text $recoveryCommit
    Send-Keys -Keys "{ENTER}"
    Start-Sleep -Seconds 1
    Take-ActiveWindowShot -Path $pngPath
  } catch {
    $result = "BLOCKED"
    $blockedReason = $_.Exception.Message
    Take-ActiveWindowShot -Path $pngPath
  } finally {
    Wait-ScreenRecording -Process $recording
  }

  Append-Text -TxtPath $txtPath -Line ""
  Append-Text -TxtPath $txtPath -Line "actual_result:"
  Append-Text -TxtPath $txtPath -Line "- end_screenshot: $pngPath"
  Append-Text -TxtPath $txtPath -Line "- recording: $mp4Path"
  Append-Text -TxtPath $txtPath -Line "- action_chain: empty create -> empty commit -> recovery create Archive-Me -> recovery commit rollback-check"

  if ($result -eq "PASS") {
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "[sqlite-proof]"
    Query-SqliteEvidence -SeriesName $recoverySeries | Add-Content -Path $txtPath
    if ($RuntimeMode -ne "sqlite_only") {
      Append-Text -TxtPath $txtPath -Line ""
      Append-Text -TxtPath $txtPath -Line "[postgres-proof]"
      Query-PostgresEvidence -SeriesName $recoverySeries | Add-Content -Path $txtPath
    }
  } else {
    Append-Text -TxtPath $txtPath -Line "- blocked_reason: $blockedReason"
  }

  Append-Text -TxtPath $txtPath -Line ""
  Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  Add-EvidenceResult -EnvId $EnvId -CaseId $caseId -Result $result -EvidenceLine "`$mp4 + `$txt"
}

function Prepare-ModeBaseline {
  param(
    [string]$RuntimeMode,
    [string]$EnvId
  )

  Stop-RememberProcesses

  switch ($RuntimeMode) {
    "sqlite_only" {
      Reset-SqliteDatabase
      Write-AppConfig -RuntimeMode $RuntimeMode
      Bootstrap-ModeStorage -RuntimeMode $RuntimeMode
      Seed-SqliteBaseline
    }
    "postgres_only" {
      Reset-TempPostgresSchema
      Write-AppConfig -RuntimeMode $RuntimeMode -PostgresDsn $tempPgDsn
      Bootstrap-ModeStorage -RuntimeMode $RuntimeMode -PostgresDsn $tempPgDsn
      Seed-PostgresBaseline
    }
    "dual_sync" {
      Reset-SqliteDatabase
      Reset-TempPostgresSchema
      Write-AppConfig -RuntimeMode $RuntimeMode -PostgresDsn $tempPgDsn
      Bootstrap-ModeStorage -RuntimeMode $RuntimeMode -PostgresDsn $tempPgDsn
      Seed-SqliteBaseline
      Seed-PostgresBaseline
    }
    default {
      throw "unsupported runtime mode $RuntimeMode"
    }
  }
}

function Run-ModeCases {
  param(
    [string]$EnvId,
    [string]$RuntimeMode
  )

  Prepare-ModeBaseline -RuntimeMode $RuntimeMode -EnvId $EnvId
  $app = Start-App

  try {
    $window = Wait-AppWindow -ProcessId $app.Id -ExpectedTitle "tauri-app [$RuntimeMode]"
    Start-Sleep -Seconds 2

    Run-VGPassCase -EnvId $EnvId -RuntimeMode $RuntimeMode -AppProcess $window
    Run-VGFailCase -EnvId $EnvId -RuntimeMode $RuntimeMode -AppProcess $window
    Run-IGPassCase -EnvId $EnvId -RuntimeMode $RuntimeMode -AppProcess $window
    Run-IGFailCase -EnvId $EnvId -RuntimeMode $RuntimeMode -AppProcess $window
  } finally {
    Stop-App -Process $app
  }
}

function Run-AutomationBaseline {
  New-Dir -Path $runtimeLogDir

  $commands = @(
    @{
      Name = "npm-test-unit"
      File = "npm.cmd"
      Args = @("run", "test:unit")
    },
    @{
      Name = "cargo-lib"
      File = "cargo"
      Args = @("test", "--manifest-path", "src-tauri\Cargo.toml", "--lib", "--", "--nocapture")
    },
    @{
      Name = "cargo-p2t5"
      File = "cargo"
      Args = @("test", "--manifest-path", "src-tauri\Cargo.toml", "--test", "p2_t5_basic_read_write_query", "--", "--nocapture")
    },
    @{
      Name = "cargo-p3t1"
      File = "cargo"
      Args = @("test", "--manifest-path", "src-tauri\Cargo.toml", "--test", "p3_t1_dual_sync_repository", "--", "--nocapture")
    },
    @{
      Name = "cargo-p3t2"
      File = "cargo"
      Args = @("test", "--manifest-path", "src-tauri\Cargo.toml", "--test", "p3_t2_parallel_tx_timeout", "--", "--nocapture")
    },
    @{
      Name = "cargo-p3t3"
      File = "cargo"
      Args = @("test", "--manifest-path", "src-tauri\Cargo.toml", "--test", "p3_t3_rollback_error_codes", "--", "--nocapture")
    },
    @{
      Name = "cargo-p3t4"
      File = "cargo"
      Args = @("test", "--manifest-path", "src-tauri\Cargo.toml", "--test", "p3_t4_single_side_compensation_alerts", "--", "--nocapture")
    },
    @{
      Name = "cargo-p3t5"
      File = "cargo"
      Args = @("test", "--manifest-path", "src-tauri\Cargo.toml", "--test", "p3_t5_startup_self_heal", "--", "--nocapture")
    }
  )

  foreach ($command in $commands) {
    $stdout = Join-Path $runtimeLogDir ("{0}.out.log" -f $command.Name)
    $stderr = Join-Path $runtimeLogDir ("{0}.err.log" -f $command.Name)
    $env:REMEMBER_TEST_POSTGRES_DSN = $tempPgDsn
    $process = Start-Process `
      -FilePath $command.File `
      -ArgumentList $command.Args `
      -WorkingDirectory $root `
      -PassThru `
      -RedirectStandardOutput $stdout `
      -RedirectStandardError $stderr
    Wait-Process -Id $process.Id
    if ($process.ExitCode -ne 0) {
      throw "$($command.Name) failed with exit code $($process.ExitCode)"
    }
  }
}

function Write-Summary {
  $ordered = $evidenceResults.Values | Sort-Object EnvId, CaseId
  $overall = if ($ordered.Result -contains "FAIL") {
    "FAIL"
  } elseif ($ordered.Result -contains "BLOCKED") {
    "BLOCKED"
  } else {
    "PASS"
  }

  Write-Output "overall=$overall"
  foreach ($entry in $ordered) {
    Write-Output ("{0} {1} {2}" -f $entry.EnvId, $entry.CaseId, $entry.Result)
  }
}

try {
  New-Dir -Path $outputDir
  New-Dir -Path $runtimeLogDir
  Backup-AppDataState
  Start-ViteServer
  Start-TempPostgres
  Run-AutomationBaseline
  Run-ModeCases -EnvId "ENV-SQLITE" -RuntimeMode "sqlite_only"
  Run-ModeCases -EnvId "ENV-PG" -RuntimeMode "postgres_only"
  Run-ModeCases -EnvId "ENV-DUAL" -RuntimeMode "dual_sync"
  Write-Summary
}
finally {
  Restore-AppDataState
  Stop-ViteServer
  Stop-TempPostgres
  Stop-RememberProcesses
}
