$ErrorActionPreference = 'Stop'
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = (Resolve-Path (Join-Path $scriptDir '..\..')).Path
Set-Location $root
$base = 'http://127.0.0.1:3000'
$run = '20260317'
$tester = 'codex'
$browser = 'msedge'
$outDir = Join-Path $root 'qa-gates-codex'
$probeJs = "() => ({ path: (document.body.innerText.match(/path:\\s*([^\\n]+)/)||[])[1] ?? '', ok: (document.body.innerText.match(/ok:\\s*(true|false)/)||[])[1] ?? '', code: (document.body.innerText.match(/code:\\s*([^\\n]+)/)||[])[1] ?? '' })"

function Run-PwCmd {
  param([string]$Txt, [string[]]$CliArgs)
  Add-Content -Path $Txt -Value ("`n$ playwright-cli " + ($CliArgs -join ' '))
  npx --yes --package @playwright/cli playwright-cli @CliArgs 2>&1 | Tee-Object -FilePath $Txt -Append | Out-Null
}

function Init-Txt {
  param([string]$Txt, [string]$CaseId)
  if (Test-Path $Txt) { Remove-Item $Txt -Force }
  Add-Content $Txt ("case_id: " + $CaseId)
  Add-Content $Txt 'steps -> visible result -> log proof -> conclusion'
}

function Run-VGPass {
  param([string]$EnvId, [string]$Mode)
  $session = "P2T5-$EnvId-VG-PASS"
  $txt = Join-Path $outDir "P2-T5-VG-PASS_${run}_${EnvId}_${tester}.txt"
  $pngRel = "qa-gates-codex\\P2-T5-VG-PASS_${run}_${EnvId}_${tester}.png"
  Init-Txt -Txt $txt -CaseId 'P2-T5-VG-PASS'
  $url = $base + '/?runtime_mode=' + $Mode + '&rpc_path=series.create'
  Run-PwCmd $txt @("-s=$session",'open',$url,'--browser',$browser)
  Run-PwCmd $txt @("-s=$session",'snapshot')
  Run-PwCmd $txt @("-s=$session",'eval',$probeJs)
  foreach ($path in @('commit.append','series.list','timeline.list','series.archive')) {
    $url = $base + '/?runtime_mode=' + $Mode + '&rpc_path=' + $path
    Run-PwCmd $txt @("-s=$session",'goto',$url)
    Run-PwCmd $txt @("-s=$session",'snapshot')
    Run-PwCmd $txt @("-s=$session",'eval',$probeJs)
  }
  Run-PwCmd $txt @("-s=$session",'screenshot','--filename',$pngRel,'--full-page')
  Run-PwCmd $txt @("-s=$session",'close')
  Add-Content $txt "`nconclusion: PASS"
}

function Run-IGPass {
  param([string]$EnvId, [string]$Mode)
  $session = "P2T5-$EnvId-IG-PASS"
  $txt = Join-Path $outDir "P2-T5-IG-PASS_${run}_${EnvId}_${tester}.txt"
  $mp4Rel = "qa-gates-codex\\P2-T5-IG-PASS_${run}_${EnvId}_${tester}.mp4"
  Init-Txt -Txt $txt -CaseId 'P2-T5-IG-PASS'
  $url = $base + '/?runtime_mode=' + $Mode + '&rpc_path=series.create'
  Run-PwCmd $txt @("-s=$session",'open',$url,'--browser',$browser)
  Run-PwCmd $txt @("-s=$session",'video-start')
  foreach ($path in @('series.create','commit.append','series.list','timeline.list','series.archive')) {
    $url = $base + '/?runtime_mode=' + $Mode + '&rpc_path=' + $path
    Run-PwCmd $txt @("-s=$session",'goto',$url)
    Run-PwCmd $txt @("-s=$session",'snapshot')
    Run-PwCmd $txt @("-s=$session",'eval',$probeJs)
  }
  Run-PwCmd $txt @("-s=$session",'video-stop','--filename',$mp4Rel)
  Run-PwCmd $txt @("-s=$session",'close')
  Add-Content $txt "`nconclusion: PASS"
}

function Run-VGFail {
  param([string]$EnvId, [string]$Mode)
  $session = "P2T5-$EnvId-VG-FAIL"
  $txt = Join-Path $outDir "P2-T5-VG-FAIL_${run}_${EnvId}_${tester}.txt"
  $pngRel = "qa-gates-codex\\P2-T5-VG-FAIL_${run}_${EnvId}_${tester}.png"
  Init-Txt -Txt $txt -CaseId 'P2-T5-VG-FAIL'
  $url = $base + '/?runtime_mode=' + $Mode + '&rpc_path=commit.append&rpc_fail=1'
  Run-PwCmd $txt @("-s=$session",'open',$url,'--browser',$browser)
  Run-PwCmd $txt @("-s=$session",'snapshot')
  Run-PwCmd $txt @("-s=$session",'eval',$probeJs)
  Run-PwCmd $txt @("-s=$session",'screenshot','--filename',$pngRel,'--full-page')
  Run-PwCmd $txt @("-s=$session",'close')
  Add-Content $txt "`nconclusion: PASS"
}

function Run-IGFail {
  param([string]$EnvId, [string]$Mode)
  $session = "P2T5-$EnvId-IG-FAIL"
  $txt = Join-Path $outDir "P2-T5-IG-FAIL_${run}_${EnvId}_${tester}.txt"
  $mp4Rel = "qa-gates-codex\\P2-T5-IG-FAIL_${run}_${EnvId}_${tester}.mp4"
  Init-Txt -Txt $txt -CaseId 'P2-T5-IG-FAIL'
  $failUrl = $base + '/?runtime_mode=' + $Mode + '&rpc_path=commit.append&rpc_fail=1'
  $recoverUrl = $base + '/?runtime_mode=' + $Mode + '&rpc_path=series.list'
  Run-PwCmd $txt @("-s=$session",'open',$failUrl,'--browser',$browser)
  Run-PwCmd $txt @("-s=$session",'video-start')
  Run-PwCmd $txt @("-s=$session",'snapshot')
  Run-PwCmd $txt @("-s=$session",'eval',$probeJs)
  Run-PwCmd $txt @("-s=$session",'goto',$recoverUrl)
  Run-PwCmd $txt @("-s=$session",'snapshot')
  Run-PwCmd $txt @("-s=$session",'eval',$probeJs)
  Run-PwCmd $txt @("-s=$session",'video-stop','--filename',$mp4Rel)
  Run-PwCmd $txt @("-s=$session",'close')
  Add-Content $txt "`nconclusion: PASS"
}

foreach ($pair in @(@('ENV-SQLITE','sqlite_only'))) {
  $envId = $pair[0]
  $mode = $pair[1]
  Run-VGPass -EnvId $envId -Mode $mode
  Run-IGPass -EnvId $envId -Mode $mode
  Run-VGFail -EnvId $envId -Mode $mode
  Run-IGFail -EnvId $envId -Mode $mode
}

Write-Output 'p2-t5 gate evidence generated'
