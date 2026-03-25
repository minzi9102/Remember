param(
  [string]$AuthToken = "remember-local-dev-token"
)

$ErrorActionPreference = "Stop"
$env:REMEMBER_IPC_AUTH_TOKEN = $AuthToken

$server = Start-Process -FilePath "cargo" -ArgumentList @("run","-p","remember-ipc-server") -PassThru -WindowStyle Hidden
try {
  $ready = $false
  for ($i = 0; $i -lt 15; $i++) {
    & cargo run -p remember-cli -- health *> $null
    if ($LASTEXITCODE -eq 0) {
      $ready = $true
      break
    }
    Start-Sleep -Seconds 1
  }
  if (-not $ready) {
    throw "ipc server health probe failed after retries"
  }

  "healthy" | Out-Host

  $rpcOk = $false
  for ($i = 0; $i -lt 8; $i++) {
    & cargo run -p remember-cli -- rpc call --path series.list --payload '{"query":"","includeArchived":false,"cursor":null,"limit":10}'
    if ($LASTEXITCODE -eq 0) {
      $rpcOk = $true
      break
    }
    Start-Sleep -Milliseconds 500
  }
  if (-not $rpcOk) {
    throw "rpc smoke call failed after retries"
  }
}
finally {
  if ($null -ne $server -and -not $server.HasExited) {
    Stop-Process -Id $server.Id -Force
  }
}
