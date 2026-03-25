param(
  [string]$AuthToken = "remember-local-dev-token"
)

$ErrorActionPreference = "Stop"
$nativePreferenceVar = Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue
if ($null -ne $nativePreferenceVar) {
  $previousNativePreference = $PSNativeCommandUseErrorActionPreference
  $PSNativeCommandUseErrorActionPreference = $false
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$serverPath = Join-Path $repoRoot "target\debug\remember-ipc-server.exe"
$cliPath = Join-Path $repoRoot "target\debug\remember-cli.exe"

$tempRoot = [System.IO.Path]::GetFullPath($env:TEMP)
$tempDir = Join-Path $env:TEMP ("remember-b6-t1-" + [Guid]::NewGuid().ToString("N"))
$resolvedTempDir = [System.IO.Path]::GetFullPath($tempDir)
if (-not $resolvedTempDir.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "refusing to use temp dir outside TEMP root: $resolvedTempDir"
}
New-Item -ItemType Directory -Path $resolvedTempDir -Force | Out-Null

$previousAppDataDir = $env:REMEMBER_APPDATA_DIR
$previousAuthToken = $env:REMEMBER_IPC_AUTH_TOKEN
$previousLoopback = $env:REMEMBER_ENABLE_LOOPBACK
$env:REMEMBER_APPDATA_DIR = $resolvedTempDir
$env:REMEMBER_IPC_AUTH_TOKEN = $AuthToken
$env:REMEMBER_ENABLE_LOOPBACK = "1"

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

function Assert-Condition {
  param(
    [bool]$Condition,
    [string]$Message
  )
  if (-not $Condition) {
    throw $Message
  }
}

function Invoke-RpcEnvelope {
  param(
    [string]$RpcPath,
    [hashtable]$Payload
  )
  $payloadJson = $Payload | ConvertTo-Json -Compress -Depth 20
  $rpcResult = Invoke-CliCommand -CommandArgs @(
    "rpc", "call",
    "--path", $RpcPath,
    "--payload", $payloadJson,
    "--transport", "loopback"
  )
  if ($rpcResult.ExitCode -ne 0) {
    $errorText = if ([string]::IsNullOrWhiteSpace($rpcResult.StdErr)) { $rpcResult.StdOut } else { $rpcResult.StdErr }
    throw "rpc call failed for ${RpcPath}: $errorText"
  }

  try {
    return $rpcResult.StdOut | ConvertFrom-Json
  }
  catch {
    throw "failed to parse rpc response for ${RpcPath}: $($rpcResult.StdOut)"
  }
}

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

$server = Start-Process -FilePath $serverPath -PassThru -WindowStyle Hidden -WorkingDirectory $repoRoot
try {
  $healthy = $false
  for ($i = 0; $i -lt 20; $i++) {
    $health = Invoke-CliCommand -CommandArgs @("health", "--transport", "loopback")
    if ($health.ExitCode -eq 0) {
      $healthy = $true
      break
    }
    Start-Sleep -Milliseconds 300
  }
  Assert-Condition -Condition $healthy -Message "ipc server health probe failed"

  $seriesName = "B6-T1_Regression"
  $create = Invoke-RpcEnvelope -RpcPath "series.create" -Payload @{ name = $seriesName }
  Assert-Condition -Condition $create.ok -Message "series.create should succeed"
  $seriesId = [string]$create.data.series.id
  Assert-Condition -Condition (-not [string]::IsNullOrWhiteSpace($seriesId)) -Message "series.create returned empty series id"

  $list = Invoke-RpcEnvelope -RpcPath "series.list" -Payload @{
    query = ""
    includeArchived = $false
    cursor = $null
    limit = 20
  }
  Assert-Condition -Condition $list.ok -Message "series.list should succeed"

  $clientTs = [DateTimeOffset]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
  $append = Invoke-RpcEnvelope -RpcPath "commit.append" -Payload @{
    seriesId = $seriesId
    content = "first_note_from_b6_t1"
    clientTs = $clientTs
  }
  Assert-Condition -Condition $append.ok -Message "commit.append should succeed before archive"
  $commitId = [string]$append.data.commit.id
  Assert-Condition -Condition (-not [string]::IsNullOrWhiteSpace($commitId)) -Message "commit.append returned empty commit id"

  $timeline = Invoke-RpcEnvelope -RpcPath "timeline.list" -Payload @{
    seriesId = $seriesId
    cursor = $null
    limit = 20
  }
  Assert-Condition -Condition $timeline.ok -Message "timeline.list should succeed"
  $timelineCommitId = [string]$timeline.data.items[0].id
  Assert-Condition -Condition ($timelineCommitId -eq $commitId) -Message "timeline.list did not return expected latest commit"

  $scan = Invoke-RpcEnvelope -RpcPath "series.scan_silent" -Payload @{
    now = [DateTimeOffset]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
    thresholdDays = 7
  }
  Assert-Condition -Condition $scan.ok -Message "series.scan_silent should succeed"

  $archive = Invoke-RpcEnvelope -RpcPath "series.archive" -Payload @{
    seriesId = $seriesId
  }
  Assert-Condition -Condition $archive.ok -Message "series.archive should succeed"
  Assert-Condition -Condition ([string]$archive.data.seriesId -eq $seriesId) -Message "series.archive returned mismatched series id"

  $appendAfterArchive = Invoke-RpcEnvelope -RpcPath "commit.append" -Payload @{
    seriesId = $seriesId
    content = "should_fail_after_archive"
    clientTs = [DateTimeOffset]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
  }
  Assert-Condition -Condition (-not $appendAfterArchive.ok) -Message "commit.append after archive should fail"
  Assert-Condition -Condition ([string]$appendAfterArchive.error.code -eq "CONFLICT") -Message "commit.append after archive should return CONFLICT"

  Write-Output "PASS: B6-T1 regression succeeded (6 RPC + archived append conflict)."
}
finally {
  if ($null -ne $server -and -not $server.HasExited) {
    Stop-Process -Id $server.Id -Force
  }

  if ($null -eq $previousAppDataDir) {
    Remove-Item Env:REMEMBER_APPDATA_DIR -ErrorAction SilentlyContinue
  }
  else {
    $env:REMEMBER_APPDATA_DIR = $previousAppDataDir
  }

  if ($null -eq $previousAuthToken) {
    Remove-Item Env:REMEMBER_IPC_AUTH_TOKEN -ErrorAction SilentlyContinue
  }
  else {
    $env:REMEMBER_IPC_AUTH_TOKEN = $previousAuthToken
  }

  if ($null -eq $previousLoopback) {
    Remove-Item Env:REMEMBER_ENABLE_LOOPBACK -ErrorAction SilentlyContinue
  }
  else {
    $env:REMEMBER_ENABLE_LOOPBACK = $previousLoopback
  }

  if (Test-Path $resolvedTempDir) {
    $safeResolvedTempDir = [System.IO.Path]::GetFullPath($resolvedTempDir)
    if ($safeResolvedTempDir.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
      Remove-Item -LiteralPath $safeResolvedTempDir -Recurse -Force
    }
  }
  if ($null -ne $nativePreferenceVar) {
    $PSNativeCommandUseErrorActionPreference = $previousNativePreference
  }
}
