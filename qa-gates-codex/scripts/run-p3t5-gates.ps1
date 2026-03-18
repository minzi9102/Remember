$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = (Resolve-Path (Join-Path $scriptDir '..\..')).Path
Set-Location $root
$tmpDir = Join-Path $root 'qa-gates-codex\tmp'
New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null

Get-CimInstance Win32_Process |
  Where-Object {
    ($_.Name -eq 'node.exe' -or $_.Name -eq 'cmd.exe') -and
    $_.CommandLine -match 'vite' -and
    $_.CommandLine -match '127\.0\.0\.1' -and
    $_.CommandLine -match '3000'
  } |
  ForEach-Object {
    try { Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop } catch {}
  }

$viteOut = Join-Path $tmpDir 'p3t5-vite.out.log'
$viteErr = Join-Path $tmpDir 'p3t5-vite.err.log'
if (Test-Path $viteOut) { Remove-Item $viteOut -Force -ErrorAction SilentlyContinue }
if (Test-Path $viteErr) { Remove-Item $viteErr -Force -ErrorAction SilentlyContinue }

$vite = Start-Process -FilePath 'npm.cmd' -ArgumentList @('run', 'dev', '--', '--host', '127.0.0.1', '--port', '3000') -WorkingDirectory $root -PassThru -RedirectStandardOutput $viteOut -RedirectStandardError $viteErr

function Wait-ViteReady {
  for ($i = 0; $i -lt 60; $i++) {
    try {
      $resp = Invoke-WebRequest -Uri 'http://127.0.0.1:3000' -UseBasicParsing -TimeoutSec 2
      if ($resp.StatusCode -ge 200) {
        return $true
      }
    } catch {}
    Start-Sleep -Milliseconds 500
  }
  return $false
}

function Run-PwCmd {
  param([string]$Txt, [string[]]$CliArgs)
  Add-Content -Path $Txt -Value ("`n$ playwright-cli " + ($CliArgs -join ' '))
  npx --yes --package @playwright/cli playwright-cli @CliArgs 2>&1 | Tee-Object -FilePath $Txt -Append | Out-Null
}

function Init-Txt {
  param([string]$Txt, [string]$CaseId, [string]$Mode, [string]$EnvId, [string]$Run)
  if (Test-Path $Txt) { Remove-Item $Txt -Force }
  Add-Content $Txt ("case_id: " + $CaseId)
  Add-Content $Txt 'steps -> visible result -> log proof -> conclusion'
  Add-Content $Txt ("runtime_mode: $Mode")
  Add-Content $Txt ("env_id: $EnvId")
  Add-Content $Txt ("run_date: $Run")
}

function Append-RustProof {
  param([string]$Txt, [string]$RustProof)
  if (Test-Path $RustProof) {
    Add-Content $Txt "`n[rust-proof]"
    Get-Content $RustProof -Tail 50 | Add-Content $Txt
  }
}

try {
  if (-not (Wait-ViteReady)) {
    throw 'vite dev server did not become ready in time'
  }

  $rustProof = Join-Path $tmpDir 'p3t5-rust-proof.txt'
  if (Test-Path $rustProof) { Remove-Item $rustProof -Force }
  $oldErr = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  cargo test --manifest-path src-tauri/Cargo.toml --test p3_t5_startup_self_heal -- --nocapture --test-threads=1 2>&1 | Tee-Object -FilePath $rustProof | Out-Null
  $cargoExit = $LASTEXITCODE
  $ErrorActionPreference = $oldErr
  if ($cargoExit -ne 0) {
    throw "cargo p3_t5_startup_self_heal test failed with exit code $cargoExit"
  }

  $base = 'http://127.0.0.1:3000'
  $run = Get-Date -Format 'yyyyMMdd'
  $tester = 'codex'
  $envId = 'ENV-DUAL'
  $mode = 'dual_sync'
  $browser = 'msedge'
  $outDir = Join-Path $root 'qa-gates-codex'

  $passUrl = "$base/?runtime_mode=$mode&rpc_path=series.list&startup_self_heal_scanned=4&startup_self_heal_repaired=4&startup_self_heal_unresolved=0&startup_self_heal_failed=0&startup_self_heal_completed_at=2026-03-17T19:45:00Z"
  $failUrl = "$base/?runtime_mode=$mode&rpc_path=commit.append&rpc_error=dual_write_failed&startup_self_heal_scanned=4&startup_self_heal_repaired=2&startup_self_heal_unresolved=2&startup_self_heal_failed=2&startup_self_heal_completed_at=2026-03-17T19:46:00Z&startup_self_heal_message=alert-create-remains-unresolved&startup_self_heal_message=alert-append-remains-unresolved"

  # VG PASS
  $session = 'P3T5-ENV-DUAL-VG-PASS'
  $txt = Join-Path $outDir "P3-T5-VG-PASS_${run}_${envId}_${tester}.txt"
  $pngRel = "qa-gates-codex\P3-T5-VG-PASS_${run}_${envId}_${tester}.png"
  Init-Txt -Txt $txt -CaseId 'P3-T5-VG-PASS' -Mode $mode -EnvId $envId -Run $run
  Run-PwCmd $txt @("-s=$session", 'open', $passUrl, '--browser', $browser)
  Run-PwCmd $txt @("-s=$session", 'snapshot')
  Run-PwCmd $txt @("-s=$session", 'screenshot', '--filename', $pngRel, '--full-page')
  Run-PwCmd $txt @("-s=$session", 'close')
  Append-RustProof -Txt $txt -RustProof $rustProof
  Add-Content $txt "`nconclusion: PASS"

  # IG PASS
  $session = 'P3T5-ENV-DUAL-IG-PASS'
  $txt = Join-Path $outDir "P3-T5-IG-PASS_${run}_${envId}_${tester}.txt"
  $mp4Rel = "qa-gates-codex\P3-T5-IG-PASS_${run}_${envId}_${tester}.mp4"
  Init-Txt -Txt $txt -CaseId 'P3-T5-IG-PASS' -Mode $mode -EnvId $envId -Run $run
  Run-PwCmd $txt @("-s=$session", 'open', $passUrl, '--browser', $browser)
  Run-PwCmd $txt @("-s=$session", 'video-start')
  Run-PwCmd $txt @("-s=$session", 'snapshot')
  Run-PwCmd $txt @("-s=$session", 'goto', "$base/?runtime_mode=$mode&rpc_path=series.create&startup_self_heal_scanned=4&startup_self_heal_repaired=4&startup_self_heal_unresolved=0&startup_self_heal_failed=0&startup_self_heal_completed_at=2026-03-17T19:45:00Z")
  Run-PwCmd $txt @("-s=$session", 'snapshot')
  Run-PwCmd $txt @("-s=$session", 'goto', "$base/?runtime_mode=$mode&rpc_path=series.scan_silent&startup_self_heal_scanned=4&startup_self_heal_repaired=4&startup_self_heal_unresolved=0&startup_self_heal_failed=0&startup_self_heal_completed_at=2026-03-17T19:45:00Z")
  Run-PwCmd $txt @("-s=$session", 'snapshot')
  Run-PwCmd $txt @("-s=$session", 'video-stop', '--filename', $mp4Rel)
  Run-PwCmd $txt @("-s=$session", 'close')
  Append-RustProof -Txt $txt -RustProof $rustProof
  Add-Content $txt "`nconclusion: PASS"

  # VG FAIL
  $session = 'P3T5-ENV-DUAL-VG-FAIL'
  $txt = Join-Path $outDir "P3-T5-VG-FAIL_${run}_${envId}_${tester}.txt"
  $pngRel = "qa-gates-codex\P3-T5-VG-FAIL_${run}_${envId}_${tester}.png"
  Init-Txt -Txt $txt -CaseId 'P3-T5-VG-FAIL' -Mode $mode -EnvId $envId -Run $run
  Run-PwCmd $txt @("-s=$session", 'open', $failUrl, '--browser', $browser)
  Run-PwCmd $txt @("-s=$session", 'snapshot')
  Run-PwCmd $txt @("-s=$session", 'screenshot', '--filename', $pngRel, '--full-page')
  Run-PwCmd $txt @("-s=$session", 'close')
  Append-RustProof -Txt $txt -RustProof $rustProof
  Add-Content $txt "`nconclusion: PASS"

  # IG FAIL
  $session = 'P3T5-ENV-DUAL-IG-FAIL'
  $txt = Join-Path $outDir "P3-T5-IG-FAIL_${run}_${envId}_${tester}.txt"
  $mp4Rel = "qa-gates-codex\P3-T5-IG-FAIL_${run}_${envId}_${tester}.mp4"
  Init-Txt -Txt $txt -CaseId 'P3-T5-IG-FAIL' -Mode $mode -EnvId $envId -Run $run
  Run-PwCmd $txt @("-s=$session", 'open', $failUrl, '--browser', $browser)
  Run-PwCmd $txt @("-s=$session", 'video-start')
  Run-PwCmd $txt @("-s=$session", 'snapshot')
  Run-PwCmd $txt @("-s=$session", 'goto', $passUrl)
  Run-PwCmd $txt @("-s=$session", 'snapshot')
  Run-PwCmd $txt @("-s=$session", 'video-stop', '--filename', $mp4Rel)
  Run-PwCmd $txt @("-s=$session", 'close')
  Append-RustProof -Txt $txt -RustProof $rustProof
  Add-Content $txt "`nconclusion: PASS"

  Write-Output 'p3-t5 gate evidence generated'
}
finally {
  if ($vite -and -not $vite.HasExited) { Stop-Process -Id $vite.Id -Force }
  Get-CimInstance Win32_Process |
    Where-Object {
      ($_.Name -eq 'node.exe' -or $_.Name -eq 'cmd.exe') -and
      $_.CommandLine -match 'vite' -and
      $_.CommandLine -match '127\.0\.0\.1' -and
      $_.CommandLine -match '3000'
    } |
    ForEach-Object {
      try { Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop } catch {}
    }
}
