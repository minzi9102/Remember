param(
  [ValidateSet("ENV-SQLITE")]
  [string]$EnvId = "ENV-SQLITE",
  [string]$Tester = "codex"
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
$outputDir = Join-Path $root "qa-gates-codex"
$runDate = Get-Date -Format "yyyyMMdd"
$reportPath = Join-Path $outputDir ("P5-T1-AUTO-REGRESSION_{0}_{1}_{2}.txt" -f $runDate, $EnvId, $Tester)
$logDir = Join-Path $env:TEMP ("p5t1-auto-regression-" + [guid]::NewGuid().ToString("N"))

function New-Dir {
  param([string]$Path)

  if (-not (Test-Path $Path)) {
    New-Item -ItemType Directory -Path $Path | Out-Null
  }
}

function Get-Slug {
  param([string]$Value)

  return (($Value.ToLowerInvariant() -replace "[^a-z0-9]+", "-").Trim("-"))
}

function Invoke-Check {
  param(
    [string]$Id,
    [string]$FilePath,
    [string[]]$Arguments,
    [string[]]$ExpectedPatterns = @()
  )

  $slug = Get-Slug -Value $Id
  $stdoutPath = Join-Path $logDir ("{0}.out.log" -f $slug)
  $stderrPath = Join-Path $logDir ("{0}.err.log" -f $slug)
  $commandText = @($FilePath) + $Arguments -join " "

  $result = "PASS"
  $note = "exit code 0"
  $missingPatterns = [System.Collections.Generic.List[string]]::new()

  try {
    $process = Start-Process `
      -FilePath $FilePath `
      -ArgumentList $Arguments `
      -WorkingDirectory $root `
      -Wait `
      -PassThru `
      -RedirectStandardOutput $stdoutPath `
      -RedirectStandardError $stderrPath

    if ($process.ExitCode -ne 0) {
      $result = "FAIL"
      $note = "exit code $($process.ExitCode)"
    }
  } catch {
    $result = "FAIL"
    $note = $_.Exception.Message
  }

  if ($result -eq "PASS" -and $ExpectedPatterns.Count -gt 0) {
    $stdoutText = if (Test-Path $stdoutPath) { Get-Content -Path $stdoutPath -Raw } else { "" }
    $stderrText = if (Test-Path $stderrPath) { Get-Content -Path $stderrPath -Raw } else { "" }
    $combinedText = ($stdoutText + "`n" + $stderrText)

    foreach ($pattern in $ExpectedPatterns) {
      if (-not $combinedText.Contains($pattern)) {
        $missingPatterns.Add($pattern)
      }
    }

    if ($missingPatterns.Count -gt 0) {
      $result = "FAIL"
      $note = "missing expected output pattern(s)"
    }
  }

  [pscustomobject]@{
    Id = $Id
    Command = $commandText
    Result = $result
    Note = $note
    StdoutLog = $stdoutPath
    StderrLog = $stderrPath
    ExpectedPatterns = $ExpectedPatterns
    MissingPatterns = @($missingPatterns.ToArray())
  }
}

New-Dir -Path $outputDir
New-Dir -Path $logDir

$checks = @(
  (Invoke-Check `
      -Id "npm_test_unit" `
      -FilePath "npm.cmd" `
      -Arguments @("run", "test:unit") `
      -ExpectedPatterns @("Test Files", "passed")),
  (Invoke-Check `
      -Id "cargo_test_full" `
      -FilePath "cargo" `
      -Arguments @("test", "--manifest-path", "src-tauri/Cargo.toml") `
      -ExpectedPatterns @("test result: ok.")),
  (Invoke-Check `
      -Id "rust_warning_assert_runtime_mode_ignored" `
      -FilePath "cargo" `
      -Arguments @(
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "application::config::tests::legacy_runtime_modes_are_accepted_but_ignored",
        "--",
        "--nocapture"
      ) `
      -ExpectedPatterns @("legacy_runtime_modes_are_accepted_but_ignored ... ok")),
  (Invoke-Check `
      -Id "rust_warning_assert_postgres_dsn_ignored" `
      -FilePath "cargo" `
      -Arguments @(
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "application::config::tests::warns_when_legacy_postgres_dsn_is_present",
        "--",
        "--nocapture"
      ) `
      -ExpectedPatterns @("warns_when_legacy_postgres_dsn_is_present ... ok")),
  (Invoke-Check `
      -Id "ts_warning_assert_runtime_mode_ignored" `
      -FilePath "npm.cmd" `
      -Arguments @(
        "exec",
        "--",
        "vitest",
        "run",
        "tests/runtime-adapter.test.ts",
        "--reporter=verbose",
        "-t",
        "warns when legacy runtime modes are present"
      ) `
      -ExpectedPatterns @("warns when legacy runtime modes are present")),
  (Invoke-Check `
      -Id "ts_warning_assert_postgres_mode_warning_collection" `
      -FilePath "npm.cmd" `
      -Arguments @(
        "exec",
        "--",
        "vitest",
        "run",
        "tests/runtime-adapter.test.ts",
        "--reporter=verbose",
        "-t",
        "keeps warning collection from query parameters"
      ) `
      -ExpectedPatterns @("keeps warning collection from query parameters"))
)

$overall = if (($checks | Where-Object { $_.Result -ne "PASS" }).Count -eq 0) { "PASS" } else { "FAIL" }
$warningVerdict = if (($checks | Where-Object { $_.Id -like "*warning_assert*" -and $_.Result -ne "PASS" }).Count -eq 0) { "PASS" } else { "FAIL" }

$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add("task_id: P5-T1")
$lines.Add("task_name: sqlite-only full regression and compatibility warning acceptance")
$lines.Add("target_mode: automated_sqlite_only")
$lines.Add("env_id: $EnvId")
$lines.Add("runtime_mode: sqlite_only")
$lines.Add("run_date: $runDate")
$lines.Add("tester: $Tester")
$lines.Add("script: qa-gates-codex/scripts/run-p5-t1-sqlite-regression.ps1")
$lines.Add("structure: checks -> logs -> warning_acceptance -> conclusion")
$lines.Add("")
$lines.Add("checks:")

foreach ($check in $checks) {
  $lines.Add("- id: $($check.Id)")
  $lines.Add("  command: $($check.Command)")
  $lines.Add("  result: $($check.Result)")
  $lines.Add("  note: $($check.Note)")
  $lines.Add("  stdout_log: $($check.StdoutLog)")
  $lines.Add("  stderr_log: $($check.StderrLog)")
  if ($check.ExpectedPatterns.Count -gt 0) {
    $lines.Add("  expected_patterns: $($check.ExpectedPatterns -join " | ")")
  }
  if ($check.MissingPatterns.Count -gt 0) {
    $lines.Add("  missing_patterns: $($check.MissingPatterns -join " | ")")
  }
}

$lines.Add("")
$lines.Add("warning_acceptance:")
$lines.Add("- rust_runtime_mode_ignored_assert: $((($checks | Where-Object { $_.Id -eq 'rust_warning_assert_runtime_mode_ignored' }).Result))")
$lines.Add("- rust_postgres_dsn_ignored_assert: $((($checks | Where-Object { $_.Id -eq 'rust_warning_assert_postgres_dsn_ignored' }).Result))")
$lines.Add("- ts_runtime_mode_ignored_assert: $((($checks | Where-Object { $_.Id -eq 'ts_warning_assert_runtime_mode_ignored' }).Result))")
$lines.Add("- ts_legacy_warning_collection_assert: $((($checks | Where-Object { $_.Id -eq 'ts_warning_assert_postgres_mode_warning_collection' }).Result))")
$lines.Add("- warning_acceptance_verdict: $warningVerdict")
$lines.Add("")
$lines.Add("conclusion: $overall")
$lines.Add("overall: $overall")

Set-Content -Path $reportPath -Value $lines -Encoding ascii

Write-Output ("report_path={0}" -f $reportPath)
Write-Output ("log_dir={0}" -f $logDir)
Write-Output ("overall={0}" -f $overall)

if ($overall -ne "PASS") {
  exit 1
}
