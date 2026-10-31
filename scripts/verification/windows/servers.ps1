#Requires -Version 5.1
<#
.SYNOPSIS
  Disposable Windows smoke tests for Apache, MySQL, and PostgreSQL.

.DESCRIPTION
  Stage 0 probe S0-WIN-SERVERS. The default action is Help and does not
  download anything. -Action Verify (or All, Apache, MySQL, PostgreSQL)
  downloads official archives, checks a publisher digest, then launches
  binaries only from this task's artifact root.

  Loopback high ports only. No Windows service install. No edits to a
  system httpd.conf, my.ini, or PostgreSQL data directory. Cleanup stops
  only processes this script started.
#>
[CmdletBinding()]
param(
  [ValidateSet('Help', 'Verify', 'All', 'Apache', 'MySQL', 'PostgreSQL')]
  [string]$Action = 'Help',

  [string]$ArtifactRoot = 'D:\_work\laragon_alternative\.artifacts\S0-WIN-SERVERS',

  [int]$ApachePort = 18080,
  [int]$ProxyUpstreamPort = 18081,
  [int]$MySqlPort = 13306,
  [int]$PostgreSqlPort = 15432,
  [int]$OccupiedPort = 18099
)

Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$script:UserAgent = 'S0-WIN-SERVERS disposable package probe'
$script:Curl = Join-Path $env:SystemRoot 'System32\curl.exe'
$script:Taskkill = Join-Path $env:SystemRoot 'System32\taskkill.exe'
$script:Netstat = Join-Path $env:SystemRoot 'System32\netstat.exe'
$script:PowerShellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$script:OriginalPath = [string]$env:PATH
$script:Secrets = New-Object System.Collections.Generic.List[string]
$script:Owned = New-Object System.Collections.Generic.List[object]
$script:LogWriters = New-Object System.Collections.Generic.List[System.IO.TextWriter]
$script:LogFile = $null
$script:ResultsPath = $null
$script:BindAddress = '127.0.0.1'

function Write-Log {
  param([string]$Message)
  $line = '{0} {1}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $Message
  Write-Host $line
  if ($script:LogFile) {
    Add-Content -Path $script:LogFile -Value $line -Encoding ASCII
  }
}

function Join-Parts {
  param([string[]]$Parts)
  $path = $Parts[0]
  for ($i = 1; $i -lt $Parts.Count; $i++) {
    $path = Join-Path $path $Parts[$i]
  }
  return $path
}

function ConvertTo-ForwardSlash {
  param([string]$Path)
  return ($Path -replace '\\', '/')
}

function Format-ArgLine {
  param([string[]]$Arguments)
  $parts = New-Object System.Collections.Generic.List[string]
  foreach ($arg in $Arguments) {
    $text = [string]$arg
    if ($text -match '[\s"]') {
      $escaped = $text.Replace('"', '\"')
      if ($escaped.EndsWith('\')) { $escaped += '\' }
      $parts.Add('"' + $escaped + '"')
    }
    else {
      $parts.Add($text)
    }
  }
  return ($parts -join ' ')
}

function Redact-Text {
  param([string]$Text)
  if (-not $Text) { return '' }
  $clean = $Text
  foreach ($secret in $script:Secrets) {
    if ($secret -and $secret.Length -ge 4) {
      $clean = $clean.Replace($secret, '[redacted]')
    }
  }
  return $clean
}

function New-ProbeSecret {
  $alphabet = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'
  $rng = New-Object System.Security.Cryptography.RNGCryptoServiceProvider
  $bytes = New-Object byte[] 24
  $rng.GetBytes($bytes)
  $builder = New-Object System.Text.StringBuilder
  foreach ($byte in $bytes) {
    [void]$builder.Append($alphabet[$byte % $alphabet.Length])
  }
  $value = $builder.ToString()
  $script:Secrets.Add($value)
  return $value
}

function Assert-PortNumber {
  param([int]$Port, [string]$Name)
  if ($Port -eq 80 -or $Port -eq 443 -or $Port -lt 1024 -or $Port -gt 65535) {
    throw "$Name must be a user port other than 80 or 443 (got $Port)."
  }
}

function Show-Help {
  Write-Output @'
S0-WIN-SERVERS disposable Windows server probe

Default action is Help. Nothing is downloaded until -Action is Verify, All,
Apache, MySQL, or PostgreSQL.

Parameters:
  -Action Help|Verify|All|Apache|MySQL|PostgreSQL   (default Help)
  -ArtifactRoot   disposable root (default D:\_work\laragon_alternative\.artifacts\S0-WIN-SERVERS)
  -ApachePort     Apache listen port (default 18080, bound to 127.0.0.1)
  -ProxyUpstreamPort  tiny local upstream (default 18081, 127.0.0.1)
  -MySqlPort      MySQL port (default 13306, 127.0.0.1)
  -PostgreSqlPort PostgreSQL port (default 15432, 127.0.0.1)
  -OccupiedPort   spare port this script binds to demonstrate a conflict (default 18099)

The script does not install Windows services, does not edit system server
configuration, and does not stop a listener it did not start. If a requested
port is already taken, that check fails and the occupant is left running.

Examples:
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\verification\windows\servers.ps1
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\verification\windows\servers.ps1 -Action Verify
'@
}

function Receive-Url {
  param(
    [string]$Url,
    [string]$Destination,
    [int]$MaxTimeSec = 180
  )
  $parent = Split-Path -Parent $Destination
  if ($parent -and -not (Test-Path $parent)) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
  }
  if (Test-Path $Destination) {
    Remove-Item -Path $Destination -Force
  }
  Write-Log "GET $Url"
  & $script:Curl -fsSL --retry 3 --retry-delay 2 --max-time $MaxTimeSec -A $script:UserAgent -o $Destination $Url
  if ($LASTEXITCODE -ne 0) {
    throw "Download failed (curl exit $LASTEXITCODE) for $Url"
  }
  if (-not (Test-Path $Destination) -or (Get-Item $Destination).Length -le 0) {
    throw "Download was empty for $Url"
  }
}

function Read-TextFile {
  param([string]$Path)
  return [System.IO.File]::ReadAllText($Path)
}

function Get-RedirectLocation {
  param([string]$Url)
  Write-Log "HEAD $Url"
  $raw = & $script:Curl -sI --max-time 60 -A $script:UserAgent $Url
  $text = ($raw | Out-String)
  $match = [regex]::Match($text, '(?im)^Location:\s*(\S+)')
  if (-not $match.Success) {
    throw "No Location redirect from $Url"
  }
  return $match.Groups[1].Value.Trim()
}

function Get-StatusCode {
  param([string]$Url)
  $headerDump = Join-Parts @($ArtifactRoot, 'logs', 'last-head.txt')
  $code = & $script:Curl -s --head -D $headerDump -o NUL -w '%{http_code}' --max-time 60 -A $script:UserAgent $Url
  return ("$code").Trim()
}

function Get-PublishedDigest {
  param(
    [string]$Text,
    [int]$HexLength,
    [string]$FileName
  )
  if ($Text -notlike "*$FileName*") {
    throw "Publisher checksum text does not name $FileName"
  }
  $pattern = '(?m)^([0-9A-Fa-f]{' + $HexLength + '})(\s+\S+)?\s*$'
  $match = [regex]::Match($Text, $pattern)
  if (-not $match.Success) {
    throw "No $HexLength-hex digest for $FileName in the publisher checksum"
  }
  return $match.Groups[1].Value.ToUpperInvariant()
}

function Test-ZipEntriesSafe {
  param([string]$ZipPath)
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $archive = [System.IO.Compression.ZipFile]::OpenRead($ZipPath)
  try {
    foreach ($entry in $archive.Entries) {
      $name = ($entry.FullName -replace '\\', '/')
      if ($name.StartsWith('/') -or $name.StartsWith('//') -or $name -match '(^|/)\.\.(/|$)') {
        throw "Unsafe archive path '$($entry.FullName)' in $ZipPath. Not extracting."
      }
    }
  }
  finally {
    $archive.Dispose()
  }
}

function Expand-VerifiedZip {
  param(
    [string]$ZipPath,
    [string]$Destination,
    [string]$Digest
  )
  $marker = Join-Path $Destination '.verified-digest'
  if ((Test-Path $marker) -and ((Get-Content -Path $marker -Raw).Trim().ToUpperInvariant() -eq $Digest)) {
    Write-Log "Reusing verified extract $Destination"
    return
  }
  Test-ZipEntriesSafe -ZipPath $ZipPath
  if (Test-Path $Destination) {
    $stamped = '{0}.previous-{1}' -f $Destination, (Get-Date -Format 'yyyyMMddHHmmss')
    Write-Log "Moving previous extract to $stamped"
    Rename-Item -Path $Destination -NewName (Split-Path -Leaf $stamped)
  }
  New-Item -ItemType Directory -Force -Path $Destination | Out-Null
  Write-Log "Extracting $ZipPath"
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  [System.IO.Compression.ZipFile]::ExtractToDirectory($ZipPath, $Destination)
  Set-Content -Path $marker -Value $Digest -Encoding ASCII
}

function Find-FirstFile {
  param([string]$Root, [string]$Name)
  if (-not (Test-Path $Root)) { return $null }
  $direct = @(
    (Join-Path $Root $Name),
    (Join-Path (Join-Path $Root 'bin') $Name)
  )
  foreach ($candidate in $direct) {
    if (Test-Path -LiteralPath $candidate) { return Get-Item -LiteralPath $candidate }
  }
  $top = @(Get-ChildItem -LiteralPath $Root -Directory -ErrorAction SilentlyContinue)
  foreach ($dir in $top) {
    $bin = Join-Path (Join-Path $dir.FullName 'bin') $Name
    if (Test-Path -LiteralPath $bin) { return Get-Item -LiteralPath $bin }
    $nested = @(Get-ChildItem -LiteralPath $dir.FullName -Directory -ErrorAction SilentlyContinue)
    foreach ($sub in $nested) {
      $deeper = Join-Path (Join-Path $sub.FullName 'bin') $Name
      if (Test-Path -LiteralPath $deeper) { return Get-Item -LiteralPath $deeper }
    }
  }
  return $null
}

function Save-OwnedPids {
  $path = Join-Parts @($ArtifactRoot, 'owned-pids.json')
  $rows = @()
  foreach ($item in $script:Owned) {
    $rows += @{ pid = $item.pid; label = $item.label }
  }
  $json = ConvertTo-Json -InputObject @($rows) -Depth 4
  Set-Content -Path $path -Value $json -Encoding ASCII
}

function Register-OwnedProcess {
  param([int]$ProcessId, [string]$Label)
  if ($ProcessId -le 0) { return }
  $script:Owned.Add([pscustomobject]@{ pid = $ProcessId; label = $Label })
  Save-OwnedPids
  Write-Log "Tracking $Label pid $ProcessId"
}

function Test-ProcessAlive {
  param([int]$ProcessId)
  if ($ProcessId -le 0) { return $false }
  return $null -ne (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)
}

function Get-DescendantIds {
  param([int]$ProcessId)
  $all = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name)
  $ids = New-Object System.Collections.Generic.List[int]
  $ids.Add($ProcessId)
  $added = $true
  while ($added) {
    $added = $false
    foreach ($row in $all) {
      $child = [int]$row.ProcessId
      $parent = [int]$row.ParentProcessId
      if ($ids.Contains($parent) -and -not $ids.Contains($child)) {
        $ids.Add($child)
        $added = $true
      }
    }
  }
  return @($ids.ToArray())
}

function Stop-OwnedProcess {
  param([int]$ProcessId, [string]$Reason)
  if ($ProcessId -le 0) { return }
  $proc = Get-CimInstance Win32_Process -Filter "ProcessId=$ProcessId" -ErrorAction SilentlyContinue
  if (-not $proc) { return }
  $command = [string]$proc.CommandLine
  $rootLower = $ArtifactRoot.ToLowerInvariant()
  $commandLower = ''
  if ($command) { $commandLower = $command.ToLowerInvariant() }
  if (-not $commandLower.Contains($rootLower)) {
    Write-Log "Refusing to stop pid $ProcessId ($Reason); command line is not under the artifact root."
    return
  }
  Write-Log "Stopping owned pid $ProcessId ($Reason)"
  & $script:Taskkill /PID $ProcessId /T | Out-Null
  Start-Sleep -Seconds 2
  if (Test-ProcessAlive -ProcessId $ProcessId) {
    & $script:Taskkill /PID $ProcessId /T /F | Out-Null
    Start-Sleep -Seconds 1
  }
}

function Stop-PreviousOwned {
  $path = Join-Parts @($ArtifactRoot, 'owned-pids.json')
  if (-not (Test-Path $path)) { return }
  Write-Log 'Stopping leftover processes from an earlier run of this script'
  $data = Get-Content -Path $path -Raw | ConvertFrom-Json
  foreach ($item in @($data)) {
    $procId = 0
    if ($item -is [System.Management.Automation.PSCustomObject] -and $item.PSObject.Properties['pid']) {
      $procId = [int]$item.pid
    }
    Stop-OwnedProcess -ProcessId $procId -Reason 'previous run'
  }
}

function Invoke-Captured {
  param(
    [string]$FilePath,
    [string[]]$Arguments,
    [string]$WorkingDirectory,
    [hashtable]$ExtraEnvironment,
    [int]$TimeoutSec = 180
  )
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $FilePath
  $psi.Arguments = Format-ArgLine -Arguments $Arguments
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  if ($WorkingDirectory) { $psi.WorkingDirectory = $WorkingDirectory }
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  if ($ExtraEnvironment) {
    foreach ($key in @($ExtraEnvironment.Keys)) {
      $psi.EnvironmentVariables[$key] = [string]$ExtraEnvironment[$key]
    }
  }
  $proc = New-Object System.Diagnostics.Process
  $proc.StartInfo = $psi
  [void]$proc.Start()
  $outTask = $proc.StandardOutput.ReadToEndAsync()
  $errTask = $proc.StandardError.ReadToEndAsync()
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 200
  }
  if (-not $proc.HasExited) {
    try { $proc.Kill() } catch { }
    Start-Sleep -Seconds 1
    throw "Timed out after $TimeoutSec seconds: $FilePath"
  }
  $stdout = ''
  $stderr = ''
  if ($outTask.Wait(1500)) { $stdout = Redact-Text $outTask.Result }
  if ($errTask.Wait(1500)) { $stderr = Redact-Text $errTask.Result }
  return [pscustomobject]@{
    ExitCode = $proc.ExitCode
    Stdout   = $stdout
    Stderr   = $stderr
    Id       = $proc.Id
    Command  = Format-ArgLine -Arguments (@($FilePath) + $Arguments)
  }
}

function Start-Background {
  param(
    [string]$FilePath,
    [string[]]$Arguments,
    [string]$WorkingDirectory,
    [string]$Label
  )
  $stamp = Get-Date -Format 'yyyyMMddHHmmssfff'
  $stdoutPath = Join-Parts @($ArtifactRoot, 'logs', "$Label-$stamp.stdout.log")
  $stderrPath = Join-Parts @($ArtifactRoot, 'logs', "$Label-$stamp.stderr.log")
  $argumentLine = Format-ArgLine -Arguments $Arguments
  Write-Log "Start $Label $FilePath $argumentLine"
  $params = @{
    FilePath               = $FilePath
    ArgumentList           = $argumentLine
    WindowStyle            = 'Hidden'
    PassThru               = $true
    RedirectStandardOutput = $stdoutPath
    RedirectStandardError  = $stderrPath
  }
  if ($WorkingDirectory) { $params.WorkingDirectory = $WorkingDirectory }
  $proc = Start-Process @params
  if (-not $proc -or -not $proc.Id) {
    throw "Failed to start $Label ($FilePath)"
  }
  Register-OwnedProcess -ProcessId $proc.Id -Label $Label
  return $proc
}

function Test-PortAvailable {
  param([int]$Port)
  $listener = $null
  try {
    $listener = New-Object System.Net.Sockets.TcpListener ([System.Net.IPAddress]::Parse($script:BindAddress), $Port)
    $listener.Start()
    return $true
  }
  catch {
    return $false
  }
  finally {
    if ($null -ne $listener) {
      try { $listener.Stop() } catch { }
    }
  }
}

function Wait-PortOpen {
  param([int]$Port, [int]$TimeoutSec)
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  do {
    $client = New-Object System.Net.Sockets.TcpClient
    try {
      $client.Connect($script:BindAddress, $Port)
      return $true
    }
    catch {
      Start-Sleep -Milliseconds 400
    }
    finally {
      $client.Close()
    }
  } while ((Get-Date) -lt $deadline)
  return $false
}

function Wait-PortClosed {
  param([int]$Port, [int]$TimeoutSec)
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  do {
    if (Test-PortAvailable -Port $Port) {
      $client = New-Object System.Net.Sockets.TcpClient
      $open = $false
      try {
        $client.Connect($script:BindAddress, $Port)
        $open = $true
      }
      catch {
        $open = $false
      }
      finally {
        $client.Close()
      }
      if (-not $open) { return $true }
    }
    Start-Sleep -Milliseconds 400
  } while ((Get-Date) -lt $deadline)
  return $false
}

function Get-PortOwners {
  param([int]$Port)
  $rows = & $script:Netstat -ano -p tcp
  $owners = @()
  foreach ($line in $rows) {
    $match = [regex]::Match($line, '^\s*TCP\s+(\S+):(\d+)\s+\S+\s+LISTENING\s+(\d+)\s*$')
    if ($match.Success -and [int]$match.Groups[2].Value -eq $Port) {
      $owners += @{
        localAddress = $match.Groups[1].Value
        pid          = [int]$match.Groups[3].Value
      }
    }
  }
  return $owners
}

function Assert-OwnedNotOnPrivilegedPorts {
  param([int[]]$OwnedIds)
  foreach ($port in @(80, 443)) {
    foreach ($owner in @(Get-PortOwners -Port $port)) {
      if ($OwnedIds -contains [int]$owner.pid) {
        throw "Owned pid $($owner.pid) is listening on privileged port $port. Stopping that process."
      }
    }
  }
}

function Get-ServiceSnapshot {
  $rows = @()
  $services = @(Get-CimInstance Win32_Service | Where-Object {
      $_.Name -match 'apache|mysql|postgres|httpd' -or $_.DisplayName -match 'Apache|MySQL|Postgre'
    })
  foreach ($service in $services) {
    $rows += @{
      name      = [string]$service.Name
      state     = [string]$service.State
      startMode = [string]$service.StartMode
      pathName  = [string]$service.PathName
    }
  }
  return $rows
}

function Test-ServiceSnapshotsEqual {
  param($Before, $After)
  $left = @($Before | ForEach-Object { '{0}|{1}|{2}|{3}' -f $_.name, $_.state, $_.startMode, $_.pathName })
  $right = @($After | ForEach-Object { '{0}|{1}|{2}|{3}' -f $_.name, $_.state, $_.startMode, $_.pathName })
  $leftText = ($left | Sort-Object) -join "`n"
  $rightText = ($right | Sort-Object) -join "`n"
  return ($leftText -eq $rightText)
}

function Assert-PortsFree {
  param([int[]]$Ports, [string]$Component)
  foreach ($port in $Ports) {
    if (-not (Test-PortAvailable -Port $port)) {
      throw "$Component check failed: $script:BindAddress`:$port is already in use. The occupant was left running."
    }
  }
}

function Get-Tail {
  param([string]$Text, [int]$Lines = 40)
  if (-not $Text) { return '' }
  $split = @($Text -split "`r?`n")
  $start = [Math]::Max(0, $split.Length - $Lines)
  $slice = @($split[$start..($split.Length - 1)])
  return ($slice -join "`n")
}

function Resolve-ApachePackage {
  $listing = Join-Parts @($ArtifactRoot, 'sources', 'apachelounge-fetch.html')
  Receive-Url -Url 'https://www.apachelounge.com/download/' -Destination $listing -MaxTimeSec 90
  $html = Read-TextFile -Path $listing
  $zipMatch = [regex]::Match($html, 'href="([^"]*httpd-[^"]*Win64-VS18\.zip)"', 'IgnoreCase')
  if (-not $zipMatch.Success) {
    throw 'Apache Lounge download page did not list a Win64 VS18 httpd zip. Apache is blocked.'
  }
  $href = $zipMatch.Groups[1].Value
  if ($href.StartsWith('/')) { $href = 'https://www.apachelounge.com' + $href }
  $fileName = [System.IO.Path]::GetFileName(($href -split '\?')[0])
  $sumMatch = [regex]::Match($html, 'href="([^"]*' + [regex]::Escape($fileName) + '\.txt)"', 'IgnoreCase')
  if (-not $sumMatch.Success) {
    throw "Apache Lounge page listed $fileName but no linked checksum file. Apache is blocked."
  }
  $sumHref = $sumMatch.Groups[1].Value
  if ($sumHref.StartsWith('/')) { $sumHref = 'https://www.apachelounge.com' + $sumHref }
  $vcMatch = [regex]::Match($html, '(\d+\.\d+\.\d+)\s+Visual C\+\+ Redistributable')
  if (-not $vcMatch.Success) {
    throw 'Apache Lounge page did not state a Visual C++ redistributable version. Apache launch is blocked.'
  }
  $note = 'The download page listed this Win64 VS18 httpd zip.'
  if ($fileName -notlike 'httpd-2.4.69-*Win64-VS18.zip') {
    $note = "httpd 2.4.69 Win64 VS18 was not the zip linked on the download page at fetch time. Using $fileName instead."
  }
  return @{
    fileName    = $fileName
    url         = $href
    checksumUrl = $sumHref
    algorithm   = 'SHA256'
    hexLength   = 64
    listingUrl  = 'https://www.apachelounge.com/download/'
    vcVersion   = $vcMatch.Groups[1].Value
    vcText      = 'Visual C++ Redistributable Visual Studio 2017-2026 x64'
    note        = $note
  }
}

function Get-VcRedistStatus {
  param([string]$RequiredVersion)
  $key = 'HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64'
  $parts = $RequiredVersion.Split('.')
  $needMajor = [int]$parts[0]
  $needMinor = [int]$parts[1]
  $needBuild = [int]$parts[2]
  if (-not (Test-Path $key)) {
    return @{
      installed    = $false
      meetsMinimum = $false
      version      = $null
      required     = $RequiredVersion
      key          = $key
    }
  }
  $props = Get-ItemProperty -Path $key
  $installed = ($props.Installed -eq 1)
  $major = [int]$props.Major
  $minor = [int]$props.Minor
  $build = [int]$props.Bld
  $meets = $installed -and (
    ($major -gt $needMajor) -or
    ($major -eq $needMajor -and $minor -gt $needMinor) -or
    ($major -eq $needMajor -and $minor -eq $needMinor -and $build -ge $needBuild)
  )
  return @{
    installed    = [bool]$installed
    meetsMinimum = [bool]$meets
    version      = [string]$props.Version
    major        = $major
    minor        = $minor
    build        = $build
    required     = $RequiredVersion
    key          = $key
  }
}

function Get-VerifiedArchive {
  param(
    [hashtable]$Package,
    [string]$ZipPath
  )
  $sumPath = $ZipPath + '.' + $Package.algorithm.ToLowerInvariant()
  Receive-Url -Url $Package.checksumUrl -Destination $sumPath -MaxTimeSec 90
  $sumText = Read-TextFile -Path $sumPath
  $expected = Get-PublishedDigest -Text $sumText -HexLength $Package.hexLength -FileName $Package.fileName
  $have = $null
  if (Test-Path $ZipPath) {
    $have = (Get-FileHash -Algorithm $Package.algorithm -Path $ZipPath).Hash.ToUpperInvariant()
  }
  if ($have -ne $expected) {
    $partial = $ZipPath + '.partial'
    Receive-Url -Url $Package.url -Destination $partial -MaxTimeSec 900
    $have = (Get-FileHash -Algorithm $Package.algorithm -Path $partial).Hash.ToUpperInvariant()
  if ($have -ne $expected) {
    Remove-Item -Path $partial -Force
    throw "Digest mismatch for $($Package.fileName). Expected $expected from $($Package.checksumUrl) and computed $have. The archive was not extracted."
  }
  if (Test-Path $ZipPath) { Remove-Item -Path $ZipPath -Force }
  Move-Item -Path $partial -Destination $ZipPath
  }
  else {
    Write-Log "Reusing verified download $($Package.fileName)"
  }
  Write-Log "Verified $($Package.fileName) $($Package.algorithm) $expected"
  return @{
    zipPath     = $ZipPath
    fileName    = $Package.fileName
    url         = $Package.url
    checksumUrl = $Package.checksumUrl
    algorithm   = $Package.algorithm
    digest      = $expected
    listingUrl  = $Package.listingUrl
  }
}

function New-Step {
  param([string]$Name, [bool]$Ok, [string]$Detail)
  return @{ name = $Name; ok = $Ok; detail = (Redact-Text $Detail) }
}

function Invoke-OccupiedPortCheck {
  $result = @{
    status = 'fail'
    port   = $OccupiedPort
    bind   = $script:BindAddress
  }
  Assert-PortNumber -Port $OccupiedPort -Name 'OccupiedPort'
  if (-not (Test-PortAvailable -Port $OccupiedPort)) {
    $result.message = "$script:BindAddress`:$OccupiedPort is already in use, so this script cannot bind its own spare listener. The occupant was left running."
    return $result
  }
  $scriptPath = Join-Parts @($ArtifactRoot, 'listeners', 'occupied-port.ps1')
  $source = @'
param([Parameter(Mandatory=$true)][int]$Port)
$ip = [System.Net.IPAddress]::Parse('127.0.0.1')
$listener = New-Object System.Net.Sockets.TcpListener($ip, $Port)
$listener.Start()
try {
  while ($true) { Start-Sleep -Seconds 1 }
}
finally {
  $listener.Stop()
}
'@
  Set-Content -Path $scriptPath -Value $source -Encoding ASCII
  $proc = Start-Background -FilePath $script:PowerShellExe -Arguments @(
    '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $scriptPath, '-Port', "$OccupiedPort"
  ) -WorkingDirectory (Split-Path -Parent $scriptPath) -Label 'occupied-port'
  $opened = Wait-PortOpen -Port $OccupiedPort -TimeoutSec 15
  if (-not $opened) {
    $result.message = "The spare listener did not bind $script:BindAddress`:$OccupiedPort."
    return $result
  }
  $conflict = $false
  $conflictDetail = ''
  try {
    $probe = New-Object System.Net.Sockets.TcpListener ([System.Net.IPAddress]::Parse($script:BindAddress), $OccupiedPort)
    $probe.Start()
    $probe.Stop()
    $conflictDetail = 'The second bind unexpectedly succeeded.'
  }
  catch {
    $conflict = $true
    $conflictDetail = $_.Exception.Message
  }
  $stillAlive = Test-ProcessAlive -ProcessId $proc.Id
  $stillOpen = Wait-PortOpen -Port $OccupiedPort -TimeoutSec 5
  $availableAfter = Test-PortAvailable -Port $OccupiedPort
  $result.occupantPid = $proc.Id
  $result.conflictDetected = $conflict
  $result.conflictDetail = Redact-Text $conflictDetail
  $result.occupantStillRunning = $stillAlive
  $result.portStillOpen = $stillOpen
  $result.portReportedFree = $availableAfter
  $result.message = "$script:BindAddress`:$OccupiedPort is already in use. Refusing to take it. The existing listener was left running."
  if ($conflict -and $stillAlive -and $stillOpen -and -not $availableAfter) {
    $result.status = 'pass'
  }
  else {
    $result.message = 'Occupied-port demonstration did not leave the spare listener running after reporting the conflict.'
  }
  return $result
}

function Write-ListenerScript {
  $path = Join-Parts @($ArtifactRoot, 'listeners', 'upstream.ps1')
  $source = @'
param(
  [Parameter(Mandatory=$true)][int]$Port,
  [Parameter(Mandatory=$true)][string]$Body
)
$ip = [System.Net.IPAddress]::Parse('127.0.0.1')
$listener = New-Object System.Net.Sockets.TcpListener($ip, $Port)
$listener.Start()
$encoding = [System.Text.Encoding]::ASCII
try {
  while ($true) {
    $client = $listener.AcceptTcpClient()
    try {
      $stream = $client.GetStream()
      $buffer = New-Object byte[] 4096
      $memory = New-Object System.IO.MemoryStream
      $deadline = [DateTime]::UtcNow.AddSeconds(5)
      while ([DateTime]::UtcNow -lt $deadline) {
        if ($stream.DataAvailable) {
          $count = $stream.Read($buffer, 0, $buffer.Length)
          if ($count -le 0) { break }
          $memory.Write($buffer, 0, $count)
          $text = $encoding.GetString($memory.ToArray())
          if ($text.Contains("`r`n`r`n")) { break }
        }
        else {
          Start-Sleep -Milliseconds 20
        }
      }
      $payload = $encoding.GetBytes($Body)
      $header = "HTTP/1.1 200 OK`r`nContent-Type: text/plain; charset=utf-8`r`nContent-Length: $($payload.Length)`r`nConnection: close`r`n`r`n"
      $headerBytes = $encoding.GetBytes($header)
      $stream.Write($headerBytes, 0, $headerBytes.Length)
      $stream.Write($payload, 0, $payload.Length)
      $stream.Flush()
    }
    catch {
    }
    finally {
      $client.Close()
    }
  }
}
finally {
  $listener.Stop()
}
'@
  Set-Content -Path $path -Value $source -Encoding ASCII
  return $path
}

function Invoke-HttpProbe {
  param(
    [string]$Url,
    [string]$HostHeader,
    [string]$BodyPath
  )
  $run = Invoke-Captured -FilePath $script:Curl -Arguments @(
    '-sS', '--max-time', '20', '-o', $BodyPath, '-w', '%{http_code}', '-H', "Host: $HostHeader", $Url
  ) -TimeoutSec 30
  $body = ''
  if (Test-Path $BodyPath) {
    $body = Get-Content -Path $BodyPath -Raw -ErrorAction SilentlyContinue
  }
  if (-not $body) { $body = '' }
  return @{ statusCode = ("$($run.Stdout)").Trim(); body = "$body".Trim() }
}

function Invoke-ApacheCheck {
  $result = @{
    status = 'fail'
    steps  = @()
    bind   = $script:BindAddress
    port   = $ApachePort
    proxyPort = $ProxyUpstreamPort
  }
  $package = Resolve-ApachePackage
  $result.package = @{
    fileName    = $package.fileName
    url         = $package.url
    checksumUrl = $package.checksumUrl
    algorithm   = $package.algorithm
    listingUrl  = $package.listingUrl
    note        = $package.note
  }
  $result.vcRedistRequired = @{
    version = $package.vcVersion
    text    = $package.vcText
    source  = $package.listingUrl
  }
  $vc = Get-VcRedistStatus -RequiredVersion $package.vcVersion
  $result.vcRedist = $vc
  $zip = Join-Parts @($ArtifactRoot, 'downloads', $package.fileName)
  $verified = Get-VerifiedArchive -Package $package -ZipPath $zip
  $result.package.digest = $verified.digest
  $result.steps += New-Step 'digest' $true "$($verified.algorithm) $($verified.digest) matched $($verified.checksumUrl) before extract"
  if (-not $vc.meetsMinimum) {
    $result.status = 'blocked'
    $result.message = "Apache launch blocked. Required $($package.vcText) $($package.vcVersion) x64 is not installed. This probe does not install it."
    $result.steps += New-Step 'vc-redist' $false $result.message
    return $result
  }
  $result.steps += New-Step 'vc-redist' $true "Installed $($vc.version); required $($package.vcVersion) or newer"
  Assert-PortsFree -Ports @($ApachePort, $ProxyUpstreamPort) -Component 'Apache'
  $extract = Join-Parts @($ArtifactRoot, 'space path', 'apache')
  Expand-VerifiedZip -ZipPath $verified.zipPath -Destination $extract -Digest $verified.digest
  $httpdItem = Find-FirstFile -Root $extract -Name 'httpd.exe'
  if (-not $httpdItem) { throw 'httpd.exe was not found after extract' }
  $httpd = $httpdItem.FullName
  $serverRoot = Split-Path -Parent (Split-Path -Parent $httpd)
  if ($httpd.IndexOf(' ') -lt 0) { throw "Apache binary path does not contain a space: $httpd" }
  $result.binary = $httpd
  $result.pathContainsSpace = $true
  $bin = Split-Path -Parent $httpd
  $compiled = Invoke-Captured -FilePath $httpd -Arguments @('-l') -WorkingDirectory $bin -TimeoutSec 30
  if ($compiled.ExitCode -ne 0) {
    throw "httpd -l failed.`n$($compiled.Stdout)`n$($compiled.Stderr)"
  }
  $result.steps += New-Step 'compiled-modules' $true (Get-Tail $compiled.Stdout 30)
  $moduleNames = @(
    @{ name = 'mpm_winnt_module'; file = 'mod_mpm_winnt.so'; compiledHint = 'mpm_winnt' },
    @{ name = 'authn_core_module'; file = 'mod_authn_core.so'; compiledHint = 'mod_authn_core' },
    @{ name = 'authz_core_module'; file = 'mod_authz_core.so'; compiledHint = 'mod_authz_core' },
    @{ name = 'authz_host_module'; file = 'mod_authz_host.so'; compiledHint = 'mod_authz_host' },
    @{ name = 'mime_module'; file = 'mod_mime.so'; compiledHint = 'mod_mime' },
    @{ name = 'log_config_module'; file = 'mod_log_config.so'; compiledHint = 'mod_log_config' },
    @{ name = 'alias_module'; file = 'mod_alias.so'; compiledHint = 'mod_alias' },
    @{ name = 'dir_module'; file = 'mod_dir.so'; compiledHint = 'mod_dir' },
    @{ name = 'env_module'; file = 'mod_env.so'; compiledHint = 'mod_env' },
    @{ name = 'setenvif_module'; file = 'mod_setenvif.so'; compiledHint = 'mod_setenvif' },
    @{ name = 'filter_module'; file = 'mod_filter.so'; compiledHint = 'mod_filter' },
    @{ name = 'proxy_module'; file = 'mod_proxy.so'; compiledHint = 'mod_proxy.c' },
    @{ name = 'proxy_http_module'; file = 'mod_proxy_http.so'; compiledHint = 'mod_proxy_http' }
  )
  $loadLines = @()
  foreach ($module in $moduleNames) {
    $so = Join-Path (Join-Path $serverRoot 'modules') $module.file
    $compiledIn = $compiled.Stdout -match [regex]::Escape($module.compiledHint)
    if ($compiledIn) { continue }
    if (-not (Test-Path $so)) {
      if ($module.name -eq 'proxy_module' -or $module.name -eq 'proxy_http_module') {
        throw "Required module file missing: $so"
      }
      continue
    }
    $loadLines += ('LoadModule {0} modules/{1}' -f $module.name, $module.file)
  }
  $siteA = Join-Parts @($ArtifactRoot, 'fixtures', 'site-a')
  $siteB = Join-Parts @($ArtifactRoot, 'fixtures', 'site-b')
  New-Item -ItemType Directory -Force -Path $siteA, $siteB | Out-Null
  Set-Content -Path (Join-Path $siteA 'secret-a.txt') -Value 'site-a-only' -Encoding ASCII
  Set-Content -Path (Join-Path $siteB 'other.txt') -Value 'site-b-only' -Encoding ASCII
  $logDir = Join-Parts @($ArtifactRoot, 'logs', 'apache')
  New-Item -ItemType Directory -Force -Path $logDir | Out-Null
  $confPath = Join-Parts @($ArtifactRoot, 'config', 'httpd.conf')
  $rootFwd = ConvertTo-ForwardSlash $serverRoot
  $siteAFwd = ConvertTo-ForwardSlash $siteA
  $siteBFwd = ConvertTo-ForwardSlash $siteB
  $logFwd = ConvertTo-ForwardSlash $logDir
  $mime = ConvertTo-ForwardSlash (Join-Path (Join-Path $serverRoot 'conf') 'mime.types')
  $lines = @()
  $lines += 'ServerTokens Prod'
  $lines += 'ServerSignature Off'
  $lines += "ServerRoot `"$rootFwd`""
  $lines += "Listen $script:BindAddress`:$ApachePort"
  $lines += "ServerName $script:BindAddress`:$ApachePort"
  $lines += 'AcceptFilter http none'
  $lines += 'EnableSendfile Off'
  $lines += 'EnableMMAP Off'
  $lines += 'HostnameLookups Off'
  $lines += "PidFile `"$logFwd/httpd.pid`""
  $lines += "ErrorLog `"$logFwd/error.log`""
  $lines += 'LogLevel info'
  $lines += "LogFormat `"%h %l %u %t \`"%r\`" %>s %b`" common"
  $lines += "CustomLog `"$logFwd/access.log`" common"
  if (Test-Path ($mime -replace '/', '\')) {
    $lines += "TypesConfig `"$mime`""
  }
  $lines += 'AddType text/plain .txt'
  $lines += $loadLines
  $lines += 'ProxyRequests Off'
  $lines += '<Directory />'
  $lines += '    AllowOverride none'
  $lines += '    Require all denied'
  $lines += '</Directory>'
  $lines += "<VirtualHost $script:BindAddress`:$ApachePort>"
  $lines += '    ServerName site-a.test'
  $lines += "    DocumentRoot `"$siteAFwd`""
  $lines += "    <Directory `"$siteAFwd`">"
  $lines += '        Options -Indexes'
  $lines += '        AllowOverride none'
  $lines += '        Require all granted'
  $lines += '    </Directory>'
  $lines += "    ProxyPass /upstream/ http://$script:BindAddress`:$ProxyUpstreamPort/"
  $lines += "    ProxyPassReverse /upstream/ http://$script:BindAddress`:$ProxyUpstreamPort/"
  $lines += '</VirtualHost>'
  $lines += "<VirtualHost $script:BindAddress`:$ApachePort>"
  $lines += '    ServerName site-b.test'
  $lines += "    DocumentRoot `"$siteBFwd`""
  $lines += "    <Directory `"$siteBFwd`">"
  $lines += '        Options -Indexes'
  $lines += '        AllowOverride none'
  $lines += '        Require all granted'
  $lines += '    </Directory>'
  $lines += '</VirtualHost>'
  Set-Content -Path $confPath -Value ($lines -join "`r`n") -Encoding ASCII
  $result.config = $confPath
  $result.vhosts = 'Two name-based loopback virtual hosts. site-a serves a fixture and proxies /upstream/. site-b has a different document root and no proxy.'
  $syntax = Invoke-Captured -FilePath $httpd -Arguments @('-t', '-f', $confPath) -WorkingDirectory $bin -TimeoutSec 30
  $syntaxDetail = (($syntax.Stdout + "`n" + $syntax.Stderr).Trim())
  if ($syntax.ExitCode -ne 0 -or $syntaxDetail -notmatch 'Syntax OK') {
    throw "httpd -t failed.`n$syntaxDetail"
  }
  $result.steps += New-Step 'config-test' $true $syntaxDetail
  $modules = Invoke-Captured -FilePath $httpd -Arguments @('-M', '-f', $confPath) -WorkingDirectory $bin -TimeoutSec 30
  $moduleText = $modules.Stdout + "`n" + $modules.Stderr
  $proxyOk = ($modules.ExitCode -eq 0 -and $moduleText -match '(?m)^\s*proxy_module' -and $moduleText -match '(?m)^\s*proxy_http_module')
  if (-not $proxyOk) {
    throw "mod_proxy and mod_proxy_http were not both loaded.`n$moduleText"
  }
  $result.steps += New-Step 'proxy-modules' $true 'proxy_module and proxy_http_module are loaded'
  $listenerScript = Write-ListenerScript
  $upstreamBody = 'upstream-marker-S0-WIN-SERVERS'
  $upstream = Start-Background -FilePath $script:PowerShellExe -Arguments @(
    '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $listenerScript,
    '-Port', "$ProxyUpstreamPort", '-Body', $upstreamBody
  ) -WorkingDirectory (Split-Path -Parent $listenerScript) -Label 'proxy-upstream'
  if (-not (Wait-PortOpen -Port $ProxyUpstreamPort -TimeoutSec 15)) {
    throw "Upstream listener did not open $script:BindAddress`:$ProxyUpstreamPort"
  }
  $server = Start-Background -FilePath $httpd -Arguments @('-f', $confPath) -WorkingDirectory $bin -Label 'apache-httpd'
  $result.parentPid = $server.Id
  if (-not (Wait-PortOpen -Port $ApachePort -TimeoutSec 25)) {
    $errorLog = Join-Path $logDir 'error.log'
    $tail = ''
    if (Test-Path $errorLog) { $tail = Get-Content -Path $errorLog -Raw }
    throw "Apache did not listen on $script:BindAddress`:$ApachePort.`n$(Get-Tail $tail)"
  }
  $owners = @(Get-PortOwners -Port $ApachePort)
  $loopbackOnly = $true
  foreach ($owner in $owners) {
    if ($owner.localAddress -ne $script:BindAddress) { $loopbackOnly = $false }
  }
  if (-not $loopbackOnly) { throw "Apache is not bound only to $script:BindAddress" }
  $ownedNow = @(Get-DescendantIds -ProcessId $server.Id)
  Assert-OwnedNotOnPrivilegedPorts -OwnedIds $ownedNow
  $probeDir = Join-Parts @($ArtifactRoot, 'logs', 'apache-probes')
  New-Item -ItemType Directory -Force -Path $probeDir | Out-Null
  $base = "http://$script:BindAddress`:$ApachePort"
  $secret = Invoke-HttpProbe -Url "$base/secret-a.txt" -HostHeader 'site-a.test' -BodyPath (Join-Path $probeDir 'site-a.txt')
  $leak = Invoke-HttpProbe -Url "$base/secret-a.txt" -HostHeader 'site-b.test' -BodyPath (Join-Path $probeDir 'leak.txt')
  $other = Invoke-HttpProbe -Url "$base/other.txt" -HostHeader 'site-b.test' -BodyPath (Join-Path $probeDir 'site-b.txt')
  $proxied = Invoke-HttpProbe -Url "$base/upstream/" -HostHeader 'site-a.test' -BodyPath (Join-Path $probeDir 'proxy.txt')
  $proxyLeak = Invoke-HttpProbe -Url "$base/upstream/" -HostHeader 'site-b.test' -BodyPath (Join-Path $probeDir 'proxy-leak.txt')
  $httpOk = ($secret.statusCode -eq '200' -and $secret.body -eq 'site-a-only' -and $leak.statusCode -eq '404' -and $leak.body -ne 'site-a-only' -and $other.statusCode -eq '200' -and $other.body -eq 'site-b-only')
  $proxyOk = ($proxied.statusCode -eq '200' -and $proxied.body -eq $upstreamBody -and $proxyLeak.body -ne $upstreamBody)
  $result.http = @{
    siteA = $secret
    siteBLeak = $leak
    siteB = $other
    proxy = $proxied
    siteBProxy = $proxyLeak
  }
  if (-not $httpOk) { throw "Apache virtual host HTTP checks failed. site-a=$($secret.statusCode) leak=$($leak.statusCode) site-b=$($other.statusCode)" }
  if (-not $proxyOk) { throw "Apache proxy check failed. proxy=$($proxied.statusCode) body=$($proxied.body) site-b-proxy=$($proxyLeak.statusCode)" }
  $result.steps += New-Step 'http-and-proxy' $true 'site-a fixture, site-b isolation, and upstream proxy succeeded'
  $childrenBefore = @(Get-DescendantIds -ProcessId $server.Id)
  Stop-OwnedProcess -ProcessId $server.Id -Reason 'apache restart'
  if (-not (Wait-PortClosed -Port $ApachePort -TimeoutSec 20)) {
    throw 'Apache did not release its port during restart'
  }
  $server = Start-Background -FilePath $httpd -Arguments @('-f', $confPath) -WorkingDirectory $bin -Label 'apache-httpd-restart'
  if (-not (Wait-PortOpen -Port $ApachePort -TimeoutSec 25)) {
    throw 'Apache did not listen after restart'
  }
  $secret2 = Invoke-HttpProbe -Url "$base/secret-a.txt" -HostHeader 'site-a.test' -BodyPath (Join-Path $probeDir 'site-a-restart.txt')
  $proxied2 = Invoke-HttpProbe -Url "$base/upstream/" -HostHeader 'site-a.test' -BodyPath (Join-Path $probeDir 'proxy-restart.txt')
  if ($secret2.statusCode -ne '200' -or $secret2.body -ne 'site-a-only' -or $proxied2.body -ne $upstreamBody) {
    throw 'Apache HTTP or proxy failed after restart'
  }
  $result.steps += New-Step 'restart' $true 'Stopped and started Apache; HTTP and proxy succeeded again'
  $childrenRestart = @(Get-DescendantIds -ProcessId $server.Id)
  Stop-OwnedProcess -ProcessId $server.Id -Reason 'apache stop'
  $portFree = Wait-PortClosed -Port $ApachePort -TimeoutSec 20
  $gone = @()
  foreach ($procId in @($childrenBefore + $childrenRestart + @($upstream.Id))) {
    if ($procId -eq $upstream.Id) { continue }
    $gone += @{ pid = $procId; alive = (Test-ProcessAlive -ProcessId $procId) }
  }
  $childrenExited = $true
  foreach ($row in $gone) {
    if ($row.alive) { $childrenExited = $false }
  }
  Stop-OwnedProcess -ProcessId $upstream.Id -Reason 'upstream stop'
  $upstreamGone = -not (Test-ProcessAlive -ProcessId $upstream.Id)
  $upstreamPortFree = Wait-PortClosed -Port $ProxyUpstreamPort -TimeoutSec 15
  $result.stop = @{
    portReleased = $portFree
    children     = $gone
    childrenExited = $childrenExited
    upstreamExited = $upstreamGone
    upstreamPortReleased = $upstreamPortFree
  }
  if ($portFree -and $childrenExited -and $upstreamGone -and $upstreamPortFree) {
    $result.status = 'pass'
    $result.message = 'Apache served two hosts, proxied to the local upstream, restarted, and stopped.'
  }
  else {
    $result.message = 'Apache stop did not release the port or all child processes.'
  }
  return $result
}

function Invoke-MySqlNonEmptyRefusal {
  param(
    [string]$Mysqld,
    [string]$BaseDir,
    [string]$DataDir
  )
  New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
  Set-Content -Path (Join-Path $DataDir 'marker.txt') -Value 'nonempty' -Encoding ASCII
  $run = Invoke-Captured -FilePath $Mysqld -Arguments @(
    '--no-defaults', '--initialize-insecure', "--basedir=$BaseDir", "--datadir=$DataDir", '--console'
  ) -WorkingDirectory (Split-Path -Parent $Mysqld) -TimeoutSec 120
  $text = ($run.Stdout + "`n" + $run.Stderr)
  $created = Test-Path (Join-Path $DataDir 'ibdata1')
  $refused = ($run.ExitCode -ne 0) -and -not $created
  return @{
    ok       = [bool]$refused
    exitCode = $run.ExitCode
    detail   = (Get-Tail $text 25)
    dataDir  = $DataDir
  }
}

function Invoke-MySqlCheck {
  $result = @{
    status = 'fail'
    steps  = @()
    bind   = $script:BindAddress
    port   = $MySqlPort
  }
  $package = @{
    fileName    = 'mysql-8.4.13-winx64.zip'
    url         = 'https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.13-winx64.zip'
    checksumUrl = 'https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.13-winx64.zip.md5'
    algorithm   = 'MD5'
    hexLength   = 32
    listingUrl  = 'https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.13-winx64.zip.md5'
    note        = 'Official MySQL CDN zip and publisher MD5 sidecar. 8.4.13 is the newest 8.4 Windows x64 zip whose MD5 sidecar was published on the CDN at fetch time (8.4.14 and newer returned 404). The HTML download page returned 403 to this client. A SHA256 sidecar returned 404, so verification uses the published MD5.'
  }
  $code = Get-StatusCode -Url $package.checksumUrl
  if ($code -ne '200') {
    $result.status = 'blocked'
    $result.message = "MySQL blocked. Publisher checksum URL returned HTTP $code ($($package.checksumUrl)). The binary was not downloaded."
    return $result
  }
  $zip = Join-Parts @($ArtifactRoot, 'downloads', $package.fileName)
  $verified = Get-VerifiedArchive -Package $package -ZipPath $zip
  $result.package = @{
    fileName    = $package.fileName
    url         = $package.url
    checksumUrl = $package.checksumUrl
    algorithm   = $verified.algorithm
    digest      = $verified.digest
    listingUrl  = 'https://dev.mysql.com/downloads/mysql/8.4.html'
    cdnUrl      = $package.url
    note        = $package.note
  }
  $result.steps += New-Step 'digest' $true "MD5 $($verified.digest) matched the publisher sidecar before extract"
  Assert-PortsFree -Ports @($MySqlPort) -Component 'MySQL'
  $extract = Join-Parts @($ArtifactRoot, 'space path', 'mysql')
  Expand-VerifiedZip -ZipPath $verified.zipPath -Destination $extract -Digest $verified.digest
  $mysqldItem = Find-FirstFile -Root $extract -Name 'mysqld.exe'
  $mysqlItem = Find-FirstFile -Root $extract -Name 'mysql.exe'
  $adminItem = Find-FirstFile -Root $extract -Name 'mysqladmin.exe'
  if (-not $mysqldItem -or -not $mysqlItem -or -not $adminItem) { throw 'MySQL binaries were not found after extract' }
  $mysqld = $mysqldItem.FullName
  $mysql = $mysqlItem.FullName
  $mysqladmin = $adminItem.FullName
  if ($mysqld.IndexOf(' ') -lt 0) { throw "MySQL binary path does not contain a space: $mysqld" }
  $baseDir = Split-Path -Parent (Split-Path -Parent $mysqld)
  $result.binary = $mysqld
  $result.pathContainsSpace = $true
  $nonEmpty = Invoke-MySqlNonEmptyRefusal -Mysqld $mysqld -BaseDir $baseDir -DataDir (Join-Parts @($ArtifactRoot, 'data', 'mysql-nonempty'))
  $result.nonemptyDatadir = $nonEmpty
  if (-not $nonEmpty.ok) { throw "MySQL initialize did not refuse a nonempty data directory.`n$($nonEmpty.detail)" }
  $result.steps += New-Step 'nonempty-datadir' $true $nonEmpty.detail
  $dataDir = Join-Parts @($ArtifactRoot, 'data', 'mysql')
  $marker = Join-Path $dataDir '.initialized-by-s0'
  if (-not (Test-Path $marker)) {
    if (Test-Path $dataDir) {
      $stamped = '{0}.previous-{1}' -f $dataDir, (Get-Date -Format 'yyyyMMddHHmmss')
      Rename-Item -Path $dataDir -NewName (Split-Path -Leaf $stamped)
    }
    New-Item -ItemType Directory -Force -Path $dataDir | Out-Null
    $init = Invoke-Captured -FilePath $mysqld -Arguments @(
      '--no-defaults', '--initialize-insecure', "--basedir=$baseDir", "--datadir=$dataDir", '--console'
    ) -WorkingDirectory (Split-Path -Parent $mysqld) -TimeoutSec 240
    if ($init.ExitCode -ne 0) {
      throw "MySQL initialize failed.`n$(Get-Tail ($init.Stdout + "`n" + $init.Stderr))"
    }
    Set-Content -Path $marker -Value 'ok' -Encoding ASCII
    $result.steps += New-Step 'initialize' $true 'Initialized a new empty data directory with --no-defaults --initialize-insecure'
  }
  else {
    $result.steps += New-Step 'initialize' $true 'Reused the disposable data directory already initialized by this probe'
  }
  $again = Invoke-Captured -FilePath $mysqld -Arguments @(
    '--no-defaults', '--initialize-insecure', "--basedir=$baseDir", "--datadir=$dataDir", '--console'
  ) -WorkingDirectory (Split-Path -Parent $mysqld) -TimeoutSec 120
  $againText = $again.Stdout + "`n" + $again.Stderr
  $againRefused = ($again.ExitCode -ne 0) -and (Test-Path $marker)
  $result.initializedDatadirRefusal = @{
    ok       = [bool]$againRefused
    exitCode = $again.ExitCode
    detail   = (Get-Tail $againText 20)
  }
  if (-not $againRefused) { throw "MySQL initialize did not refuse the already initialized data directory.`n$(Get-Tail $againText)" }
  $result.steps += New-Step 'initialized-datadir-refusal' $true (Get-Tail $againText 12)
  $password = New-ProbeSecret
  $secretDir = Join-Parts @($ArtifactRoot, 'secrets')
  New-Item -ItemType Directory -Force -Path $secretDir | Out-Null
  $passwordPath = Join-Path $secretDir 'mysql-password.txt'
  Set-Content -Path $passwordPath -Value $password -Encoding ASCII
  $initSql = Join-Path $secretDir 'mysql-init.sql'
  $sql = @"
ALTER USER 'root'@'localhost' IDENTIFIED BY '$password';
CREATE USER IF NOT EXISTS 'probe'@'127.0.0.1' IDENTIFIED BY '$password';
ALTER USER 'probe'@'127.0.0.1' IDENTIFIED BY '$password';
GRANT ALL PRIVILEGES ON *.* TO 'probe'@'127.0.0.1' WITH GRANT OPTION;
"@
  Set-Content -Path $initSql -Value $sql -Encoding ASCII
  $clientCnf = Join-Path $secretDir 'mysql-client.cnf'
  $client = @"
[client]
protocol=TCP
host=127.0.0.1
port=$MySqlPort
user=probe
password=$password
"@
  Set-Content -Path $clientCnf -Value $client -Encoding ASCII
  $result.credentialFile = $passwordPath
  $logDir = Join-Parts @($ArtifactRoot, 'logs', 'mysql')
  $secureDir = Join-Parts @($ArtifactRoot, 'data', 'mysql-files')
  New-Item -ItemType Directory -Force -Path $logDir, $secureDir | Out-Null
  $ini = Join-Parts @($ArtifactRoot, 'config', 'my.ini')
  $iniText = @"
[mysqld]
basedir="$(ConvertTo-ForwardSlash $baseDir)"
datadir="$(ConvertTo-ForwardSlash $dataDir)"
bind-address=127.0.0.1
port=$MySqlPort
mysqlx=0
skip-name-resolve
skip-log-bin
named-pipe=0
shared-memory=0
secure-file-priv="$(ConvertTo-ForwardSlash $secureDir)"
log-error="$(ConvertTo-ForwardSlash (Join-Path $logDir 'error.log'))"
pid-file="$(ConvertTo-ForwardSlash (Join-Path $logDir 'mysql.pid'))"
"@
  Set-Content -Path $ini -Value $iniText -Encoding ASCII
  $result.config = $ini
  $server = Start-Background -FilePath $mysqld -Arguments @("--defaults-file=$ini", "--init-file=$initSql", '--console') -WorkingDirectory (Split-Path -Parent $mysqld) -Label 'mysqld'
  $result.pid = $server.Id
  if (-not (Wait-PortOpen -Port $MySqlPort -TimeoutSec 90)) {
    $errorLog = Join-Path $logDir 'error.log'
    $tail = ''
    if (Test-Path $errorLog) { $tail = Get-Content -Path $errorLog -Raw }
    $stderrLog = Get-ChildItem -Path (Join-Parts @($ArtifactRoot, 'logs')) -Filter 'mysqld-*.stderr.log' -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($stderrLog) { $tail = $tail + "`n" + (Get-Content -Path $stderrLog.FullName -Raw -ErrorAction SilentlyContinue) }
    throw "MySQL did not listen on $script:BindAddress`:$MySqlPort.`n$(Get-Tail (Redact-Text $tail))"
  }
  $owners = @(Get-PortOwners -Port $MySqlPort)
  foreach ($owner in $owners) {
    if ($owner.localAddress -ne $script:BindAddress) { throw "MySQL is not bound only to $script:BindAddress" }
  }
  Assert-OwnedNotOnPrivilegedPorts -OwnedIds @(Get-DescendantIds -ProcessId $server.Id)
  $query = 'SELECT 1 AS probe, @@version AS version, @@port AS port, @@bind_address AS bind_address'
  $first = Invoke-Captured -FilePath $mysql -Arguments @("--defaults-file=$clientCnf", '--batch', '--raw', '-e', $query) -WorkingDirectory (Split-Path -Parent $mysql) -TimeoutSec 30
  if ($first.ExitCode -ne 0) { throw "MySQL query failed.`n$($first.Stdout)`n$($first.Stderr)" }
  $result.query = ($first.Stdout.Trim())
  $result.steps += New-Step 'query' $true $result.query
  $children = @(Get-DescendantIds -ProcessId $server.Id)
  $shutdown = Invoke-Captured -FilePath $mysqladmin -Arguments @("--defaults-file=$clientCnf", 'shutdown') -WorkingDirectory (Split-Path -Parent $mysqladmin) -TimeoutSec 60
  if (-not (Wait-PortClosed -Port $MySqlPort -TimeoutSec 40)) {
    throw "MySQL shutdown did not release the port.`n$($shutdown.Stdout)`n$($shutdown.Stderr)"
  }
  $server = Start-Background -FilePath $mysqld -Arguments @("--defaults-file=$ini", '--console') -WorkingDirectory (Split-Path -Parent $mysqld) -Label 'mysqld-restart'
  if (-not (Wait-PortOpen -Port $MySqlPort -TimeoutSec 90)) { throw 'MySQL did not listen after restart' }
  $second = Invoke-Captured -FilePath $mysql -Arguments @("--defaults-file=$clientCnf", '--batch', '--raw', '-e', $query) -WorkingDirectory (Split-Path -Parent $mysql) -TimeoutSec 30
  if ($second.ExitCode -ne 0) { throw "MySQL query failed after restart.`n$($second.Stdout)`n$($second.Stderr)" }
  $result.queryAfterRestart = ($second.Stdout.Trim())
  $result.steps += New-Step 'restart' $true $result.queryAfterRestart
  $childrenRestart = @(Get-DescendantIds -ProcessId $server.Id)
  $shutdown2 = Invoke-Captured -FilePath $mysqladmin -Arguments @("--defaults-file=$clientCnf", 'shutdown') -WorkingDirectory (Split-Path -Parent $mysqladmin) -TimeoutSec 60
  $portFree = Wait-PortClosed -Port $MySqlPort -TimeoutSec 40
  $gone = @()
  $childrenExited = $true
  foreach ($procId in @($children + $childrenRestart)) {
    $alive = Test-ProcessAlive -ProcessId $procId
    $gone += @{ pid = $procId; alive = $alive }
    if ($alive) { $childrenExited = $false }
  }
  $result.stop = @{
    portReleased   = $portFree
    children       = $gone
    childrenExited = $childrenExited
    shutdownExit   = $shutdown2.ExitCode
  }
  if ($portFree -and $childrenExited) {
    $result.status = 'pass'
    $result.message = 'MySQL initialized disposable data, refused a nonempty data directory, answered a query, restarted, and stopped.'
  }
  else {
    $result.message = 'MySQL stop did not release the port or all processes.'
  }
  return $result
}

function Invoke-PostgreSqlCheck {
  $result = @{
    status = 'fail'
    steps  = @()
    bind   = $script:BindAddress
    port   = $PostgreSqlPort
  }
  $listingPath = Join-Parts @($ArtifactRoot, 'sources', 'edb-binaries-fetch.html')
  Receive-Url -Url 'https://www.enterprisedb.com/download-postgresql-binaries' -Destination $listingPath -MaxTimeSec 90
  $html = Read-TextFile -Path $listingPath
  $index = $html.IndexOf('Version <!-- -->18.')
  if ($index -lt 0) {
    $result.status = 'blocked'
    $result.message = 'PostgreSQL blocked. https://www.enterprisedb.com/download-postgresql-binaries did not list a PostgreSQL 18 Windows binary.'
    $result.listingUrl = 'https://www.enterprisedb.com/download-postgresql-binaries'
    return $result
  }
  $slice = $html.Substring($index, [Math]::Min(4000, $html.Length - $index))
  $versionMatch = [regex]::Match($slice, 'Version <!-- -->(18\.\d+)')
  $linkMatch = [regex]::Match($slice, 'href="(https://sbp\.enterprisedb\.com/getfile\.jsp\?fileid=\d+)"[^>]*>\s*<img[^>]*alt="Windows x86-64"')
  if (-not $versionMatch.Success -or -not $linkMatch.Success) {
    $result.status = 'blocked'
    $result.message = 'PostgreSQL blocked. The EDB binaries page did not expose a PostgreSQL 18 Windows x86-64 link.'
    $result.listingUrl = 'https://www.enterprisedb.com/download-postgresql-binaries'
    return $result
  }
  $fileIdUrl = $linkMatch.Groups[1].Value
  $finalUrl = Get-RedirectLocation -Url $fileIdUrl
  if ($finalUrl.IndexOf('get.enterprisedb.com') -lt 0 -or $finalUrl -notmatch '\.zip($|\?)') {
    $result.status = 'blocked'
    $result.message = "PostgreSQL blocked. The EDB file link did not redirect to a get.enterprisedb.com zip ($finalUrl)."
    return $result
  }
  $fileUrl = ($finalUrl -split '\?')[0]
  $fileName = [System.IO.Path]::GetFileName($fileUrl)
  $checksumUrl = $fileUrl + '.sha256'
  $checksumCode = Get-StatusCode -Url $checksumUrl
  if ($checksumCode -ne '200') {
    $result.status = 'blocked'
    $result.message = "PostgreSQL blocked. No publisher SHA256 sidecar at $checksumUrl (HTTP $checksumCode). The zip was not downloaded."
    $result.listingUrl = 'https://www.enterprisedb.com/download-postgresql-binaries'
    $result.fileIdUrl = $fileIdUrl
    $result.url = $fileUrl
    return $result
  }
  $package = @{
    fileName    = $fileName
    url         = $fileUrl
    checksumUrl = $checksumUrl
    algorithm   = 'SHA256'
    hexLength   = 64
    listingUrl  = 'https://www.enterprisedb.com/download-postgresql-binaries'
    note        = "PostgreSQL $($versionMatch.Groups[1].Value) Windows x86-64 from the EDB binaries page via $fileIdUrl. Windows 10 is not in the postgresql.org tested-platform table for major 18 (Server 2025 and 2022 are). The launch below is the actual result on this machine."
  }
  $zip = Join-Parts @($ArtifactRoot, 'downloads', $package.fileName)
  $verified = Get-VerifiedArchive -Package $package -ZipPath $zip
  $result.package = @{
    fileName    = $fileName
    url         = $fileUrl
    checksumUrl = $checksumUrl
    algorithm   = 'SHA256'
    digest      = $verified.digest
    listingUrl  = $package.listingUrl
    fileIdUrl   = $fileIdUrl
    version     = $versionMatch.Groups[1].Value
    note        = $package.note
    platformNote = 'postgresql.org Windows installer table lists Postgres 18 on Server 2025 and 2022, and says desktop releases can generally be expected to run. Windows 10 is not an explicitly tested platform there.'
  }
  $result.steps += New-Step 'digest' $true "SHA256 $($verified.digest) matched $checksumUrl before extract"
  Assert-PortsFree -Ports @($PostgreSqlPort) -Component 'PostgreSQL'
  $extract = Join-Parts @($ArtifactRoot, 'space path', 'postgresql')
  Expand-VerifiedZip -ZipPath $verified.zipPath -Destination $extract -Digest $verified.digest
  $initdbItem = Find-FirstFile -Root $extract -Name 'initdb.exe'
  $ctlItem = Find-FirstFile -Root $extract -Name 'pg_ctl.exe'
  $psqlItem = Find-FirstFile -Root $extract -Name 'psql.exe'
  if (-not $initdbItem -or -not $ctlItem -or -not $psqlItem) { throw 'PostgreSQL binaries were not found after extract' }
  $initdb = $initdbItem.FullName
  $pgCtl = $ctlItem.FullName
  $psql = $psqlItem.FullName
  if ($initdb.IndexOf(' ') -lt 0) { throw "PostgreSQL binary path does not contain a space: $initdb" }
  $bin = Split-Path -Parent $initdb
  $lib = Join-Path (Split-Path -Parent $bin) 'lib'
  $childPath = $bin
  if (Test-Path $lib) { $childPath = $bin + ';' + $lib }
  $childPath = $childPath + ';' + $script:OriginalPath
  $childEnv = @{
    Path        = $childPath
    LC_MESSAGES = 'C'
    LC_ALL      = 'C'
    LANGUAGE    = 'en'
  }
  $result.binary = $initdb
  $result.pathContainsSpace = $true
  $nonEmptyDir = Join-Parts @($ArtifactRoot, 'data', 'postgres-nonempty')
  New-Item -ItemType Directory -Force -Path $nonEmptyDir | Out-Null
  Set-Content -Path (Join-Path $nonEmptyDir 'marker.txt') -Value 'nonempty' -Encoding ASCII
  $refuse = Invoke-Captured -FilePath $initdb -Arguments @('--no-locale', '--encoding=UTF8', '-D', $nonEmptyDir, '-U', 'postgres', '--auth-host=scram-sha-256') -WorkingDirectory $bin -ExtraEnvironment $childEnv -TimeoutSec 60
  $refuseText = $refuse.Stdout + "`n" + $refuse.Stderr
  $versionFile = Join-Path $nonEmptyDir 'PG_VERSION'
  $refuseOk = ($refuse.ExitCode -ne 0) -and -not (Test-Path $versionFile)
  $result.nonemptyDatadir = @{ ok = [bool]$refuseOk; exitCode = $refuse.ExitCode; detail = (Get-Tail $refuseText 15); dataDir = $nonEmptyDir }
  if (-not $refuseOk) { throw "initdb did not refuse a nonempty data directory.`n$(Get-Tail $refuseText)" }
  $result.steps += New-Step 'nonempty-datadir' $true (Get-Tail $refuseText 8)
  $dataDir = Join-Parts @($ArtifactRoot, 'data', 'postgres')
  $secretDir = Join-Parts @($ArtifactRoot, 'secrets')
  New-Item -ItemType Directory -Force -Path $secretDir | Out-Null
  $passwordPath = Join-Path $secretDir 'postgres-password.txt'
  $pwFile = Join-Path $secretDir 'postgres-pwfile.txt'
  $marker = Join-Path $dataDir 'PG_VERSION'
  if ((Test-Path $marker) -and (Test-Path $passwordPath)) {
    $password = (Get-Content -Path $passwordPath -Raw).Trim()
    $script:Secrets.Add($password)
    Set-Content -Path $pwFile -Value $password -Encoding ASCII
    $result.steps += New-Step 'initdb' $true 'Reused the disposable data directory already initialized by this probe'
  }
  else {
    if (Test-Path $dataDir) {
      $stamped = '{0}.previous-{1}' -f $dataDir, (Get-Date -Format 'yyyyMMddHHmmss')
      Rename-Item -Path $dataDir -NewName (Split-Path -Leaf $stamped)
    }
    $password = New-ProbeSecret
    Set-Content -Path $passwordPath -Value $password -Encoding ASCII
    Set-Content -Path $pwFile -Value $password -Encoding ASCII
    $init = Invoke-Captured -FilePath $initdb -Arguments @(
      '--no-locale', '--encoding=UTF8', '-D', $dataDir, '-U', 'postgres',
      "--pwfile=$pwFile", '--auth-local=scram-sha-256', '--auth-host=scram-sha-256'
    ) -WorkingDirectory $bin -ExtraEnvironment $childEnv -TimeoutSec 180
    if ($init.ExitCode -ne 0) {
      throw "initdb failed.`n$(Get-Tail ($init.Stdout + "`n" + $init.Stderr))"
    }
    $result.steps += New-Step 'initdb' $true 'initdb created a new disposable data directory'
  }
  $result.credentialFile = $passwordPath
  $conf = Join-Path $dataDir 'postgresql.conf'
  $hba = Join-Path $dataDir 'pg_hba.conf'
  $existingConf = ''
  if (Test-Path $conf) { $existingConf = Get-Content -Path $conf -Raw }
  if ($existingConf -notlike '*S0-WIN-SERVERS disposable probe overrides*') {
    $override = @"

# S0-WIN-SERVERS disposable probe overrides
listen_addresses = '127.0.0.1'
port = $PostgreSqlPort
"@
    Add-Content -Path $conf -Value $override -Encoding ASCII
  }
  Set-Content -Path $hba -Value "host all all 127.0.0.1/32 scram-sha-256`r`n" -Encoding ASCII
  $result.config = @{ postgresqlConf = $conf; pgHba = $hba }
  $logFile = Join-Parts @($ArtifactRoot, 'logs', 'postgres', 'pg_ctl.log')
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $logFile) | Out-Null
  $start = Invoke-Captured -FilePath $pgCtl -Arguments @('start', '-D', $dataDir, '-l', $logFile, '-w', '-t', '60') -WorkingDirectory $bin -ExtraEnvironment $childEnv -TimeoutSec 90
  if ($start.ExitCode -ne 0) {
    $logTail = ''
    if (Test-Path $logFile) { $logTail = Get-Content -Path $logFile -Raw }
    throw "pg_ctl start failed.`n$($start.Stdout)`n$($start.Stderr)`n$(Get-Tail $logTail)"
  }
  $pidFile = Join-Path $dataDir 'postmaster.pid'
  if (-not (Test-Path $pidFile)) { throw 'postmaster.pid was not written' }
  $postmasterPid = [int]((Get-Content -Path $pidFile -TotalCount 1).Trim())
  Register-OwnedProcess -ProcessId $postmasterPid -Label 'postgres'
  $result.pid = $postmasterPid
  if (-not (Wait-PortOpen -Port $PostgreSqlPort -TimeoutSec 20)) { throw "PostgreSQL did not listen on $script:BindAddress`:$PostgreSqlPort" }
  foreach ($owner in @(Get-PortOwners -Port $PostgreSqlPort)) {
    if ($owner.localAddress -ne $script:BindAddress) { throw "PostgreSQL is not bound only to $script:BindAddress" }
  }
  Assert-OwnedNotOnPrivilegedPorts -OwnedIds @(Get-DescendantIds -ProcessId $postmasterPid)
  $query = "SELECT 1 AS probe, current_setting('port') AS port, inet_server_addr() AS addr"
  $psqlArgs = @('-w', '-h', $script:BindAddress, '-p', "$PostgreSqlPort", '-U', 'postgres', '-d', 'postgres', '-v', 'ON_ERROR_STOP=1', '-c', $query)
  $psqlEnv = @{ Path = $childPath; PGPASSWORD = $password }
  $first = Invoke-Captured -FilePath $psql -Arguments $psqlArgs -WorkingDirectory $bin -ExtraEnvironment $psqlEnv -TimeoutSec 30
  if ($first.ExitCode -ne 0) { throw "psql query failed.`n$($first.Stdout)`n$($first.Stderr)" }
  $result.query = ($first.Stdout.Trim())
  if ($result.query -notmatch [regex]::Escape("$PostgreSqlPort") -or $result.query -notmatch '127\.0\.0\.1') {
    throw "PostgreSQL query did not show the loopback port.`n$($result.query)"
  }
  $result.steps += New-Step 'query' $true $result.query
  $children = @(Get-DescendantIds -ProcessId $postmasterPid)
  $restart = Invoke-Captured -FilePath $pgCtl -Arguments @('restart', '-D', $dataDir, '-l', $logFile, '-m', 'fast', '-w', '-t', '60') -WorkingDirectory $bin -ExtraEnvironment $childEnv -TimeoutSec 90
  if ($restart.ExitCode -ne 0) { throw "pg_ctl restart failed.`n$($restart.Stdout)`n$($restart.Stderr)" }
  if (-not (Wait-PortOpen -Port $PostgreSqlPort -TimeoutSec 20)) { throw 'PostgreSQL did not listen after restart' }
  $postmasterPid = [int]((Get-Content -Path $pidFile -TotalCount 1).Trim())
  Register-OwnedProcess -ProcessId $postmasterPid -Label 'postgres-restart'
  $second = Invoke-Captured -FilePath $psql -Arguments $psqlArgs -WorkingDirectory $bin -ExtraEnvironment $psqlEnv -TimeoutSec 30
  if ($second.ExitCode -ne 0) { throw "psql query failed after restart.`n$($second.Stdout)`n$($second.Stderr)" }
  $result.queryAfterRestart = ($second.Stdout.Trim())
  $result.steps += New-Step 'restart' $true $result.queryAfterRestart
  $childrenRestart = @(Get-DescendantIds -ProcessId $postmasterPid)
  $stop = Invoke-Captured -FilePath $pgCtl -Arguments @('stop', '-D', $dataDir, '-m', 'fast', '-w', '-t', '60') -WorkingDirectory $bin -ExtraEnvironment $childEnv -TimeoutSec 90
  $portFree = Wait-PortClosed -Port $PostgreSqlPort -TimeoutSec 20
  $gone = @()
  $childrenExited = $true
  foreach ($procId in @($children + $childrenRestart)) {
    $alive = Test-ProcessAlive -ProcessId $procId
    $gone += @{ pid = $procId; alive = $alive }
    if ($alive) { $childrenExited = $false }
  }
  $result.stop = @{
    portReleased   = $portFree
    children       = $gone
    childrenExited = $childrenExited
    stopExit       = $stop.ExitCode
  }
  if ($stop.ExitCode -eq 0 -and $portFree -and $childrenExited) {
    $result.status = 'pass'
    $result.message = 'PostgreSQL initdb, query, restart, and fast shutdown succeeded on disposable data.'
  }
  else {
    $result.message = 'PostgreSQL stop did not release the port or all processes.'
  }
  return $result
}

function Invoke-Cleanup {
  $stopped = @()
  $snapshot = New-Object System.Collections.ArrayList
  for ($i = 0; $i -lt $script:Owned.Count; $i++) {
    [void]$snapshot.Add($script:Owned[$i])
  }
  for ($i = $snapshot.Count - 1; $i -ge 0; $i--) {
    $item = $snapshot[$i]
    $aliveBefore = Test-ProcessAlive -ProcessId $item.pid
    if ($aliveBefore) {
      Stop-OwnedProcess -ProcessId $item.pid -Reason $item.label
    }
    $stopped += @{
      pid          = $item.pid
      label        = $item.label
      wasAlive     = $aliveBefore
      aliveAfter   = (Test-ProcessAlive -ProcessId $item.pid)
    }
  }
  return @{
    stopped = $stopped
    note    = 'Cleanup stops only tracked processes whose command line contains this artifact root. Archives, extracts, data, logs, and secrets were retained.'
  }
}

function Save-Results {
  param($Results)
  try {
    $json = ConvertTo-Json -InputObject $Results -Depth 8
    Set-Content -Path $script:ResultsPath -Value $json -Encoding UTF8
  }
  catch {
    $fallback = Join-Parts @($ArtifactRoot, 'logs', 'results-write-error.txt')
    Set-Content -Path $fallback -Value (Redact-Text $_.Exception.ToString()) -Encoding ASCII
    throw
  }
}

if ($Action -eq 'Help') {
  Show-Help
  exit 0
}

Assert-PortNumber -Port $ApachePort -Name 'ApachePort'
Assert-PortNumber -Port $ProxyUpstreamPort -Name 'ProxyUpstreamPort'
Assert-PortNumber -Port $MySqlPort -Name 'MySqlPort'
Assert-PortNumber -Port $PostgreSqlPort -Name 'PostgreSqlPort'
Assert-PortNumber -Port $OccupiedPort -Name 'OccupiedPort'
$ports = @($ApachePort, $ProxyUpstreamPort, $MySqlPort, $PostgreSqlPort, $OccupiedPort)
if (@($ports | Select-Object -Unique).Count -ne $ports.Count) {
  throw 'Server and occupied-port values must be distinct.'
}

$ArtifactRoot = [System.IO.Path]::GetFullPath($ArtifactRoot)
if ($ArtifactRoot.ToLowerInvariant().Contains('s0-win-servers') -eq $false) {
  throw "ArtifactRoot must stay under an S0-WIN-SERVERS directory. Got $ArtifactRoot"
}
New-Item -ItemType Directory -Force -Path @(
  $ArtifactRoot
  (Join-Parts @($ArtifactRoot, 'downloads'))
  (Join-Parts @($ArtifactRoot, 'sources'))
  (Join-Parts @($ArtifactRoot, 'space path'))
  (Join-Parts @($ArtifactRoot, 'config'))
  (Join-Parts @($ArtifactRoot, 'data'))
  (Join-Parts @($ArtifactRoot, 'logs'))
  (Join-Parts @($ArtifactRoot, 'listeners'))
  (Join-Parts @($ArtifactRoot, 'secrets'))
) | Out-Null

$script:LogFile = Join-Parts @($ArtifactRoot, 'logs', 'script.log')
$script:ResultsPath = Join-Parts @($ArtifactRoot, 'results.json')

$osKey = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$results = @{
  task         = 'S0-WIN-SERVERS'
  action       = $Action
  startedAt    = (Get-Date).ToString('o')
  artifactRoot = $ArtifactRoot
  os           = @{
    productName = [string]$osKey.ProductName
    displayVersion = [string]$osKey.DisplayVersion
    build       = [string]$osKey.CurrentBuild
    ubr         = [string]$osKey.UBR
    edition     = [string]$osKey.EditionID
    architecture = [string]$env:PROCESSOR_ARCHITECTURE
  }
  ports        = @{
    apache         = $ApachePort
    proxyUpstream  = $ProxyUpstreamPort
    mysql          = $MySqlPort
    postgresql     = $PostgreSqlPort
    occupied       = $OccupiedPort
    bind           = $script:BindAddress
  }
  occupiedPort = @{ status = 'skipped' }
  apache       = @{ status = 'skipped' }
  mysql        = @{ status = 'skipped' }
  postgresql   = @{ status = 'skipped' }
}

$exitCode = 0
try {
  Write-Log "S0-WIN-SERVERS action $Action"
  Stop-PreviousOwned
  $results.servicesBefore = @(Get-ServiceSnapshot)
  $results.occupiedPort = Invoke-OccupiedPortCheck
  $runApache = $Action -in @('Verify', 'All', 'Apache')
  $runMysql = $Action -in @('Verify', 'All', 'MySQL')
  $runPostgres = $Action -in @('Verify', 'All', 'PostgreSQL')
  if ($runApache) {
    try { $results.apache = Invoke-ApacheCheck }
    catch {
      $results.apache = @{ status = 'fail'; message = (Redact-Text $_.Exception.Message) }
      Write-Log "Apache failed: $($results.apache.message)"
    }
  }
  if ($runMysql) {
    try { $results.mysql = Invoke-MySqlCheck }
    catch {
      $results.mysql = @{ status = 'fail'; message = (Redact-Text $_.Exception.Message) }
      Write-Log "MySQL failed: $($results.mysql.message)"
    }
  }
  if ($runPostgres) {
    try { $results.postgresql = Invoke-PostgreSqlCheck }
    catch {
      $results.postgresql = @{ status = 'fail'; message = (Redact-Text $_.Exception.Message) }
      Write-Log "PostgreSQL failed: $($results.postgresql.message)"
    }
  }
}
catch {
  $results.fatal = Redact-Text $_.Exception.Message
  Write-Log "Fatal: $($results.fatal)"
  $exitCode = 1
}
finally {
  try {
    Write-Log 'cleanup begin'
    $results.cleanup = Invoke-Cleanup
    Write-Log 'cleanup done'
    try { $results.servicesAfter = @(Get-ServiceSnapshot) }
    catch { $results.servicesAfter = @() }
    $results.serviceStatesUnchanged = Test-ServiceSnapshotsEqual -Before $results.servicesBefore -After $results.servicesAfter
    $results.pathUnchanged = ($env:PATH -eq $script:OriginalPath)
    $results.finishedAt = (Get-Date).ToString('o')
    if (-not $results.serviceStatesUnchanged) {
      $results.serviceWarning = 'A pre-existing Apache, MySQL, or PostgreSQL service changed state or configuration during this probe.'
      $exitCode = 1
    }
    if (-not $results.pathUnchanged) {
      $results.pathWarning = 'The process PATH changed and was not restored.'
      $exitCode = 1
    }
    Save-Results -Results $results
    Write-Log "Wrote $script:ResultsPath"
  }
  catch {
    $trace = $_.Exception.ToString() + "`r`n" + $_.ScriptStackTrace
    try {
      [System.IO.File]::WriteAllText((Join-Parts @($ArtifactRoot, 'logs', 'finally-error.txt')), $trace)
    }
    catch { }
    Write-Log "finally failed: $($_.Exception.Message)"
    $exitCode = 1
  }
}

foreach ($name in @('occupiedPort', 'apache', 'mysql', 'postgresql')) {
  $item = $results[$name]
  if ($item -and $item.status -and $item.status -ne 'pass' -and $item.status -ne 'skipped') {
    $exitCode = 1
  }
}
if ($results.ContainsKey('fatal') -and $results.fatal) { $exitCode = 1 }
exit ([int]$exitCode)
