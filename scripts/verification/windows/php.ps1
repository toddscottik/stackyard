<#
.SYNOPSIS
  Disposable Stage 8A PHP/Composer artifact and bundled-extension probe.
.DESCRIPTION
  Help is the default and performs no writes or downloads. Verify pins official
  PHP 8.5.12-1 NTS VS17 x64 and Composer 2.11.1, checks SHA256 before use,
  and keeps every generated file under .artifacts/S8A-ARTIFACTS.
  External memcached and macOS execution remain explicitly unverified.
#>
[CmdletBinding()]
param(
    [ValidateSet('Help', 'Verify')][string]$Action = 'Help',
    [string]$ArtifactRoot = '',
    [string]$ResultJson = ''
)

if ($Action -eq 'Help') {
    Write-Output @'
S8A-ARTIFACTS Windows PHP probe (Windows PowerShell 5.1)
Default Help performs no writes or downloads.
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/php.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/php.ps1 -Action Verify
Optional ArtifactRoot must be .artifacts/S8A-ARTIFACTS or a child; ResultJson
must be inside ArtifactRoot. No global PATH, php.ini, services or tools change.
'@
    exit 0
}

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$allowedRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../.artifacts/S8A-ARTIFACTS'))
if (-not $ArtifactRoot) { $ArtifactRoot = $allowedRoot }
$ArtifactRoot = [IO.Path]::GetFullPath($ArtifactRoot)
if (-not $ResultJson) { $ResultJson = Join-Path $ArtifactRoot 'result.json' }
$ResultJson = [IO.Path]::GetFullPath($ResultJson)
$utf8 = New-Object Text.UTF8Encoding $false
$results = New-Object 'Collections.Generic.List[object]'

function Assert-Contained([string]$Root, [string]$Candidate) {
    $prefix = $Root.TrimEnd('\') + '\'
    if ($Candidate -ne $Root -and -not $Candidate.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Path escapes authorized artifact root: $Candidate"
    }
    $current = $Candidate
    while ($current -and $current.Length -ge $Root.Length) {
        if ((Test-Path -LiteralPath $current) -and ((Get-Item -LiteralPath $current).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Reparse points are not allowed in artifact paths: $current"
        }
        $current = [IO.Path]::GetDirectoryName($current)
    }
}
Assert-Contained $allowedRoot $ArtifactRoot
Assert-Contained $ArtifactRoot $ResultJson
New-Item -ItemType Directory -Force -Path $ArtifactRoot | Out-Null
New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($ResultJson)) | Out-Null

function Write-Text([string]$Path, [string]$Value) {
    Assert-Contained $ArtifactRoot ([IO.Path]::GetFullPath($Path))
    [IO.File]::WriteAllText($Path, $Value, $utf8)
}
function Record([string]$Name, [string]$Status, $Details) {
    $results.Add([ordered]@{ name = $Name; status = $Status; details = $Details })
    Write-Output "$Status $Name"
}
function Assert-Digest([string]$Path, [string]$Expected) {
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($actual -ne $Expected) { throw "SHA256 mismatch for $Path; expected $Expected, observed $actual" }
    return $actual
}
function Download([string]$Url, [string]$Path) {
    Assert-Contained $ArtifactRoot ([IO.Path]::GetFullPath($Path))
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $Path -TimeoutSec 120
}
function Assert-SafeZip([string]$Path, [string]$Destination) {
    $zip = [IO.Compression.ZipFile]::OpenRead($Path)
    try {
        foreach ($entry in $zip.Entries) {
            if ($entry.FullName -match '(^[/\\]|:|(^|[/\\])\.\.([/\\]|$))') { throw "Unsafe archive entry: $($entry.FullName)" }
            $target = [IO.Path]::GetFullPath((Join-Path $Destination $entry.FullName))
            Assert-Contained $Destination $target
            if (($entry.ExternalAttributes -band 0xF0000000) -eq 0xA0000000) { throw 'Archive symlink rejected' }
        }
    } finally { $zip.Dispose() }
}
# Quote each argument using the Windows CRT argv rules; never launch a shell.
function Quote-Argument([string]$Value) {
    if ($Value -and $Value -notmatch '[\s"]') { return $Value }
    return '"' + ([regex]::Replace([regex]::Replace($Value, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1')) + '"'
}
function Invoke-Probe([string]$Name, [string]$Executable, [string[]]$Arguments, [hashtable]$ExtraEnvironment = @{}) {
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $Executable
    $start.Arguments = (($Arguments | ForEach-Object { Quote-Argument $_ }) -join ' ')
    $start.WorkingDirectory = $ArtifactRoot
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.EnvironmentVariables['PHP_INI_SCAN_DIR'] = (Join-Path $ArtifactRoot 'empty scan')
    $start.EnvironmentVariables['COMPOSER_HOME'] = (Join-Path $ArtifactRoot 'composer home')
    $start.EnvironmentVariables['COMPOSER_CACHE_DIR'] = (Join-Path $ArtifactRoot 'composer cache')
    $start.EnvironmentVariables['COMPOSER_DISABLE_NETWORK'] = '1'
    $start.EnvironmentVariables.Remove('COMPOSER')
    $start.EnvironmentVariables.Remove('PHPRC')
    foreach ($key in $ExtraEnvironment.Keys) { $start.EnvironmentVariables[$key] = $ExtraEnvironment[$key] }
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw "Cannot start $Name" }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) { $process.Kill(); $process.WaitForExit(); throw "$Name exceeded 30 seconds" }
        $out = $stdout.GetAwaiter().GetResult()
        $err = $stderr.GetAwaiter().GetResult()
        Write-Text (Join-Path $ArtifactRoot "$Name.stdout.log") $out
        Write-Text (Join-Path $ArtifactRoot "$Name.stderr.log") $err
        return [ordered]@{ executable = $Executable; arguments = $Arguments; exit_code = $process.ExitCode; stdout = $out.Trim(); stderr = $err.Trim() }
    } finally { $process.Dispose() }
}
function Require-Success($Probe) {
    if ($Probe.exit_code -ne 0 -or $Probe.stderr) { throw ($Probe | ConvertTo-Json -Depth 5) }
}

$os = Get-CimInstance Win32_OperatingSystem
try {
    if (-not [Environment]::Is64BitOperatingSystem -or -not [Environment]::Is64BitProcess -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'This probe requires Windows x64 and 64-bit PowerShell' }
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $phpUrl = 'https://downloads.php.net/~windows/releases/archives/php-8.5.12-1-nts-Win32-vs17-x64.zip'
    $phpHash = '2a36a2edcb7a5b9252f638fd5e4cf84d7f6e066092654220dc0b63e5cf4e9723'
    $composerUrl = 'https://getcomposer.org/download/2.11.1/composer.phar'
    $composerHash = '6f1b891e1d4927d62740276c540fb490e47689e19e102c49e8da849794882f8c'
    $phpPage = Join-Path $ArtifactRoot 'php-downloads.html'
    Download 'https://www.php.net/downloads.php?os=windows' $phpPage
    if (-not ([IO.File]::ReadAllText($phpPage).Contains($phpHash))) { throw 'Pinned PHP checksum no longer appears on publisher downloads page; review provenance' }
    $sidecar = Join-Path $ArtifactRoot 'composer.phar.sha256sum'
    Download ($composerUrl + '.sha256sum') $sidecar
    if (([IO.File]::ReadAllText($sidecar) -split '\s+')[0] -ne $composerHash) { throw 'Composer publisher checksum changed' }
    $archive = Join-Path $ArtifactRoot 'php-8.5.12-1-nts-Win32-vs17-x64.zip'
    $composer = Join-Path $ArtifactRoot 'composer.phar'
    foreach ($artifact in @(@{ url = $phpUrl; path = $archive; digest = $phpHash }, @{ url = $composerUrl; path = $composer; digest = $composerHash })) {
        if (-not (Test-Path -LiteralPath $artifact.path)) { Download $artifact.url $artifact.path }
        $digest = Assert-Digest $artifact.path $artifact.digest
        Record ([IO.Path]::GetFileName($artifact.path)) 'passed' @{ url = $artifact.url; sha256 = $digest }
    }
    $runtime = Join-Path $ArtifactRoot 'php runtime'
    Assert-SafeZip $archive $runtime
    Expand-Archive -LiteralPath $archive -DestinationPath $runtime -Force
    $php = Join-Path $runtime 'php.exe'
    $cgi = Join-Path $runtime 'php-cgi.exe'
    New-Item -ItemType Directory -Force -Path (Join-Path $ArtifactRoot 'empty scan') | Out-Null
    foreach ($item in @(@{ name = 'php-cli-version'; exe = $php }, @{ name = 'php-cgi-version'; exe = $cgi })) {
        $probe = Invoke-Probe $item.name $item.exe @('-n', '-v')
        Require-Success $probe
        if ($probe.stdout -notmatch 'PHP 8\.5\.12' -or $probe.stdout -notmatch 'NTS Visual C\+\+ 2022 x64') { throw 'Unexpected PHP build identity' }
        Record $item.name 'passed' $probe
    }
    # INFO_GENERAL records ABI/build identity without phpinfo's environment dump.
    $probe = Invoke-Probe 'php-build-info' $php @('-n', '-r', 'phpinfo(INFO_GENERAL);')
    Require-Success $probe
    $identity = ($probe.stdout -split '\r?\n' | Where-Object { $_ -match '^(PHP API|PHP Extension|Zend Extension|Compiler|Architecture|Debug Build|Thread Safety) =>' })
    Record 'php-build-identity' 'passed' $identity
    $profile = Join-Path $ArtifactRoot 'bundled.ini'
    Write-Text $profile ("extension_dir=`"$($runtime.Replace('\','/'))/ext`"`nextension=mbstring`nextension=openssl`nextension=curl`nextension=pdo_sqlite`n")
    $fixture = Join-Path $ArtifactRoot 'extensions.php'
    Write-Text $fixture '<?php echo json_encode(["version"=>PHP_VERSION,"sapi"=>PHP_SAPI,"mbstring"=>extension_loaded("mbstring"),"openssl"=>extension_loaded("openssl"),"curl"=>extension_loaded("curl"),"pdo_sqlite"=>extension_loaded("pdo_sqlite")], JSON_THROW_ON_ERROR);'
    $base = Invoke-Probe 'base-profile' $php @('-n', $fixture)
    Require-Success $base
    if (($base.stdout | ConvertFrom-Json).mbstring) { throw 'Base profile unexpectedly loads mbstring' }
    Record 'base-profile' 'passed' $base
    $probe = Invoke-Probe 'bundled-profile-cli' $php @('-c', $profile, $fixture)
    Require-Success $probe
    $loaded = $probe.stdout | ConvertFrom-Json
    foreach ($extension in @('mbstring','openssl','curl','pdo_sqlite')) { if (-not $loaded.$extension) { throw "Missing bundled $extension" } }
    Record 'bundled-profile-cli' 'passed' $probe
    $probe = Invoke-Probe 'bundled-profile-cgi' $cgi @('-c', $profile) @{ REDIRECT_STATUS = '1'; REQUEST_METHOD = 'GET'; SCRIPT_FILENAME = $fixture; GATEWAY_INTERFACE = 'CGI/1.1' }
    Require-Success $probe
    if ($probe.stdout -notmatch '"sapi":"cgi-fcgi"' -or $probe.stdout -notmatch '"mbstring":true') { throw 'CGI did not load intended profile' }
    Record 'bundled-profile-cgi' 'passed' $probe
    Write-Text (Join-Path $ArtifactRoot 'composer.json') '{"name":"stackyard/php-artifact-probe","version":"1.0.0","repositories":{"packagist.org":false},"require":{"php":"~8.5.0","ext-json":"*","ext-mbstring":"*","ext-openssl":"*"}}'
    foreach ($operation in @(@{ name = 'composer-version'; args = @('--version') }, @{ name = 'composer-install'; args = @('install','--no-interaction','--no-plugins','--no-scripts','--no-progress') }, @{ name = 'composer-platform'; args = @('check-platform-reqs','--no-interaction','--no-plugins','--no-scripts') })) {
        $probe = Invoke-Probe $operation.name $php (@('-c', $profile, $composer) + $operation.args)
        if ($probe.exit_code -ne 0) { throw ($probe | ConvertTo-Json -Depth 5) }
        Record $operation.name 'passed' $probe
    }
    $limited = Join-Path $ArtifactRoot 'without-mbstring.ini'
    Write-Text $limited ("extension_dir=`"$($runtime.Replace('\','/'))/ext`"`nextension=openssl`nextension=curl`n")
    $probe = Invoke-Probe 'composer-missing-extension' $php @('-c', $limited, $composer, 'check-platform-reqs','--no-interaction','--no-plugins','--no-scripts')
    if ($probe.exit_code -eq 0 -or ($probe.stdout + $probe.stderr) -notmatch 'ext-mbstring') { throw 'Composer failed to detect missing extension' }
    Record 'composer-missing-extension' 'passed' $probe
    $invalid = Join-Path $ArtifactRoot 'invalid.ini'
    Write-Text $invalid 'extension=stackyard_missing_extension.dll'
    $probe = Invoke-Probe 'invalid-profile' $php @('-c', $invalid, $fixture)
    if (($probe.stdout + $probe.stderr) -notmatch 'Unable to load dynamic library') { throw 'Missing extension load warning not detected' }
    Record 'invalid-profile-diagnostic' 'passed' $probe
    $corrupt = Join-Path $ArtifactRoot 'corrupt-artifact.txt'
    Write-Text $corrupt 'intentionally corrupt artifact'
    $rejected = $false
    try { $null = Assert-Digest $corrupt $phpHash } catch { $rejected = $true }
    if (-not $rejected) { throw 'Corrupt digest accepted' }
    Record 'digest-mismatch-rejected' 'passed' 'Corrupt fixture rejected before extraction/execution'
    foreach ($entryName in @('../escape.txt','/absolute.txt','C:/drive.txt')) {
        $unsafe = Join-Path $ArtifactRoot 'unsafe.zip'
        $stream = [IO.File]::Open($unsafe, [IO.FileMode]::Create)
        $zip = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create)
        try { $null = $zip.CreateEntry($entryName) } finally { $zip.Dispose(); $stream.Dispose() }
        $rejected = $false
        try { Assert-SafeZip $unsafe (Join-Path $ArtifactRoot 'unsafe extract') } catch { $rejected = $true }
        if (-not $rejected) { throw "Unsafe zip entry accepted: $entryName" }
    }
    Record 'unsafe-archive-rejected' 'passed' 'Traversal, absolute and drive-qualified zip entries rejected'
    Record 'windows-memcached-extension' 'blocked' 'Official matching 3.4.0 PHP8.5 NTS VS17 x64 zip listed; publisher SHA256 not found. No download, load or set/get claimed.'
    Record 'macos-14.5-arm64' 'pending' 'This host is Windows. Current Homebrew PHP8.5 bottles do not list arm64_sonoma. No target launch or standalone relocatable package approved.'
} catch {
    Record 'verification' 'failed' $_.Exception.Message
} finally {
    $failed = @($results | Where-Object { $_.status -eq 'failed' }).Count
    Write-Text $ResultJson ([ordered]@{ task = 'S8A-ARTIFACTS'; checked_on = (Get-Date -Format 'o'); os = @{ caption = $os.Caption; version = $os.Version; build = $os.BuildNumber }; arch = $env:PROCESSOR_ARCHITECTURE; failed_count = $failed; results = $results.ToArray() } | ConvertTo-Json -Depth 10)
}
exit ([int]($failed -gt 0))
