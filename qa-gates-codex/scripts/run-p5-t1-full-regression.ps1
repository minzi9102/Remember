param(
  [string[]]$Cases = @(
    "P5-T1-VG-PASS",
    "P5-T1-VG-FAIL",
    "P5-T1-IG-PASS",
    "P5-T1-IG-FAIL"
  ),
  [switch]$SkipAutomationBaseline
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class Win32Native {
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);

  [DllImport("user32.dll")]
  public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
}
"@

$validCases = @(
  "P5-T1-VG-PASS",
  "P5-T1-VG-FAIL",
  "P5-T1-IG-PASS",
  "P5-T1-IG-FAIL"
)
$selectedCases = [System.Collections.Generic.List[string]]::new()
foreach ($caseId in $Cases) {
  if ($validCases -notcontains $caseId) {
    throw "unsupported case id $caseId. Valid cases: $($validCases -join ', ')"
  }
  $selectedCases.Add($caseId)
}

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$outputDir = Join-Path $root "qa-gates-codex"
$appDataDir = Join-Path $env:APPDATA "com.remember.app"
$configPath = Join-Path $appDataDir "config.toml"
$sqlitePath = Join-Path $appDataDir "remember.sqlite3"
$pythonExe = Join-Path $root ".venv\Scripts\python.exe"
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
$vitePort = 1420
$viteUrl = "http://127.0.0.1:${vitePort}"
$runtimeLogDir = Join-Path $env:TEMP ("p5t1-logs-" + [guid]::NewGuid().ToString())
$backupDir = Join-Path $env:TEMP ("p5t1-backup-" + [guid]::NewGuid().ToString())
$modeMatrix = @(
  [pscustomobject]@{ EnvId = "ENV-SQLITE"; RuntimeMode = "sqlite_only" },
  [pscustomobject]@{ EnvId = "ENV-PG"; RuntimeMode = "postgres_only" },
  [pscustomobject]@{ EnvId = "ENV-DUAL"; RuntimeMode = "dual_sync" }
)
$caseResults = [ordered]@{}
$baselineResults = [System.Collections.Generic.List[object]]::new()

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

function Assert-Preconditions {
  $failures = [System.Collections.Generic.List[string]]::new()

  if (-not (Test-Path $pythonExe)) {
    $failures.Add("uv-managed python missing: $pythonExe")
  }
  if (-not (Test-Path $screenshotScript)) {
    $failures.Add("screenshot script missing: $screenshotScript")
  }
  if (-not (Test-Path $exePath)) {
    $failures.Add("tauri app binary missing: $exePath")
  }
  if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    $failures.Add("docker is not available on PATH")
  }
  if (-not (Get-Command npm.cmd -ErrorAction SilentlyContinue)) {
    $failures.Add("npm.cmd is not available on PATH")
  }

  if ($failures.Count -gt 0) {
    throw ($failures -join "; ")
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
  Stop-TempPostgres

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

function Invoke-TempPsqlText {
  param([string]$Sql)

  $output = $Sql | docker exec -i $containerName psql -q -v ON_ERROR_STOP=1 -U $tempPgUser -d $tempPgDatabase 2>&1
  if ($LASTEXITCODE -ne 0) {
    throw "failed to run psql in ${containerName}: $($output -join ' ')"
  }

  return (($output | Where-Object { $_ -ne $null }) -join "`n").Trim()
}

function Reset-TempPostgresSchema {
  $sql = @"
DROP SCHEMA IF EXISTS public CASCADE;
CREATE SCHEMA public;
"@
  $null = Invoke-TempPsqlText -Sql $sql
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

function Send-Keys {
  param([string]$Keys)

  [System.Windows.Forms.SendKeys]::SendWait($Keys)
  Start-Sleep -Milliseconds 500
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

function Invoke-PythonText {
  param([string]$Code)

  $output = $Code | & $pythonExe - 2>&1
  if ($LASTEXITCODE -ne 0) {
    throw "python command failed with exit code ${LASTEXITCODE}: $($output -join ' ')"
  }

  return (($output | Where-Object { $_ -ne $null }) -join "`n").Trim()
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
  $null = Invoke-PythonText -Code $code
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
  $null = Invoke-TempPsqlText -Sql $sql
}

function Prepare-ModeBaseline {
  param([string]$RuntimeMode)

  Stop-RememberProcesses

  switch ($RuntimeMode) {
    "sqlite_only" {
      Reset-SqliteDatabase
      Write-AppConfig -RuntimeMode $RuntimeMode
      Bootstrap-ModeStorage -RuntimeMode $RuntimeMode
      Seed-SqliteBaseline
    }
    "postgres_only" {
      Reset-SqliteDatabase
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

function Escape-SqlLiteral {
  param([string]$Value)

  if ($null -eq $Value) {
    return ""
  }

  return $Value.Replace("'", "''")
}

function Get-SqliteState {
  param(
    [string]$SeriesName = "",
    [string]$CommitContent = ""
  )

  $escapedPath = $sqlitePath.Replace("\", "\\")
  $seriesJson = $SeriesName.Replace("\", "\\").Replace('"', '\"')
  $commitJson = $CommitContent.Replace("\", "\\").Replace('"', '\"')

  $code = @"
import json
import sqlite3

path = r"$escapedPath"
series_name = "$seriesJson"
commit_content = "$commitJson"

conn = sqlite3.connect(path)
series_order = [
    f"{row[0]}:{row[1]}:{row[2]}"
    for row in conn.execute(
        "SELECT name, status, latest_excerpt FROM series ORDER BY last_updated_at DESC, id DESC"
    ).fetchall()
]
named_rows = []
if series_name:
    named_rows = conn.execute(
        "SELECT id, status, latest_excerpt FROM series WHERE name = ? ORDER BY created_at DESC, id DESC",
        (series_name,),
    ).fetchall()

payload = {
    "backend": "sqlite",
    "series_order": series_order,
    "anchor_status": conn.execute(
        "SELECT COALESCE(status, '') FROM series WHERE id = 'p5t1-anchor'"
    ).fetchone()[0],
    "project_a_status": conn.execute(
        "SELECT COALESCE(status, '') FROM series WHERE id = 'p5t1-project-a'"
    ).fetchone()[0],
    "project_a_archived_at": conn.execute(
        "SELECT COALESCE(archived_at, '') FROM series WHERE id = 'p5t1-project-a'"
    ).fetchone()[0],
    "named_series_count": len(named_rows),
    "named_series_id": named_rows[0][0] if named_rows else "",
    "named_series_status": named_rows[0][1] if named_rows else "",
    "named_series_excerpt": named_rows[0][2] if named_rows else "",
    "named_commit_count": conn.execute(
        "SELECT COUNT(*) FROM commits WHERE content = ?",
        (commit_content,),
    ).fetchone()[0] if commit_content else 0,
}

print(json.dumps(payload, ensure_ascii=True))
"@

  return (Invoke-PythonText -Code $code | ConvertFrom-Json)
}

function Get-PostgresState {
  param(
    [string]$SeriesName = "",
    [string]$CommitContent = ""
  )

  $escapedName = Escape-SqlLiteral -Value $SeriesName
  $escapedCommit = Escape-SqlLiteral -Value $CommitContent
  $sql = @"
\pset format unaligned
\pset tuples_only on
SELECT json_build_object(
  'backend', 'postgres',
  'series_order', COALESCE(
    (
      SELECT json_agg(name || ':' || status || ':' || latest_excerpt ORDER BY last_updated_at DESC, id DESC)
      FROM series
    ),
    '[]'::json
  ),
  'anchor_status', COALESCE((SELECT status FROM series WHERE id = 'p5t1-anchor'), ''),
  'project_a_status', COALESCE((SELECT status FROM series WHERE id = 'p5t1-project-a'), ''),
  'project_a_archived_at', COALESCE(
    (
      SELECT to_char(archived_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
      FROM series
      WHERE id = 'p5t1-project-a'
    ),
    ''
  ),
  'named_series_count', (SELECT COUNT(*) FROM series WHERE name = '$escapedName'),
  'named_series_id', COALESCE(
    (
      SELECT id
      FROM series
      WHERE name = '$escapedName'
      ORDER BY created_at DESC, id DESC
      LIMIT 1
    ),
    ''
  ),
  'named_series_status', COALESCE(
    (
      SELECT status
      FROM series
      WHERE name = '$escapedName'
      ORDER BY created_at DESC, id DESC
      LIMIT 1
    ),
    ''
  ),
  'named_series_excerpt', COALESCE(
    (
      SELECT latest_excerpt
      FROM series
      WHERE name = '$escapedName'
      ORDER BY created_at DESC, id DESC
      LIMIT 1
    ),
    ''
  ),
  'named_commit_count', (SELECT COUNT(*) FROM commits WHERE content = '$escapedCommit')
)::text;
"@

  return (Invoke-TempPsqlText -Sql $sql | ConvertFrom-Json)
}

function Get-ModeStates {
  param(
    [string]$RuntimeMode,
    [string]$SeriesName = "",
    [string]$CommitContent = ""
  )

  $states = [System.Collections.Generic.List[object]]::new()

  switch ($RuntimeMode) {
    "sqlite_only" {
      $states.Add((Get-SqliteState -SeriesName $SeriesName -CommitContent $CommitContent))
    }
    "postgres_only" {
      $states.Add((Get-PostgresState -SeriesName $SeriesName -CommitContent $CommitContent))
    }
    "dual_sync" {
      $states.Add((Get-SqliteState -SeriesName $SeriesName -CommitContent $CommitContent))
      $states.Add((Get-PostgresState -SeriesName $SeriesName -CommitContent $CommitContent))
    }
    default {
      throw "unsupported runtime mode $RuntimeMode"
    }
  }

  return $states
}

function Format-StateLines {
  param([object[]]$States)

  $lines = [System.Collections.Generic.List[string]]::new()
  foreach ($state in $States) {
    $orderText = @($state.series_order) -join " | "
    $lines.Add("backend=$($state.backend)")
    $lines.Add("series_order=$orderText")
    $lines.Add("anchor_status=$($state.anchor_status)")
    $lines.Add("project_a_status=$($state.project_a_status)")
    $lines.Add("project_a_archived_at=$($state.project_a_archived_at)")
    $lines.Add("named_series_count=$($state.named_series_count)")
    $lines.Add("named_series_id=$($state.named_series_id)")
    $lines.Add("named_series_status=$($state.named_series_status)")
    $lines.Add("named_series_excerpt=$($state.named_series_excerpt)")
    $lines.Add("named_commit_count=$($state.named_commit_count)")
  }
  return $lines
}

function Get-Excerpt {
  param([string]$Content)

  if ($Content.Length -le 48) {
    return $Content
  }

  return $Content.Substring(0, 48) + "..."
}

function Assert-DualNamedSeriesConsistency {
  param([object[]]$States)

  if ($States.Count -ne 2) {
    return @()
  }

  $left = $States[0]
  $right = $States[1]
  if ($left.named_series_id -ne $right.named_series_id) {
    Fail-Assert "dual sync series id mismatch: $($left.named_series_id) vs $($right.named_series_id)"
  }
  if ($left.named_series_status -ne $right.named_series_status) {
    Fail-Assert "dual sync series status mismatch: $($left.named_series_status) vs $($right.named_series_status)"
  }
  if ($left.named_series_excerpt -ne $right.named_series_excerpt) {
    Fail-Assert "dual sync latest excerpt mismatch: $($left.named_series_excerpt) vs $($right.named_series_excerpt)"
  }
  if ($left.project_a_status -ne $right.project_a_status) {
    Fail-Assert "dual sync Project-A status mismatch: $($left.project_a_status) vs $($right.project_a_status)"
  }

  return @(
    "dual_named_series_id_match=true",
    "dual_named_series_status_match=true",
    "dual_named_series_excerpt_match=true",
    "dual_project_a_status_match=true"
  )
}

function Invoke-Assertion {
  param(
    [string]$Name,
    [string]$RuntimeMode,
    [string]$SeriesName = "",
    [string]$CommitContent = "",
    [scriptblock]$AssertScript
  )

  $states = Get-ModeStates -RuntimeMode $RuntimeMode -SeriesName $SeriesName -CommitContent $CommitContent
  $lines = [System.Collections.Generic.List[string]]::new()
  $result = "PASS"

  try {
    $extraLines = & $AssertScript $states
  } catch {
    $result = "FAIL"
    $lines.Add("assertion_error=$($_.Exception.Message)")
    $extraLines = @()
  }

  foreach ($line in (Format-StateLines -States $states)) {
    $lines.Add($line)
  }
  foreach ($line in $extraLines) {
    $lines.Add($line)
  }

  return [pscustomobject]@{
    Name = $Name
    Result = $result
    Lines = $lines
  }
}

function Test-BaselineProof {
  param([string]$RuntimeMode)

  return Invoke-Assertion `
    -Name "baseline_state" `
    -RuntimeMode $RuntimeMode `
    -AssertScript {
      param($States)

      foreach ($state in $States) {
        $seriesOrder = @($state.series_order)
        if ($state.anchor_status -ne "active") {
          Fail-Assert "anchor series should be active on $($state.backend)"
        }
        if ($state.project_a_status -ne "silent") {
          Fail-Assert "Project-A should be silent on $($state.backend)"
        }
        if ($seriesOrder.Count -eq 0 -or $seriesOrder[0] -ne "Anchor Series:active:anchor-note") {
          Fail-Assert "Anchor Series should be the top row on $($state.backend)"
        }
      }

      return @("expected_project_a_status=silent")
    }
}

function Test-CreateProof {
  param(
    [string]$RuntimeMode,
    [string]$SeriesName
  )

  return Invoke-Assertion `
    -Name "create_series" `
    -RuntimeMode $RuntimeMode `
    -SeriesName $SeriesName `
    -AssertScript {
      param($States)

      foreach ($state in $States) {
        if ([int]$state.named_series_count -ne 1) {
          Fail-Assert "created series should appear exactly once on $($state.backend)"
        }
        if ($state.named_series_status -ne "active") {
          Fail-Assert "created series should be active on $($state.backend)"
        }
        if ($state.named_series_excerpt -ne "") {
          Fail-Assert "created series should have an empty excerpt before commit on $($state.backend)"
        }
        if ([int]$state.named_commit_count -ne 0) {
          Fail-Assert "created series should not have a matching commit yet on $($state.backend)"
        }
        if ($state.project_a_status -ne "silent") {
          Fail-Assert "Project-A should remain silent before archive on $($state.backend)"
        }
      }

      return @(Assert-DualNamedSeriesConsistency -States $States)
    }
}

function Test-CommitProof {
  param(
    [string]$RuntimeMode,
    [string]$SeriesName,
    [string]$CommitContent
  )

  $expectedExcerpt = Get-Excerpt -Content $CommitContent
  return Invoke-Assertion `
    -Name "append_commit" `
    -RuntimeMode $RuntimeMode `
    -SeriesName $SeriesName `
    -CommitContent $CommitContent `
    -AssertScript {
      param($States)

      foreach ($state in $States) {
        $seriesOrder = @($state.series_order)
        if ([int]$state.named_series_count -ne 1) {
          Fail-Assert "committed series should appear exactly once on $($state.backend)"
        }
        if ($state.named_series_status -ne "active") {
          Fail-Assert "committed series should stay active on $($state.backend)"
        }
        if ($state.named_series_excerpt -ne $expectedExcerpt) {
          Fail-Assert "latest excerpt should equal the submitted commit on $($state.backend)"
        }
        if ([int]$state.named_commit_count -ne 1) {
          Fail-Assert "matching commit should be written exactly once on $($state.backend)"
        }
        if ($seriesOrder.Count -eq 0 -or $seriesOrder[0] -ne "$SeriesName:active:$expectedExcerpt") {
          Fail-Assert "committed series should rise to the top on $($state.backend)"
        }
        if ($state.project_a_status -ne "silent") {
          Fail-Assert "Project-A should still be silent before archive on $($state.backend)"
        }
      }

      return @(Assert-DualNamedSeriesConsistency -States $States)
    }
}

function Test-ArchiveProof {
  param(
    [string]$RuntimeMode,
    [string]$SeriesName,
    [string]$CommitContent
  )

  $expectedExcerpt = Get-Excerpt -Content $CommitContent
  return Invoke-Assertion `
    -Name "archive_project_a" `
    -RuntimeMode $RuntimeMode `
    -SeriesName $SeriesName `
    -CommitContent $CommitContent `
    -AssertScript {
      param($States)

      foreach ($state in $States) {
        if ([int]$state.named_series_count -ne 1) {
          Fail-Assert "created series should remain unique after archive on $($state.backend)"
        }
        if ($state.named_series_excerpt -ne $expectedExcerpt) {
          Fail-Assert "created series excerpt should remain stable after archive on $($state.backend)"
        }
        if ([int]$state.named_commit_count -ne 1) {
          Fail-Assert "matching commit count should remain one after archive on $($state.backend)"
        }
        if ($state.project_a_status -ne "archived") {
          Fail-Assert "Project-A should be archived on $($state.backend)"
        }
        if ([string]::IsNullOrWhiteSpace($state.project_a_archived_at)) {
          Fail-Assert "Project-A archived_at should be populated on $($state.backend)"
        }
      }

      return @(Assert-DualNamedSeriesConsistency -States $States)
    }
}

function Test-RecoveryProof {
  param(
    [string]$RuntimeMode,
    [string]$SeriesName,
    [string]$CommitContent
  )

  $expectedExcerpt = Get-Excerpt -Content $CommitContent
  return Invoke-Assertion `
    -Name "recovery_create_and_commit" `
    -RuntimeMode $RuntimeMode `
    -SeriesName $SeriesName `
    -CommitContent $CommitContent `
    -AssertScript {
      param($States)

      foreach ($state in $States) {
        $seriesOrder = @($state.series_order)
        if ([int]$state.named_series_count -ne 1) {
          Fail-Assert "recovery series should appear exactly once on $($state.backend)"
        }
        if ($state.named_series_status -ne "active") {
          Fail-Assert "recovery series should be active on $($state.backend)"
        }
        if ($state.named_series_excerpt -ne $expectedExcerpt) {
          Fail-Assert "recovery excerpt should equal the submitted recovery commit on $($state.backend)"
        }
        if ([int]$state.named_commit_count -ne 1) {
          Fail-Assert "recovery commit should be written exactly once on $($state.backend)"
        }
        if ($state.anchor_status -ne "active") {
          Fail-Assert "Anchor Series should remain active on $($state.backend)"
        }
        if ($state.project_a_status -ne "silent") {
          Fail-Assert "Project-A should remain silent in the fail gate on $($state.backend)"
        }
        if ($seriesOrder.Count -eq 0 -or $seriesOrder[0] -ne "$SeriesName:active:$expectedExcerpt") {
          Fail-Assert "recovery series should become the top row on $($state.backend)"
        }
      }

      return @(Assert-DualNamedSeriesConsistency -States $States)
    }
}

function Test-FailureBaselineProof {
  param(
    [string]$RuntimeMode,
    [string]$SeriesName,
    [string]$CommitContent
  )

  return Invoke-Assertion `
    -Name "failure_without_side_effects" `
    -RuntimeMode $RuntimeMode `
    -SeriesName $SeriesName `
    -CommitContent $CommitContent `
    -AssertScript {
      param($States)

      foreach ($state in $States) {
        if ([int]$state.named_series_count -ne 0) {
          Fail-Assert "no recovery series should exist before the recovery step on $($state.backend)"
        }
        if ([int]$state.named_commit_count -ne 0) {
          Fail-Assert "no recovery commit should exist before the recovery step on $($state.backend)"
        }
        if ($state.anchor_status -ne "active") {
          Fail-Assert "Anchor Series should remain active on $($state.backend)"
        }
        if ($state.project_a_status -ne "silent") {
          Fail-Assert "Project-A should remain silent on $($state.backend)"
        }
      }

      return @("expected_recovery_series_count=0", "expected_recovery_commit_count=0")
    }
}

function Get-CaseEvidenceBase {
  param(
    [string]$CaseId,
    [string]$EnvId
  )

  return Join-Path $outputDir "${CaseId}_${runDate}_${EnvId}_${tester}"
}

function Write-CaseTextHeader {
  param(
    [string]$TxtPath,
    [string]$CaseId,
    [string]$EnvId,
    [string]$RuntimeMode,
    [string]$TargetMode,
    [string]$ReviewMode
  )

  Set-Content -Path $TxtPath -Value @(
    "case_id: $CaseId",
    "target_mode: $TargetMode",
    "review_mode: $ReviewMode",
    "env_id: $EnvId",
    "runtime_mode: $RuntimeMode",
    "run_date: $runDate",
    "tester: $tester",
    "structure: environment -> steps_or_checklist -> observer_result -> db_proof -> conclusion"
  )
}

function Append-Text {
  param(
    [string]$TxtPath,
    [string]$Line
  )

  Add-Content -Path $TxtPath -Value $Line
}

function Append-Lines {
  param(
    [string]$TxtPath,
    [string[]]$Lines
  )

  foreach ($line in $Lines) {
    Append-Text -TxtPath $TxtPath -Line $line
  }
}

function Set-CaseResult {
  param(
    [string]$EnvId,
    [string]$CaseId,
    [string]$Result,
    [string]$ObserverVerdict,
    [string]$DbVerdict,
    [string]$Evidence
  )

  $caseResults["$EnvId|$CaseId"] = [pscustomobject]@{
    EnvId = $EnvId
    CaseId = $CaseId
    Result = $Result
    ObserverVerdict = $ObserverVerdict
    DbVerdict = $DbVerdict
    Evidence = $Evidence
  }
}

function Add-BaselineResult {
  param(
    [string]$Name,
    [string]$Result,
    [string]$StdoutPath,
    [string]$StderrPath,
    [string]$Note
  )

  $baselineResults.Add([pscustomobject]@{
      Name = $Name
      Result = $Result
      StdoutPath = $StdoutPath
      StderrPath = $StderrPath
      Note = $Note
    })
}

function Read-PassFailVerdict {
  param([string]$Prompt)

  while ($true) {
    $value = (Read-Host $Prompt).Trim().ToUpperInvariant()
    if ($value -in @("PASS", "FAIL")) {
      return $value
    }

    Write-Host "Please enter PASS or FAIL." -ForegroundColor Yellow
  }
}

function Read-OptionalNote {
  param([string]$Prompt)

  $note = Read-Host $Prompt
  if ([string]::IsNullOrWhiteSpace($note)) {
    return "<none>"
  }

  return $note.Trim()
}

function Invoke-CodexMultimodalReview {
  param(
    [string]$CaseId,
    [string]$EnvId,
    [string]$RuntimeMode,
    [string[]]$Artifacts,
    [string[]]$Checklist
  )

  Write-Host ""
  Write-Host "[$CaseId][$EnvId][$RuntimeMode] Codex multimodal review required." -ForegroundColor Cyan
  Write-Host "Review these artifacts with Codex multimodal, then enter PASS or FAIL:" -ForegroundColor Cyan
  foreach ($artifact in $Artifacts) {
    Write-Host "  - $artifact" -ForegroundColor Gray
  }
  Write-Host "Checklist:" -ForegroundColor Cyan
  foreach ($item in $Checklist) {
    Write-Host "  - $item" -ForegroundColor Gray
  }

  $verdict = Read-PassFailVerdict -Prompt "codex_verdict"
  $note = Read-OptionalNote -Prompt "codex_note (optional)"

  return [pscustomobject]@{
    Verdict = $verdict
    Note = $note
  }
}

function Append-DbAssertionSection {
  param(
    [string]$TxtPath,
    [pscustomobject]$DbAssertion
  )

  Append-Text -TxtPath $TxtPath -Line "db_assertion_result: $($DbAssertion.Result)"
  Append-Text -TxtPath $TxtPath -Line "db_proof:"
  foreach ($line in $DbAssertion.Lines) {
    Append-Text -TxtPath $TxtPath -Line "- $line"
  }
}

function Invoke-ManualGateStep {
  param(
    [string]$TxtPath,
    [System.Diagnostics.Process]$AppProcess,
    [string]$StepId,
    [string]$Instruction,
    [string]$ScreenshotPath,
    [scriptblock]$DbAssertionScript
  )

  Focus-AppWindow -Process $AppProcess
  Write-Host ""
  Write-Host "[$StepId] $Instruction" -ForegroundColor Cyan

  $humanResult = Read-PassFailVerdict -Prompt "${StepId}_result"
  Take-ActiveWindowShot -Path $ScreenshotPath
  $humanNote = Read-OptionalNote -Prompt "${StepId}_note (optional)"
  $dbAssertion = & $DbAssertionScript

  Append-Text -TxtPath $TxtPath -Line ""
  Append-Text -TxtPath $TxtPath -Line "step_id: $StepId"
  Append-Text -TxtPath $TxtPath -Line "instruction: $Instruction"
  Append-Text -TxtPath $TxtPath -Line "human_result: $humanResult"
  Append-Text -TxtPath $TxtPath -Line "human_note: $humanNote"
  Append-Text -TxtPath $TxtPath -Line "screenshot: $ScreenshotPath"
  Append-DbAssertionSection -TxtPath $TxtPath -DbAssertion $dbAssertion

  return [pscustomobject]@{
    StepId = $StepId
    HumanResult = $humanResult
    HumanNote = $humanNote
    ScreenshotPath = $ScreenshotPath
    DbAssertion = $dbAssertion
    ShouldContinue = ($humanResult -eq "PASS" -and $dbAssertion.Result -eq "PASS")
  }
}

function Resolve-CaseResult {
  param(
    [string]$ObserverVerdict,
    [string]$DbVerdict
  )

  if ($ObserverVerdict -eq "PASS" -and $DbVerdict -eq "PASS") {
    return "PASS"
  }

  return "FAIL"
}

function New-CaseToken {
  return ([guid]::NewGuid().ToString("N").Substring(0, 8))
}

function Run-VGPassCase {
  param(
    [string]$EnvId,
    [string]$RuntimeMode,
    [System.Diagnostics.Process]$AppProcess
  )

  $caseId = "P5-T1-VG-PASS"
  $base = Get-CaseEvidenceBase -CaseId $caseId -EnvId $EnvId
  $txtPath = "$base.txt"
  $listShot = "${base}-list.png"
  $timelineShot = "${base}-timeline.png"
  Write-CaseTextHeader -TxtPath $txtPath -CaseId $caseId -EnvId $EnvId -RuntimeMode $RuntimeMode -TargetMode "desktop_window" -ReviewMode "codex_multimodal"

  $observerVerdict = "FAIL"
  $dbVerdict = "FAIL"
  $result = "BLOCKED"

  try {
    Focus-AppWindow -Process $AppProcess
    Take-ActiveWindowShot -Path $listShot
    Send-Keys -Keys "{RIGHT}"
    Take-ActiveWindowShot -Path $timelineShot
    Send-Keys -Keys "{ESC}"

    $dbAssertion = Test-BaselineProof -RuntimeMode $RuntimeMode
    $review = Invoke-CodexMultimodalReview `
      -CaseId $caseId `
      -EnvId $EnvId `
      -RuntimeMode $RuntimeMode `
      -Artifacts @($listShot, $timelineShot) `
      -Checklist @(
        "Confirm the key panels are visible and the shell is fully rendered.",
        "Confirm the seeded list ordering and badges match the baseline state.",
        "Confirm the timeline screenshot opens the correct series without obvious layout breakage.",
        "Mark archived-view checks as N/A for this case if no archived panel is shown.",
        "Fail the review if there is obvious clipping, overlap, blank content, or fake-success UI."
      )

    $observerVerdict = $review.Verdict
    $dbVerdict = $dbAssertion.Result
    $result = Resolve-CaseResult -ObserverVerdict $observerVerdict -DbVerdict $dbVerdict

    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "environment:"
    Append-Text -TxtPath $txtPath -Line "- vite_url: $viteUrl"
    Append-Text -TxtPath $txtPath -Line "- desktop_title: $($AppProcess.MainWindowTitle)"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "visual_artifacts:"
    Append-Text -TxtPath $txtPath -Line "- list_screenshot: $listShot"
    Append-Text -TxtPath $txtPath -Line "- timeline_screenshot: $timelineShot"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "codex_checklist:"
    Append-Lines -TxtPath $txtPath -Lines @(
      "- key panels visible",
      "- seeded ordering and badges stable",
      "- timeline screenshot targets the correct series",
      "- archived-view checks marked N/A if not shown",
      "- no visible overlap, clipping, or fake-success state"
    )
    Append-Text -TxtPath $txtPath -Line "codex_verdict: $observerVerdict"
    Append-Text -TxtPath $txtPath -Line "codex_note: $($review.Note)"
    Append-Text -TxtPath $txtPath -Line ""
    Append-DbAssertionSection -TxtPath $txtPath -DbAssertion $dbAssertion
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  } catch {
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "blocked_reason: $($_.Exception.Message)"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: BLOCKED"
  }

  Set-CaseResult -EnvId $EnvId -CaseId $caseId -Result $result -ObserverVerdict $observerVerdict -DbVerdict $dbVerdict -Evidence "`$txt + list/timeline screenshots"
}

function Run-VGFailCase {
  param(
    [string]$EnvId,
    [string]$RuntimeMode,
    [System.Diagnostics.Process]$AppProcess
  )

  $caseId = "P5-T1-VG-FAIL"
  $base = Get-CaseEvidenceBase -CaseId $caseId -EnvId $EnvId
  $txtPath = "$base.txt"
  $errorShot = "${base}-error.png"
  Write-CaseTextHeader -TxtPath $txtPath -CaseId $caseId -EnvId $EnvId -RuntimeMode $RuntimeMode -TargetMode "desktop_window" -ReviewMode "codex_multimodal"

  $observerVerdict = "FAIL"
  $dbVerdict = "FAIL"
  $result = "BLOCKED"

  try {
    Focus-AppWindow -Process $AppProcess
    Send-Keys -Keys "+n"
    Send-Keys -Keys "{ENTER}"
    Take-ActiveWindowShot -Path $errorShot
    Send-Keys -Keys "{ESC}"

    $dbAssertion = Test-BaselineProof -RuntimeMode $RuntimeMode
    $review = Invoke-CodexMultimodalReview `
      -CaseId $caseId `
      -EnvId $EnvId `
      -RuntimeMode $RuntimeMode `
      -Artifacts @($errorShot) `
      -Checklist @(
        "Confirm there is an explicit visible validation or failure signal.",
        "Confirm the shell remains readable and does not collapse after the invalid action.",
        "Fail the review if the app silently accepts the empty create action.",
        "Fail the review if the screen is blank, clipped, or visibly unstable."
      )

    $observerVerdict = $review.Verdict
    $dbVerdict = $dbAssertion.Result
    $result = Resolve-CaseResult -ObserverVerdict $observerVerdict -DbVerdict $dbVerdict

    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "environment:"
    Append-Text -TxtPath $txtPath -Line "- vite_url: $viteUrl"
    Append-Text -TxtPath $txtPath -Line "- desktop_title: $($AppProcess.MainWindowTitle)"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "visual_artifacts:"
    Append-Text -TxtPath $txtPath -Line "- validation_screenshot: $errorShot"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "codex_checklist:"
    Append-Lines -TxtPath $txtPath -Lines @(
      "- explicit validation or failure feedback is visible",
      "- shell remains stable after the invalid action",
      "- no silent acceptance of the empty create request",
      "- no blank, clipped, or broken layout"
    )
    Append-Text -TxtPath $txtPath -Line "codex_verdict: $observerVerdict"
    Append-Text -TxtPath $txtPath -Line "codex_note: $($review.Note)"
    Append-Text -TxtPath $txtPath -Line ""
    Append-DbAssertionSection -TxtPath $txtPath -DbAssertion $dbAssertion
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  } catch {
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "blocked_reason: $($_.Exception.Message)"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: BLOCKED"
  }

  Set-CaseResult -EnvId $EnvId -CaseId $caseId -Result $result -ObserverVerdict $observerVerdict -DbVerdict $dbVerdict -Evidence "`$txt + error screenshot"
}

function Run-IGPassCase {
  param(
    [string]$EnvId,
    [string]$RuntimeMode,
    [System.Diagnostics.Process]$AppProcess
  )

  $caseId = "P5-T1-IG-PASS"
  $token = New-CaseToken
  $createdSeries = "P5T1-$EnvId-PASS-$token"
  $createdCommit = "p5t1-pass-note-$token"
  $base = Get-CaseEvidenceBase -CaseId $caseId -EnvId $EnvId
  $txtPath = "$base.txt"
  Write-CaseTextHeader -TxtPath $txtPath -CaseId $caseId -EnvId $EnvId -RuntimeMode $RuntimeMode -TargetMode "desktop_window" -ReviewMode "human_step_input"

  $result = "BLOCKED"
  $observerVerdict = "FAIL"
  $dbVerdict = "FAIL"
  $stepResults = [System.Collections.Generic.List[object]]::new()

  try {
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "environment:"
    Append-Text -TxtPath $txtPath -Line "- vite_url: $viteUrl"
    Append-Text -TxtPath $txtPath -Line "- desktop_title: $($AppProcess.MainWindowTitle)"
    Append-Text -TxtPath $txtPath -Line "- created_series: $createdSeries"
    Append-Text -TxtPath $txtPath -Line "- created_commit: $createdCommit"
    Append-Text -TxtPath $txtPath -Line "- archive_target: p5t1-project-a"

    $step1 = Invoke-ManualGateStep `
      -TxtPath $txtPath `
      -AppProcess $AppProcess `
      -StepId "step_1_create_series" `
      -Instruction "Press Shift+N, enter '$createdSeries', and press Enter. Confirm the commit draft opens for the new series." `
      -ScreenshotPath "${base}-step1-create.png" `
      -DbAssertionScript { Test-CreateProof -RuntimeMode $RuntimeMode -SeriesName $createdSeries }
    $stepResults.Add($step1)
    if (-not $step1.ShouldContinue) {
      throw "interactive gate stopped after step_1_create_series because a human or DB assertion failed"
    }

    $step2 = Invoke-ManualGateStep `
      -TxtPath $txtPath `
      -AppProcess $AppProcess `
      -StepId "step_2_append_commit" `
      -Instruction "Type '$createdCommit' and press Enter. Confirm the new series moves to the top and shows the new excerpt." `
      -ScreenshotPath "${base}-step2-commit.png" `
      -DbAssertionScript { Test-CommitProof -RuntimeMode $RuntimeMode -SeriesName $createdSeries -CommitContent $createdCommit }
    $stepResults.Add($step2)
    if (-not $step2.ShouldContinue) {
      throw "interactive gate stopped after step_2_append_commit because a human or DB assertion failed"
    }

    $step3 = Invoke-ManualGateStep `
      -TxtPath $txtPath `
      -AppProcess $AppProcess `
      -StepId "step_3_search_roundtrip" `
      -Instruction "Press /, search for '$createdSeries', then press Esc. Confirm the new series remains selected or visible after the search roundtrip." `
      -ScreenshotPath "${base}-step3-search.png" `
      -DbAssertionScript { Test-CommitProof -RuntimeMode $RuntimeMode -SeriesName $createdSeries -CommitContent $createdCommit }
    $stepResults.Add($step3)
    if (-not $step3.ShouldContinue) {
      throw "interactive gate stopped after step_3_search_roundtrip because a human or DB assertion failed"
    }

    $step4 = Invoke-ManualGateStep `
      -TxtPath $txtPath `
      -AppProcess $AppProcess `
      -StepId "step_4_archive_project_a" `
      -Instruction "Use the keyboard to select Project-A, press 'a', and confirm Project-A disappears from the active list." `
      -ScreenshotPath "${base}-step4-archive.png" `
      -DbAssertionScript { Test-ArchiveProof -RuntimeMode $RuntimeMode -SeriesName $createdSeries -CommitContent $createdCommit }
    $stepResults.Add($step4)
    if (-not $step4.ShouldContinue) {
      throw "interactive gate stopped after step_4_archive_project_a because a human or DB assertion failed"
    }

    $step5 = Invoke-ManualGateStep `
      -TxtPath $txtPath `
      -AppProcess $AppProcess `
      -StepId "step_5_archived_timeline" `
      -Instruction "Switch to Archived, open Project-A, and confirm the Archived badge and read-only timeline are visible." `
      -ScreenshotPath "${base}-step5-archived-timeline.png" `
      -DbAssertionScript { Test-ArchiveProof -RuntimeMode $RuntimeMode -SeriesName $createdSeries -CommitContent $createdCommit }
    $stepResults.Add($step5)
    if (-not $step5.ShouldContinue) {
      throw "interactive gate stopped after step_5_archived_timeline because a human or DB assertion failed"
    }

    $observerVerdict = if (($stepResults | Where-Object { $_.HumanResult -ne "PASS" }).Count -eq 0) { "PASS" } else { "FAIL" }
    $dbVerdict = if (($stepResults | Where-Object { $_.DbAssertion.Result -ne "PASS" }).Count -eq 0) { "PASS" } else { "FAIL" }
    $result = Resolve-CaseResult -ObserverVerdict $observerVerdict -DbVerdict $dbVerdict
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "human_verdict: $observerVerdict"
    Append-Text -TxtPath $txtPath -Line "final_db_verdict: $dbVerdict"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  } catch {
    if ($stepResults.Count -gt 0) {
      $observerVerdict = if (($stepResults | Where-Object { $_.HumanResult -ne "PASS" }).Count -eq 0) { "PASS" } else { "FAIL" }
      $dbVerdict = if (($stepResults | Where-Object { $_.DbAssertion.Result -ne "PASS" }).Count -eq 0) { "PASS" } else { "FAIL" }
    }
    $result = if ($_.Exception.Message.StartsWith("ASSERT:") -or $_.Exception.Message.StartsWith("interactive gate stopped")) { "FAIL" } else { "BLOCKED" }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "stopped_reason: $($_.Exception.Message)"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "human_verdict: $observerVerdict"
    Append-Text -TxtPath $txtPath -Line "final_db_verdict: $dbVerdict"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  }

  Set-CaseResult -EnvId $EnvId -CaseId $caseId -Result $result -ObserverVerdict $observerVerdict -DbVerdict $dbVerdict -Evidence "`$txt + per-step screenshots"
}

function Run-IGFailCase {
  param(
    [string]$EnvId,
    [string]$RuntimeMode,
    [System.Diagnostics.Process]$AppProcess
  )

  $caseId = "P5-T1-IG-FAIL"
  $token = New-CaseToken
  $recoverySeries = "P5T1-$EnvId-RECOVERY-$token"
  $recoveryCommit = "p5t1-recovery-note-$token"
  $base = Get-CaseEvidenceBase -CaseId $caseId -EnvId $EnvId
  $txtPath = "$base.txt"
  Write-CaseTextHeader -TxtPath $txtPath -CaseId $caseId -EnvId $EnvId -RuntimeMode $RuntimeMode -TargetMode "desktop_window" -ReviewMode "human_step_input"

  $result = "BLOCKED"
  $observerVerdict = "FAIL"
  $dbVerdict = "FAIL"
  $stepResults = [System.Collections.Generic.List[object]]::new()

  try {
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "environment:"
    Append-Text -TxtPath $txtPath -Line "- vite_url: $viteUrl"
    Append-Text -TxtPath $txtPath -Line "- desktop_title: $($AppProcess.MainWindowTitle)"
    Append-Text -TxtPath $txtPath -Line "- recovery_series: $recoverySeries"
    Append-Text -TxtPath $txtPath -Line "- recovery_commit: $recoveryCommit"

    $step1 = Invoke-ManualGateStep `
      -TxtPath $txtPath `
      -AppProcess $AppProcess `
      -StepId "step_1_empty_create" `
      -Instruction "Press Shift+N, submit an empty create request, and confirm a visible validation or failure message appears." `
      -ScreenshotPath "${base}-step1-empty-create.png" `
      -DbAssertionScript { Test-FailureBaselineProof -RuntimeMode $RuntimeMode -SeriesName $recoverySeries -CommitContent $recoveryCommit }
    $stepResults.Add($step1)
    if (-not $step1.ShouldContinue) {
      throw "interactive fail gate stopped after step_1_empty_create because a human or DB assertion failed"
    }

    $step2 = Invoke-ManualGateStep `
      -TxtPath $txtPath `
      -AppProcess $AppProcess `
      -StepId "step_2_empty_commit" `
      -Instruction "Select Anchor Series, start a commit draft, clear the input, submit the empty draft, and confirm the validation or failure message appears." `
      -ScreenshotPath "${base}-step2-empty-commit.png" `
      -DbAssertionScript { Test-FailureBaselineProof -RuntimeMode $RuntimeMode -SeriesName $recoverySeries -CommitContent $recoveryCommit }
    $stepResults.Add($step2)
    if (-not $step2.ShouldContinue) {
      throw "interactive fail gate stopped after step_2_empty_commit because a human or DB assertion failed"
    }

    $step3 = Invoke-ManualGateStep `
      -TxtPath $txtPath `
      -AppProcess $AppProcess `
      -StepId "step_3_recovery_path" `
      -Instruction "Run the recovery path: create '$recoverySeries', submit '$recoveryCommit', and confirm the shell returns to a healthy usable state." `
      -ScreenshotPath "${base}-step3-recovery.png" `
      -DbAssertionScript { Test-RecoveryProof -RuntimeMode $RuntimeMode -SeriesName $recoverySeries -CommitContent $recoveryCommit }
    $stepResults.Add($step3)
    if (-not $step3.ShouldContinue) {
      throw "interactive fail gate stopped after step_3_recovery_path because a human or DB assertion failed"
    }

    $observerVerdict = if (($stepResults | Where-Object { $_.HumanResult -ne "PASS" }).Count -eq 0) { "PASS" } else { "FAIL" }
    $dbVerdict = if (($stepResults | Where-Object { $_.DbAssertion.Result -ne "PASS" }).Count -eq 0) { "PASS" } else { "FAIL" }
    $result = Resolve-CaseResult -ObserverVerdict $observerVerdict -DbVerdict $dbVerdict
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "human_verdict: $observerVerdict"
    Append-Text -TxtPath $txtPath -Line "final_db_verdict: $dbVerdict"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  } catch {
    if ($stepResults.Count -gt 0) {
      $observerVerdict = if (($stepResults | Where-Object { $_.HumanResult -ne "PASS" }).Count -eq 0) { "PASS" } else { "FAIL" }
      $dbVerdict = if (($stepResults | Where-Object { $_.DbAssertion.Result -ne "PASS" }).Count -eq 0) { "PASS" } else { "FAIL" }
    }
    $result = if ($_.Exception.Message.StartsWith("ASSERT:") -or $_.Exception.Message.StartsWith("interactive fail gate stopped")) { "FAIL" } else { "BLOCKED" }
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "stopped_reason: $($_.Exception.Message)"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "human_verdict: $observerVerdict"
    Append-Text -TxtPath $txtPath -Line "final_db_verdict: $dbVerdict"
    Append-Text -TxtPath $txtPath -Line ""
    Append-Text -TxtPath $txtPath -Line "conclusion: $result"
  }

  Set-CaseResult -EnvId $EnvId -CaseId $caseId -Result $result -ObserverVerdict $observerVerdict -DbVerdict $dbVerdict -Evidence "`$txt + per-step screenshots"
}

function Invoke-IsolatedCase {
  param(
    [string]$EnvId,
    [string]$RuntimeMode,
    [string]$CaseId
  )

  Prepare-ModeBaseline -RuntimeMode $RuntimeMode
  $app = Start-App

  try {
    $window = Wait-AppWindow -ProcessId $app.Id -ExpectedTitle "tauri-app [$RuntimeMode]"
    Start-Sleep -Seconds 2

    switch ($CaseId) {
      "P5-T1-VG-PASS" { Run-VGPassCase -EnvId $EnvId -RuntimeMode $RuntimeMode -AppProcess $window }
      "P5-T1-VG-FAIL" { Run-VGFailCase -EnvId $EnvId -RuntimeMode $RuntimeMode -AppProcess $window }
      "P5-T1-IG-PASS" { Run-IGPassCase -EnvId $EnvId -RuntimeMode $RuntimeMode -AppProcess $window }
      "P5-T1-IG-FAIL" { Run-IGFailCase -EnvId $EnvId -RuntimeMode $RuntimeMode -AppProcess $window }
      default { throw "unsupported case id $CaseId" }
    }
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

    try {
      $process = Start-Process `
        -FilePath $command.File `
        -ArgumentList $command.Args `
        -WorkingDirectory $root `
        -Wait `
        -PassThru `
        -RedirectStandardOutput $stdout `
        -RedirectStandardError $stderr

      if ($process.ExitCode -eq 0) {
        Add-BaselineResult -Name $command.Name -Result "PASS" -StdoutPath $stdout -StderrPath $stderr -Note "exit code 0"
      } else {
        Add-BaselineResult -Name $command.Name -Result "FAIL" -StdoutPath $stdout -StderrPath $stderr -Note ("exit code {0}" -f $process.ExitCode)
      }
    } catch {
      Add-BaselineResult -Name $command.Name -Result "BLOCKED" -StdoutPath $stdout -StderrPath $stderr -Note $_.Exception.Message
    }
  }
}

function Write-Summary {
  $ordered = $caseResults.Values | Sort-Object EnvId, CaseId
  $baselineState = $baselineResults.Result
  $overall = if ($baselineState -contains "FAIL" -or $ordered.Result -contains "FAIL") {
    "FAIL"
  } elseif ($baselineState -contains "BLOCKED" -or $ordered.Result -contains "BLOCKED") {
    "BLOCKED"
  } else {
    "PASS"
  }

  Write-Output "overall=$overall"
  foreach ($entry in $baselineResults) {
    Write-Output ("BASELINE {0} {1} {2}" -f $entry.Name, $entry.Result, $entry.Note)
  }
  foreach ($entry in $ordered) {
    Write-Output ("{0} {1} {2} observer={3} db={4}" -f $entry.EnvId, $entry.CaseId, $entry.Result, $entry.ObserverVerdict, $entry.DbVerdict)
  }
}

try {
  Assert-Preconditions
  New-Dir -Path $outputDir
  New-Dir -Path $runtimeLogDir
  Backup-AppDataState
  Start-ViteServer
  Start-TempPostgres

  if (-not $SkipAutomationBaseline) {
    Run-AutomationBaseline
  }

  foreach ($mode in $modeMatrix) {
    foreach ($caseId in $selectedCases) {
      Invoke-IsolatedCase -EnvId $mode.EnvId -RuntimeMode $mode.RuntimeMode -CaseId $caseId
    }
  }

  Write-Summary
}
finally {
  Restore-AppDataState
  Stop-ViteServer
  Stop-TempPostgres
  Stop-RememberProcesses
}
