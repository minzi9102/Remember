param(
  [string]$AuthToken = "remember-local-dev-token"
)

$ErrorActionPreference = "Stop"
$env:REMEMBER_IPC_AUTH_TOKEN = $AuthToken
$nativePreferenceVar = Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue
if ($null -ne $nativePreferenceVar) {
  $previousNativePreference = $PSNativeCommandUseErrorActionPreference
  $PSNativeCommandUseErrorActionPreference = $false
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$serverPath = Join-Path $repoRoot "target\debug\remember-ipc-server.exe"
$cliPath = Join-Path $repoRoot "target\debug\remember-cli.exe"

& cargo build --workspace
if ($LASTEXITCODE -ne 0) {
  throw "cargo build --workspace failed"
}

if (-not (Test-Path $serverPath)) {
  throw "server binary not found: $serverPath"
}
if (-not (Test-Path $cliPath)) {
  throw "cli binary not found: $cliPath"
}

function Invoke-CliCommand {
  param(
    [string[]]$CommandArgs
  )

  function ConvertTo-WindowsArgument {
    param([string]$Value)
    if ($null -eq $Value) {
      return '""'
    }
    if ($Value -notmatch '[\s"]') {
      return $Value
    }

    $builder = New-Object System.Text.StringBuilder
    [void]$builder.Append('"')
    $backslashes = 0
    foreach ($char in $Value.ToCharArray()) {
      if ($char -eq '\') {
        $backslashes++
        continue
      }

      if ($char -eq '"') {
        [void]$builder.Append(('\' * ($backslashes * 2 + 1)))
        [void]$builder.Append('"')
        $backslashes = 0
        continue
      }

      if ($backslashes -gt 0) {
        [void]$builder.Append(('\' * $backslashes))
        $backslashes = 0
      }
      [void]$builder.Append($char)
    }

    if ($backslashes -gt 0) {
      [void]$builder.Append(('\' * ($backslashes * 2)))
    }
    [void]$builder.Append('"')
    return $builder.ToString()
  }

  $psi = [System.Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $cliPath
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.Arguments = (($CommandArgs | ForEach-Object { ConvertTo-WindowsArgument -Value $_ }) -join " ")

  $process = [System.Diagnostics.Process]::Start($psi)
  $stdout = $process.StandardOutput.ReadToEnd()
  $stderr = $process.StandardError.ReadToEnd()
  $process.WaitForExit()

  [PSCustomObject]@{
    ExitCode = $process.ExitCode
    StdOut = $stdout.Trim()
    StdErr = $stderr.Trim()
  }
}

$smokePayloadJson = @{
  query = ""
  includeArchived = $false
  cursor = $null
  limit = 10
} | ConvertTo-Json -Compress -Depth 10

$server = Start-Process -FilePath $serverPath -PassThru -WindowStyle Hidden -WorkingDirectory $repoRoot
try {
  $ready = $false
  $lastHealthError = ""
  for ($i = 0; $i -lt 15; $i++) {
    $health = Invoke-CliCommand -CommandArgs @("health")
    if ($health.ExitCode -eq 0) {
      $ready = $true
      break
    }
    $lastHealthError = if ([string]::IsNullOrWhiteSpace($health.StdErr)) { $health.StdOut } else { $health.StdErr }
    Start-Sleep -Seconds 1
  }
  if (-not $ready) {
    throw "ipc server health probe failed after retries: $lastHealthError"
  }

  "healthy" | Out-Host

  $rpcOk = $false
  $lastRpcError = ""
  $rpcArgs = @("rpc", "call", "--path", "series.list", "--payload", $smokePayloadJson)
  for ($i = 0; $i -lt 8; $i++) {
    $rpc = Invoke-CliCommand -CommandArgs $rpcArgs
    if ($rpc.ExitCode -eq 0) {
      $rpcOk = $true
      $rpc.StdOut | Out-Host
      break
    }
    $lastRpcError = if ([string]::IsNullOrWhiteSpace($rpc.StdErr)) { $rpc.StdOut } else { $rpc.StdErr }
    Start-Sleep -Milliseconds 500
  }
  if (-not $rpcOk) {
    throw "rpc smoke call failed after retries: $lastRpcError"
  }
}
finally {
  if ($null -ne $server -and -not $server.HasExited) {
    Stop-Process -Id $server.Id -Force
  }
  if ($null -ne $nativePreferenceVar) {
    $PSNativeCommandUseErrorActionPreference = $previousNativePreference
  }
}
