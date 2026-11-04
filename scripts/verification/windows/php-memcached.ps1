<#
.SYNOPSIS
  Verify pinned Windows PHP patches and publisher-verified memcached DLL bundles.
.DESCRIPTION
  Help creates nothing. Verify uses only .artifacts/S8A-NATIVE-CACHE, verifies
  official PHP SPDX digests and ServBay GitHub asset digests before extraction,
  and probes separate CLI/CGI profiles. Optional Port explicitly selects an
  existing loopback cache endpoint for namespaced add/set/get/delete checks.
  This script never starts/stops the selected endpoint or changes global PATH.
#>
[CmdletBinding()]
param(
    [ValidateSet('Help', 'Verify')][string]$Action = 'Help',
    [string]$ArtifactRoot = '',
    [ValidateRange(0,65535)][int]$Port = 0
)
if ($Action -eq 'Help') {
    Write-Output @'
S8A-NATIVE-CACHE PHP memcached artifact verification
Help (default): no files, downloads or process launches.
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/php-memcached.ps1 -Action Verify
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/php-memcached.ps1 -Action Verify -Port 41234
Port explicitly selects an existing 127.0.0.1 endpoint (1024-65535); only random
30-second probe keys are changed. No server ownership is inferred or acquired.
ArtifactRoot must be .artifacts/S8A-NATIVE-CACHE or a child. result.json records
payload pins, CLI/CGI profiles, actual compatibility and optional real set/get.
ServBay is a third-party publisher; its archive digest is not a PECL checksum.
'@
    exit 0
}
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$allowed = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../.artifacts/S8A-NATIVE-CACHE'))
if (-not $ArtifactRoot) { $ArtifactRoot = $allowed }
$ArtifactRoot = [IO.Path]::GetFullPath($ArtifactRoot)
function Assert-Contained([string]$Root, [string]$Path) {
    if ($Path -ne $Root -and -not $Path.StartsWith($Root.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Path escapes authorized artifact root' }
    $current = $Path
    while ($current -and $current.Length -ge $Root.Length) {
        if ((Test-Path -LiteralPath $current) -and ((Get-Item -LiteralPath $current).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Reparse point in artifact path' }
        $current = [IO.Path]::GetDirectoryName($current)
    }
}
Assert-Contained $allowed $ArtifactRoot
if ($Port -ne 0 -and $Port -lt 1024) { throw 'Probe endpoint requires a high loopback port' }
New-Item -ItemType Directory -Force -Path $ArtifactRoot | Out-Null
$utf8 = New-Object Text.UTF8Encoding $false
$results = New-Object 'Collections.Generic.List[object]'
$os = Get-CimInstance Win32_OperatingSystem
function Write-Text([string]$Path,[string]$Value) {
    Assert-Contained $ArtifactRoot ([IO.Path]::GetFullPath($Path))
    [IO.File]::WriteAllText($Path,$Value,$utf8)
}
function Record([string]$Name,[string]$Status,$Details) {
    $results.Add([ordered]@{name=$Name;status=$Status;details=$Details})
    Write-Output "$Status $Name"
}
function Fetch([string]$Url,[string]$Path) {
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $Path -Headers @{'User-Agent'='Stackyard-artifact-verification'} -TimeoutSec 120
}
function Assert-Digest([string]$Path,[string]$Expected) {
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Expected) { throw "SHA256 mismatch: $Path" }
    return $actual
}
function Expand-Verified([string]$Archive,[string]$Destination) {
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        foreach ($entry in $zip.Entries) {
            if ($entry.FullName -match '(^[/\\]|:|(^|[/\\])\.\.([/\\]|$))') { throw 'Unsafe archive entry' }
            Assert-Contained $Destination ([IO.Path]::GetFullPath((Join-Path $Destination $entry.FullName)))
        }
    } finally { $zip.Dispose() }
    Expand-Archive -LiteralPath $Archive -DestinationPath $Destination
}
function Quote-Argument([string]$Value) {
    if ($Value -and $Value -notmatch '[\s"]') { return $Value }
    return '"'+([regex]::Replace([regex]::Replace($Value,'(\\*)"','$1$1\"'),'(\\+)$','$1$1'))+'"'
}
function Probe([string]$Name,[string]$Executable,[string[]]$Arguments,[string]$NativeDirectory = '',[hashtable]$Extra = @{}) {
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $Executable
    $start.Arguments = (($Arguments | ForEach-Object { Quote-Argument $_ }) -join ' ')
    $start.WorkingDirectory = $run
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.EnvironmentVariables.Remove('PHPRC')
    $start.EnvironmentVariables['PHP_INI_SCAN_DIR'] = (Join-Path $run 'empty scan')
    # Keep the selected official PHP first; dependency DLLs live in a distinct
    # verified package. This only changes the child process environment.
    $start.EnvironmentVariables['PATH'] = [IO.Path]::GetDirectoryName($Executable)+';'+$NativeDirectory+';'+$env:PATH
    foreach ($key in $Extra.Keys) { $start.EnvironmentVariables[$key] = $Extra[$key] }
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw "Cannot start $Name" }
        $out = $process.StandardOutput.ReadToEndAsync()
        $err = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(15000)) { $process.Kill(); $process.WaitForExit(); throw "$Name exceeded 15 seconds" }
        $stdout = $out.GetAwaiter().GetResult()
        $stderr = $err.GetAwaiter().GetResult()
        Write-Text (Join-Path $run "$Name.stdout.log") $stdout
        Write-Text (Join-Path $run "$Name.stderr.log") $stderr
        return [ordered]@{executable=$Executable;arguments=$Arguments;native_directory=$NativeDirectory;exit_code=$process.ExitCode;stdout=$stdout.Trim();stderr=$stderr.Trim()}
    } finally { $process.Dispose() }
}
function Require-Success($Probe) {
    if ($Probe.exit_code -ne 0 -or $Probe.stderr -or $Probe.stdout -match 'PHP Startup:|Warning:') { throw ($Probe | ConvertTo-Json -Depth 5) }
}
try {
    if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'Windows x64 is required' }
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $run = Join-Path $ArtifactRoot ('verified run '+[guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $run | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $run 'empty scan') | Out-Null
    $pins = @(
        @{id='php8511';file='php-8.5.11-nts-Win32-vs17-x64.zip';url='https://downloads.php.net/~windows/releases/archives/php-8.5.11-nts-Win32-vs17-x64.zip';sha256='0ea96e0d2b9b737a6036f05cf4e95c49313faa6d0f27bd97edb2742503f0c043';source='https://downloads.php.net/~windows/releases/archives/php-8.5.11-nts-Win32-vs17-x64.zip.spdx.json';vendor='PHP Group'},
        @{id='php8512';file='php-8.5.12-1-nts-Win32-vs17-x64.zip';url='https://downloads.php.net/~windows/releases/archives/php-8.5.12-1-nts-Win32-vs17-x64.zip';sha256='2a36a2edcb7a5b9252f638fd5e4cf84d7f6e066092654220dc0b63e5cf4e9723';source='https://downloads.php.net/~windows/releases/archives/php-8.5.12-1-nts-Win32-vs17-x64.zip.spdx.json';vendor='PHP Group'},
        @{id='extension8511';file='ServBay-php-8.5.11-windows-x64.zip';url='https://github.com/ServBay/PHP-Windows-Portable/releases/download/php-8.5.11-x64-nts/php-8.5.11-windows-x64.zip';sha256='8011ec199c0b714e2c9a15b0355abe0e93117e1e09f46f486cff4463a6cb9348';source='https://api.github.com/repos/ServBay/PHP-Windows-Portable/releases/tags/php-8.5.11-x64-nts';vendor='ServBay'},
        @{id='extension8512';file='ServBay-php-8.5.12-windows-x64.zip';url='https://github.com/ServBay/PHP-Windows-Portable/releases/download/php-8.5.12-x64-nts/php-8.5.12-windows-x64.zip';sha256='b2e21593123c4e53930b8afb4a6686e4e9f9d8481caccff43c545679992e58c9';source='https://api.github.com/repos/ServBay/PHP-Windows-Portable/releases/tags/php-8.5.12-x64-nts';vendor='ServBay'}
    )
    foreach ($pin in $pins) {
        $metadata = Join-Path $run ($pin.id+'-publisher.json')
        Fetch $pin.source $metadata
        $published = [IO.File]::ReadAllText($metadata) | ConvertFrom-Json
        if ($pin.vendor -eq 'PHP Group') {
            $expected = @($published.packages | Where-Object name -eq 'php')[0].checksums | Where-Object algorithm -eq 'SHA256' | Select-Object -ExpandProperty checksumValue
        } else {
            $asset = @($published.assets | Where-Object browser_download_url -eq $pin.url)
            if ($asset.Count -ne 1) { throw 'Publisher release does not identify exactly one pinned asset' }
            $expected = $asset[0].digest -replace '^sha256:',''
        }
        if ($expected -ne $pin.sha256) { throw "Publisher digest changed: $($pin.id)" }
        $archive = Join-Path $ArtifactRoot $pin.file
        if (-not (Test-Path -LiteralPath $archive)) { Fetch $pin.url $archive }
        $digest = Assert-Digest $archive $pin.sha256
        $destination = Join-Path $run $pin.id
        Expand-Verified $archive $destination
        $pin.directory = $destination
        Record $pin.id 'passed' @{vendor=$pin.vendor;url=$pin.url;sha256=$digest;digest_source=$pin.source;directory=$destination}
    }
    $php11 = Join-Path $pins[0].directory 'php.exe'
    $php12 = Join-Path $pins[1].directory 'php.exe'
    foreach ($runtime in @(@{id='8511';path=$php11},@{id='8512';path=$php12})) {
        $probe = Probe ('identity'+$runtime.id) $runtime.path @('-n','-r','phpinfo(INFO_GENERAL);')
        Require-Success $probe
        if ($probe.stdout -notmatch 'PHP Extension Build => API20250925,NTS,VS17' -or $probe.stdout -notmatch 'Zend Extension => 420250925' -or $probe.stdout -notmatch 'Architecture => x64') { throw 'Unexpected PHP identity' }
        Record ('identity'+$runtime.id) 'passed' $probe
    }
    $fixture = Join-Path $run 'extension.php'
    Write-Text $fixture '<?php echo json_encode(["php"=>PHP_VERSION,"sapi"=>PHP_SAPI,"memcached"=>extension_loaded("memcached"),"version"=>phpversion("memcached")], JSON_THROW_ON_ERROR);'
    $profiles = @{}
    foreach ($pair in @(@{id='8511';php=$php11;bundle=$pins[2].directory},@{id='8512';php=$php12;bundle=$pins[3].directory},@{id='forward8511to8512';php=$php12;bundle=$pins[2].directory})) {
        foreach ($native in @('libmemcached.dll','libhashkit.dll')) { if (-not (Test-Path -LiteralPath (Join-Path $pair.bundle $native))) { throw "Missing native dependency $native" } }
        $ini = Join-Path $run ($pair.id+'.ini')
        $library = (Join-Path $pair.bundle 'ext/php_memcached.dll').Replace('\','/')
        Write-Text $ini ("display_startup_errors=1`ndisplay_errors=stderr`nlog_errors=0`nextension=`"$library`"`n")
        $profiles[$pair.id] = $ini
        $probe = Probe ('cli'+$pair.id) $pair.php @('-c',$ini,$fixture) $pair.bundle
        Require-Success $probe
        $loaded = $probe.stdout | ConvertFrom-Json
        if (-not $loaded.memcached -or $loaded.version -ne '3.4.0') { throw 'Incorrect extension load' }
        Record ('cli'+$pair.id) 'passed' $probe
        $cgi = Join-Path ([IO.Path]::GetDirectoryName($pair.php)) 'php-cgi.exe'
        $probe = Probe ('cgi'+$pair.id) $cgi @('-c',$ini) $pair.bundle @{REDIRECT_STATUS='1';REQUEST_METHOD='GET';SCRIPT_FILENAME=$fixture;GATEWAY_INTERFACE='CGI/1.1'}
        Require-Success $probe
        if ($probe.stdout -notmatch '"sapi":"cgi-fcgi"' -or $probe.stdout -notmatch '"memcached":true') { throw 'CGI failed to load memcached' }
        Record ('cgi'+$pair.id) 'passed' $probe
    }
    $probe = Probe 'reverse8512to8511' $php11 @('-c',$profiles['8512'],$fixture) $pins[3].directory
    if (($probe.stdout+$probe.stderr) -notmatch 'Unable to load dynamic library' -or $probe.stdout -notmatch '"memcached":false') { throw 'Expected reverse-patch incompatibility was not detected; review evidence' }
    Record 'reverse-patch-incompatible' 'passed' $probe
    $probe = Probe 'native-build-capabilities' $php12 @('-c',$profiles['8512'],'--ri','memcached') $pins[3].directory
    Require-Success $probe
    Record 'native-build-capabilities' 'passed' $probe
    if ($Port -ne 0) {
        $client = Join-Path $run 'cache.php'
        Write-Text $client @'
<?php
if (!class_exists('Memcached')) { fwrite(STDERR, "missing-extension\n"); exit(42); }
$m = new Memcached();
$m->setOption(Memcached::OPT_CONNECT_TIMEOUT, 1000);
$m->setOption(Memcached::OPT_POLL_TIMEOUT, 1000);
$m->setOption(Memcached::OPT_RECV_TIMEOUT, 500000);
$m->setOption(Memcached::OPT_SEND_TIMEOUT, 500000);
$m->addServer('127.0.0.1', (int)getenv('STACKYARD_PROBE_PORT'));
$key = 'stackyard:php-probe:'.bin2hex(random_bytes(16));
$reserved = false;
try {
    if (!$m->add($key, 'reserved', 30)) { throw new RuntimeException('endpoint-unavailable-or-key-not-reserved:'.$m->getResultCode()); }
    $reserved = true;
    if (!$m->set($key, 'native-php-memcached', 30)) { throw new RuntimeException('set-failed'); }
    if ($m->get($key) !== 'native-php-memcached') { throw new RuntimeException('get-mismatch'); }
    if (!$m->delete($key)) { throw new RuntimeException('delete-failed'); }
    $reserved = false;
    if ($m->get($key) !== false || $m->getResultCode() !== Memcached::RES_NOTFOUND) { throw new RuntimeException('cleanup-failed'); }
    echo json_encode(['php'=>PHP_VERSION,'extension'=>phpversion('memcached'),'set_get'=>true,'cleanup'=>true], JSON_THROW_ON_ERROR);
} finally { if ($reserved) { $m->delete($key); } }
'@
        $probe = Probe 'native-client-roundtrip' $php12 @('-c',$profiles['8512'],$client) $pins[3].directory @{STACKYARD_PROBE_PORT=[string]$Port}
        Require-Success $probe
        Record 'native-client-roundtrip' 'passed' $probe
        $probe = Probe 'missing-extension' $php12 @('-n',$client) '' @{STACKYARD_PROBE_PORT=[string]$Port}
        if ($probe.exit_code -ne 42 -or $probe.stderr -ne 'missing-extension') { throw 'Missing-extension diagnostic did not differ from endpoint failure' }
        Record 'missing-extension' 'passed' $probe
        $closed = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback,0)
        $closed.ExclusiveAddressUse = $true
        $closed.Start()
        $closedPort = $closed.LocalEndpoint.Port
        $closed.Stop()
        $probe = Probe 'unreachable-endpoint' $php12 @('-c',$profiles['8512'],$client) $pins[3].directory @{STACKYARD_PROBE_PORT=[string]$closedPort}
        if ($probe.exit_code -eq 0 -or $probe.exit_code -eq 42 -or ($probe.stdout+$probe.stderr) -notmatch 'endpoint-unavailable-or-key-not-reserved') { throw 'Unreachable endpoint did not produce a distinct bounded failure' }
        Record 'unreachable-endpoint' 'passed' $probe
    } else { Record 'native-client-roundtrip' 'pending' 'Pass an explicitly selected high loopback Port to exercise a real endpoint; no server was implicitly adopted.' }
} catch { Record 'verification' 'failed' $_.Exception.Message }
finally {
    $failed = @($results | Where-Object status -eq 'failed').Count
    Write-Text (Join-Path $ArtifactRoot 'result.json') ([ordered]@{task='S8A-NATIVE-CACHE';os=@{caption=$os.Caption;version=$os.Version;build=$os.BuildNumber};arch=$env:PROCESSOR_ARCHITECTURE;checked_on=(Get-Date -Format 'o');failed_count=$failed;results=$results.ToArray()} | ConvertTo-Json -Depth 10)
}
exit ([int]($failed -gt 0))
