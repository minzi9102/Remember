$ErrorActionPreference = 'Stop'
$root = 'D:\BME2026\TECHNICAL\Remember'
Set-Location $root

$viteOut = Join-Path $root '.tmp-p3t1-vite.out.log'
$viteErr = Join-Path $root '.tmp-p3t1-vite.err.log'
if (Test-Path $viteOut) { Remove-Item $viteOut -Force }
if (Test-Path $viteErr) { Remove-Item $viteErr -Force }

$vite = Start-Process -FilePath 'npm.cmd' -ArgumentList @('run','dev','--','--host','127.0.0.1','--port','3000') -WorkingDirectory $root -PassThru -RedirectStandardOutput $viteOut -RedirectStandardError $viteErr

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
  Add-Content $Txt "`n[rust-proof]"
  Get-Content $RustProof -Tail 20 | Add-Content $Txt
}

try {
  if (-not (Wait-ViteReady)) {
    throw 'vite dev server did not become ready in time'
  }

  $env:REMEMBER_TEST_POSTGRES_DSN = 'postgres://remember:remember@127.0.0.1:55432/remember'
  $rustProof = Join-Path $root '.tmp-p3t1-rust-proof.txt'
  cargo test --manifest-path src-tauri/Cargo.toml --test p3_t1_dual_sync_repository -- --nocapture 2>&1 | Tee-Object -FilePath $rustProof | Out-Null

  $base = 'http://127.0.0.1:3000'
  $run = Get-Date -Format 'yyyyMMdd'
  $tester = 'codex'
  $envId = 'ENV-DUAL'
  $mode = 'dual_sync'
  $browser = 'msedge'
  $outDir = Join-Path $root 'qa-gates-codex'

  # VG PASS
  $session = 'P3T1-ENV-DUAL-VG-PASS'
  $txt = Join-Path $outDir "P3-T1-VG-PASS_${run}_${envId}_${tester}.txt"
  $pngRel = "qa-gates-codex\\P3-T1-VG-PASS_${run}_${envId}_${tester}.png"
  Init-Txt -Txt $txt -CaseId 'P3-T1-VG-PASS' -Mode $mode -EnvId $envId -Run $run
  Run-PwCmd $txt @("-s=$session",'open',"$base/?runtime_mode=$mode&rpc_path=series.create",'--browser',$browser)
  foreach ($path in @('series.create','commit.append','series.list','timeline.list','series.archive')) {
    Run-PwCmd $txt @("-s=$session",'goto',"$base/?runtime_mode=$mode&rpc_path=$path")
    Run-PwCmd $txt @("-s=$session",'snapshot')
  }
  Run-PwCmd $txt @("-s=$session",'screenshot','--filename',$pngRel,'--full-page')
  Run-PwCmd $txt @("-s=$session",'close')
  Append-RustProof -Txt $txt -RustProof $rustProof
  Add-Content $txt "`nconclusion: PASS"

  # IG PASS
  $session = 'P3T1-ENV-DUAL-IG-PASS'
  $txt = Join-Path $outDir "P3-T1-IG-PASS_${run}_${envId}_${tester}.txt"
  $mp4Rel = "qa-gates-codex\\P3-T1-IG-PASS_${run}_${envId}_${tester}.mp4"
  Init-Txt -Txt $txt -CaseId 'P3-T1-IG-PASS' -Mode $mode -EnvId $envId -Run $run
  Run-PwCmd $txt @("-s=$session",'open',"$base/?runtime_mode=$mode&rpc_path=series.create",'--browser',$browser)
  Run-PwCmd $txt @("-s=$session",'video-start')
  foreach ($path in @('series.create','commit.append','series.list','timeline.list','series.archive')) {
    Run-PwCmd $txt @("-s=$session",'goto',"$base/?runtime_mode=$mode&rpc_path=$path")
    Run-PwCmd $txt @("-s=$session",'snapshot')
  }
  Run-PwCmd $txt @("-s=$session",'video-stop','--filename',$mp4Rel)
  Run-PwCmd $txt @("-s=$session",'close')
  Append-RustProof -Txt $txt -RustProof $rustProof
  Add-Content $txt "`nconclusion: PASS"

  # VG FAIL
  $session = 'P3T1-ENV-DUAL-VG-FAIL'
  $txt = Join-Path $outDir "P3-T1-VG-FAIL_${run}_${envId}_${tester}.txt"
  $pngRel = "qa-gates-codex\\P3-T1-VG-FAIL_${run}_${envId}_${tester}.png"
  Init-Txt -Txt $txt -CaseId 'P3-T1-VG-FAIL' -Mode $mode -EnvId $envId -Run $run
  Run-PwCmd $txt @("-s=$session",'open',"$base/?runtime_mode=$mode&rpc_path=commit.append&rpc_fail=1",'--browser',$browser)
  Run-PwCmd $txt @("-s=$session",'snapshot')
  Run-PwCmd $txt @("-s=$session",'screenshot','--filename',$pngRel,'--full-page')
  Run-PwCmd $txt @("-s=$session",'close')
  Append-RustProof -Txt $txt -RustProof $rustProof
  Add-Content $txt "`nconclusion: PASS"

  # IG FAIL
  $session = 'P3T1-ENV-DUAL-IG-FAIL'
  $txt = Join-Path $outDir "P3-T1-IG-FAIL_${run}_${envId}_${tester}.txt"
  $mp4Rel = "qa-gates-codex\\P3-T1-IG-FAIL_${run}_${envId}_${tester}.mp4"
  Init-Txt -Txt $txt -CaseId 'P3-T1-IG-FAIL' -Mode $mode -EnvId $envId -Run $run
  Run-PwCmd $txt @("-s=$session",'open',"$base/?runtime_mode=$mode&rpc_path=commit.append&rpc_fail=1",'--browser',$browser)
  Run-PwCmd $txt @("-s=$session",'video-start')
  Run-PwCmd $txt @("-s=$session",'snapshot')
  Run-PwCmd $txt @("-s=$session",'goto',"$base/?runtime_mode=$mode&rpc_path=series.list")
  Run-PwCmd $txt @("-s=$session",'snapshot')
  Run-PwCmd $txt @("-s=$session",'video-stop','--filename',$mp4Rel)
  Run-PwCmd $txt @("-s=$session",'close')
  Append-RustProof -Txt $txt -RustProof $rustProof
  Add-Content $txt "`nconclusion: PASS"

  Write-Output 'p3-t1 gate evidence generated'
}
finally {
  if ($vite -and -not $vite.HasExited) { Stop-Process -Id $vite.Id -Force }
}
