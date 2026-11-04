<#
.SYNOPSIS
  Run opt-in real Windows engine-managed Memcached/PHP acceptance.
.DESCRIPTION
  Help (default) does not write, download or execute. Verify checks five already
  downloaded publisher-pinned archives, then invokes the repository Rust native
  integration test. The engine owns its exclusive TCP listener and native
  private-socket process; this wrapper does not implement another provider.
#>
[CmdletBinding()]
param(
    [ValidateSet('Help','Verify')][string]$Action='Help',
    [string]$ArtifactRoot='',
    [string]$Php8511Archive='',
    [string]$Php8512Archive='',
    [string]$Extension8511Archive='',
    [string]$Extension8512Archive='',
    [string]$ServerArchive=''
)
if($Action -eq 'Help') {
    Write-Output @'
S8A-NATIVE-CACHE engine-managed native acceptance
Help (default): no writes, downloads or process launches.
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/memcached-native.ps1 -Action Verify
Verify requires the publisher-checked PHP8.5.11/8.5.12, ServBay extension8.5.11/
8.5.12 and jefyt server ZIPs cached by the Stage8A probes. Optional archive
parameters select explicit local files; every digest must match the fixed pin.
Runs: cargo test -p stackyard-engine --test php_memcached --offline -- --nocapture
The Rust fixture owns all disposable processes, sockets, project state and keys.
No OS service, global PATH/configuration, existing project or external cache is
adopted. The separate endpoint in the test is another explicitly owned process.
Result: .artifacts/S8A-NATIVE-CACHE/native-provider/result.json by default.
'@
    exit 0
}
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$allowed=[IO.Path]::GetFullPath((Join-Path $repository '.artifacts/S8A-NATIVE-CACHE'))
if(-not $ArtifactRoot){$ArtifactRoot=Join-Path $allowed 'native-provider'}
$ArtifactRoot=[IO.Path]::GetFullPath($ArtifactRoot)
if($ArtifactRoot -ne $allowed -and -not $ArtifactRoot.StartsWith($allowed+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'ArtifactRoot escapes assigned task directory'}
$current=$ArtifactRoot
while($current -and $current.Length -ge $allowed.Length){
    if((Test-Path -LiteralPath $current) -and ((Get-Item -LiteralPath $current).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Reparse point in artifact path'}
    $current=[IO.Path]::GetDirectoryName($current)
}
New-Item -ItemType Directory -Force -Path $ArtifactRoot|Out-Null
$utf8=New-Object Text.UTF8Encoding $false
$results=New-Object 'Collections.Generic.List[object]'
$os=Get-CimInstance Win32_OperatingSystem
function Record([string]$Name,[string]$Status,$Details){
    $results.Add([ordered]@{name=$Name;status=$Status;details=$Details})
    Write-Output "$Status $Name"
}
if(-not $Php8511Archive){$Php8511Archive=Join-Path $allowed 'php-8.5.11-nts-Win32-vs17-x64.zip'}
if(-not $Php8512Archive){$Php8512Archive=Join-Path $repository '.artifacts/S8A-ARTIFACTS/php-8.5.12-1-nts-Win32-vs17-x64.zip'}
if(-not $Extension8511Archive){$Extension8511Archive=Join-Path $allowed 'ServBay-php-8.5.11-windows-x64.zip'}
if(-not $Extension8512Archive){$Extension8512Archive=Join-Path $allowed 'ServBay-php-8.5.12-windows-x64.zip'}
if(-not $ServerArchive){$ServerArchive=Join-Path $allowed 'memcached-1.6.8-win64-mingw.zip'}
try{
    if($env:PROCESSOR_ARCHITECTURE -ne 'AMD64'){throw 'This probe requires Windows x64'}
    $inputs=@(
        @{name='PHP8.5.11';env='STACKYARD_TEST_PHP8511_ARCHIVE';path=$Php8511Archive;sha256='0ea96e0d2b9b737a6036f05cf4e95c49313faa6d0f27bd97edb2742503f0c043'},
        @{name='PHP8.5.12';env='STACKYARD_TEST_PHP8512_ARCHIVE';path=$Php8512Archive;sha256='2a36a2edcb7a5b9252f638fd5e4cf84d7f6e066092654220dc0b63e5cf4e9723'},
        @{name='ServBay8511';env='STACKYARD_TEST_MEMCACHED_EXTENSION8511_ARCHIVE';path=$Extension8511Archive;sha256='8011ec199c0b714e2c9a15b0355abe0e93117e1e09f46f486cff4463a6cb9348'},
        @{name='ServBay8512';env='STACKYARD_TEST_MEMCACHED_EXTENSION8512_ARCHIVE';path=$Extension8512Archive;sha256='b2e21593123c4e53930b8afb4a6686e4e9f9d8481caccff43c545679992e58c9'},
        @{name='Memcached1.6.8';env='STACKYARD_TEST_MEMCACHED_SERVER_ARCHIVE';path=$ServerArchive;sha256='48ec62cef718f0d73698414b783c0e4a69821013553ca00afe0eed324eb5994b'}
    )
    $start=New-Object Diagnostics.ProcessStartInfo
    $start.FileName=(Get-Command cargo -CommandType Application -ErrorAction Stop).Source
    $start.Arguments='test -p stackyard-engine --test php_memcached --offline -- --nocapture'
    $start.WorkingDirectory=$repository
    $start.UseShellExecute=$false
    $start.CreateNoWindow=$true
    $start.RedirectStandardOutput=$true
    $start.RedirectStandardError=$true
    foreach($artifact in $inputs){
        $path=[IO.Path]::GetFullPath($artifact.path)
        if(-not (Test-Path -LiteralPath $path -PathType Leaf)){throw "Missing $($artifact.name) archive; run the artifact probe first or select its explicit path"}
        $digest=(Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        if($digest -ne $artifact.sha256){throw "Publisher digest mismatch for $($artifact.name); no acceptance code executed"}
        $start.EnvironmentVariables[$artifact.env]=$path
        Record $artifact.name 'passed' @{path=$path;sha256=$digest}
    }
    $process=New-Object Diagnostics.Process
    $process.StartInfo=$start
    try{
        if(-not $process.Start()){throw 'Cannot start native acceptance'}
        $out=$process.StandardOutput.ReadToEndAsync()
        $err=$process.StandardError.ReadToEndAsync()
        if(-not $process.WaitForExit(180000)){
            # The PID is the Cargo child created immediately above. Terminate
            # only its descendant tree; never enumerate unrelated processes.
            & "$env:SystemRoot/System32/taskkill.exe" /PID ([string]$process.Id) /T /F | Out-Null
            $process.WaitForExit(5000)|Out-Null
            throw 'Native acceptance exceeded 180 seconds; owned test tree terminated'
        }
        if(-not $out.Wait(3000) -or -not $err.Wait(3000)){throw 'Acceptance output capture did not close'}
        $stdout=$out.GetAwaiter().GetResult()
        $stderr=$err.GetAwaiter().GetResult()
        [IO.File]::WriteAllText((Join-Path $ArtifactRoot 'acceptance.stdout.log'),$stdout,$utf8)
        [IO.File]::WriteAllText((Join-Path $ArtifactRoot 'acceptance.stderr.log'),$stderr,$utf8)
        if($process.ExitCode -ne 0 -or $stderr -match 'SKIPPED native PHP memcached' -or $stderr -notmatch 'Native acceptance passed:'){
            throw "Native acceptance failed or was skipped; inspect task-local logs (exit $($process.ExitCode))"
        }
        Record 'native-engine-acceptance' 'passed' @{executable=$start.FileName;arguments=$start.Arguments;exit_code=$process.ExitCode;stdout=$stdout.Trim();stderr=$stderr.Trim()}
    }finally{$process.Dispose()}
}catch{Record 'verification' 'failed' $_.Exception.Message}
finally{
    $failed=@($results|Where-Object status -eq 'failed').Count
    $result=[ordered]@{task='S8A-NATIVE-CACHE';os=@{caption=$os.Caption;version=$os.Version;build=$os.BuildNumber};arch=$env:PROCESSOR_ARCHITECTURE;checked_on=(Get-Date -Format 'o');failed_count=$failed;results=$results.ToArray()}
    [IO.File]::WriteAllText((Join-Path $ArtifactRoot 'result.json'),($result|ConvertTo-Json -Depth 10),$utf8)
}
exit ([int]($failed -gt 0))
