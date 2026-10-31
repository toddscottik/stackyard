<#
.SYNOPSIS
  Stage 0 disposable Windows runtime probe for Node, Python, uv, and pnpm.

.DESCRIPTION
  Proves managed downloads, publisher digest checks, safe extraction, a path
  that contains a space, relocation, local Node/Python reuse, and pinned
  pnpm/pip smoke checks. The default action prints this help and does not
  download or write anything. Pass -Action Verify to run the probe.

  Downloads, extracts, virtual environments, and logs stay under the artifact
  root. The script does not change the user PATH, enable Corepack, install
  global packages, or modify C:\Program Files\nodejs or C:\Python\Python312.

.PARAMETER Action
  Help or Verify. Help is the default.

.PARAMETER ArtifactRoot
  Disposable directory. Must be D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES
  or a directory inside it.

.PARAMETER ResultJson
  Result JSON path. Must be inside ArtifactRoot. Defaults to result.json there.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verification\windows\runtimes.ps1

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verification\windows\runtimes.ps1 -Action Verify

.NOTES
  Windows PowerShell 5.1. Node 24.22.2, uv 0.13.6, and CPython 3.12.16+20261026
  are pinned candidate URLs. pnpm v12.16.1 is the official GitHub release read
  for this probe. Expected digests are re-fetched from the publisher at run time.
#>
[CmdletBinding()]
param(
    [ValidateSet('Help', 'Verify')]
    [string]$Action = 'Help',
    [string]$ArtifactRoot = 'D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES',
    [string]$ResultJson = ''
)

function Show-Usage {
    Write-Output @'
S0-WIN-RUNTIMES Windows runtime verification (Windows PowerShell 5.1)

Default action Help does not download files or create directories.

  powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verification\windows\runtimes.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verification\windows\runtimes.ps1 -Action Help
  powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verification\windows\runtimes.ps1 -Action Verify
  powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verification\windows\runtimes.ps1 -Action Verify -ArtifactRoot D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES -ResultJson D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES\result.json

Verify re-fetches publisher checksums, compares them with docs\verification-evidence.json,
downloads payloads only after a publisher digest is known, and refuses to extract
or execute when the digest does not match. Synthetic zip entries that contain
.. or an absolute path are rejected before extraction.
'@
}

if ($Action -eq 'Help') {
    Show-Usage
    exit 0
}

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$script:AllowedRoot = 'D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES'
$script:SystemNode = 'C:\Program Files\nodejs\node.exe'
$script:SystemPython = 'C:\Python\Python312\python.exe'
$script:Utf8 = New-Object System.Text.UTF8Encoding $false
$script:Results = New-Object System.Collections.Generic.List[object]
$script:CheckedOn = Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz'
$script:CleanupSummary = 'artifact root retained for coordinator review; system Node and Python installs were not modified'
$script:OsText = ''
$script:Evidence = $null
$script:OriginalPath = [Environment]::GetEnvironmentVariable('Path', 'Process')
$script:Fatal = ''

function Test-SameOrUnder {
    param([string]$Root, [string]$Candidate)
    $sep = [System.IO.Path]::DirectorySeparatorChar
    $rootFull = [System.IO.Path]::GetFullPath($Root)
    $candFull = [System.IO.Path]::GetFullPath($Candidate)
    if (-not $rootFull.EndsWith([string]$sep)) {
        $rootFull = $rootFull + $sep
    }
    if ($candFull.StartsWith($rootFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $true
    }
    $candAsDir = $candFull
    if (-not $candAsDir.EndsWith([string]$sep)) {
        $candAsDir = $candAsDir + $sep
    }
    return $candAsDir.Equals($rootFull, [System.StringComparison]::OrdinalIgnoreCase)
}

function Ensure-Dir {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path | Out-Null
    }
}

function Format-Arg {
    param([string]$Value)
    if ($Value -notmatch '[\s"]') {
        return $Value
    }
    return '"' + ($Value.Replace('"', '\"')) + '"'
}

function Format-Args {
    param([string[]]$Values)
    $parts = @()
    foreach ($value in $Values) {
        $parts += (Format-Arg $value)
    }
    return ($parts -join ' ')
}

function Add-Row {
    param($Row)
    $script:Results.Add($Row) | Out-Null
    Write-Output ('{0} {1}' -f $Row.status, $Row.component)
}

function New-Check {
    param(
        [string]$Component,
        [string]$Version = '',
        [string]$Url = '',
        [string]$Sha256 = '',
        [string]$ExpectedSha256 = '',
        [string]$ExpectedSha256Source = '',
        [string]$EvidenceSha256 = '',
        [string]$EvidenceSha256Source = '',
        [string]$LaunchCommand = '',
        $ExitCode = $null,
        [string]$StdoutVersion = '',
        [string]$Status,
        [string]$Notes = '',
        [string]$Limitations = '',
        [string]$Cleanup = ''
    )
    if ([string]::IsNullOrEmpty($Cleanup)) {
        $Cleanup = $script:CleanupSummary
    }
    if ([string]::IsNullOrEmpty($Limitations)) {
        $Limitations = 'macOS was not tested'
    }
    return [pscustomobject]@{
        component               = $Component
        version                 = $Version
        url                     = $Url
        sha256                  = $Sha256
        expected_sha256         = $ExpectedSha256
        expected_sha256_source  = $ExpectedSha256Source
        evidence_sha256         = $EvidenceSha256
        evidence_sha256_source  = $EvidenceSha256Source
        os                      = $script:OsText
        arch                    = 'x64'
        launch_command          = $LaunchCommand
        exit_code               = $ExitCode
        stdout_version          = $StdoutVersion
        cleanup                 = $Cleanup
        limitations             = $Limitations
        status                  = $Status
        notes                   = $Notes
    }
}

function Write-Text {
    param([string]$Path, [string]$Text)
    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrEmpty($parent)) {
        Ensure-Dir $parent
    }
    [System.IO.File]::WriteAllText($Path, $Text, $script:Utf8)
}

function Add-CommandLog {
    param([string]$Line)
    if ([string]::IsNullOrEmpty($script:CommandLog)) {
        return
    }
    [System.IO.File]::AppendAllText($script:CommandLog, ($Line + [Environment]::NewLine), $script:Utf8)
}

function Get-FirstLine {
    param([string]$Text)
    if ([string]::IsNullOrEmpty($Text)) {
        return ''
    }
    $trimmed = $Text.Trim()
    $index = $trimmed.IndexOfAny([char[]]@("`r", "`n"))
    if ($index -lt 0) {
        return $trimmed
    }
    return $trimmed.Substring(0, $index).Trim()
}

function Install-NativeHelpers {
    if ('S0VerifyNative' -as [type]) {
        return
    }
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.IO;
using System.Text;
public static class S0ZipWriter {
    public static uint Crc32(byte[] data) {
        uint[] table = new uint[256];
        for (uint i = 0; i < 256; i++) {
            uint c = i;
            for (int k = 0; k < 8; k++) {
                if ((c & 1) != 0) c = 0xEDB88320u ^ (c >> 1);
                else c >>= 1;
            }
            table[i] = c;
        }
        uint crc = 0xFFFFFFFFu;
        for (int i = 0; i < data.Length; i++) {
            crc = table[(crc ^ data[i]) & 0xFF] ^ (crc >> 8);
        }
        return crc ^ 0xFFFFFFFFu;
    }
    public static void WriteStored(string path, string entryName, byte[] payload) {
        uint crc = Crc32(payload);
        byte[] name = Encoding.UTF8.GetBytes(entryName);
        using (FileStream fs = File.Create(path))
        using (BinaryWriter bw = new BinaryWriter(fs)) {
            bw.Write(0x04034b50);
            bw.Write((ushort)20);
            bw.Write((ushort)0);
            bw.Write((ushort)0);
            bw.Write((ushort)0);
            bw.Write((ushort)0);
            bw.Write(crc);
            bw.Write(payload.Length);
            bw.Write(payload.Length);
            bw.Write((ushort)name.Length);
            bw.Write((ushort)0);
            bw.Write(name);
            bw.Write(payload);
            bw.Flush();
            long cdOffset = bw.BaseStream.Position;
            bw.Write(0x02014b50);
            bw.Write((ushort)20);
            bw.Write((ushort)20);
            bw.Write((ushort)0);
            bw.Write((ushort)0);
            bw.Write((ushort)0);
            bw.Write((ushort)0);
            bw.Write(crc);
            bw.Write(payload.Length);
            bw.Write(payload.Length);
            bw.Write((ushort)name.Length);
            bw.Write((ushort)0);
            bw.Write((ushort)0);
            bw.Write((ushort)0);
            bw.Write((ushort)0);
            bw.Write((uint)0);
            bw.Write((uint)0);
            bw.Write(name);
            bw.Flush();
            long cdSize = bw.BaseStream.Position - cdOffset;
            bw.Write(0x06054b50);
            bw.Write((ushort)0);
            bw.Write((ushort)0);
            bw.Write((ushort)1);
            bw.Write((ushort)1);
            bw.Write((uint)cdSize);
            bw.Write((uint)cdOffset);
            bw.Write((ushort)0);
        }
    }
}
public sealed class ProcResult {
    public int ExitCode;
    public string Stdout;
    public string Stderr;
    public bool TimedOut;
    public int Pid;
}
public static class S0VerifyNative {
    public static ProcResult Run(string file, string arguments, string cwd, string[] envPairs, int timeoutMs) {
        ProcResult result = new ProcResult();
        result.Stdout = "";
        result.Stderr = "";
        result.ExitCode = -1;
        result.Pid = 0;
        if (!File.Exists(file)) {
            result.Stderr = "executable not found: " + file;
            result.ExitCode = 127;
            return result;
        }
        ProcessStartInfo psi = new ProcessStartInfo();
        psi.FileName = file;
        psi.Arguments = arguments ?? "";
        if (!string.IsNullOrEmpty(cwd)) psi.WorkingDirectory = cwd;
        psi.UseShellExecute = false;
        psi.RedirectStandardOutput = true;
        psi.RedirectStandardError = true;
        psi.CreateNoWindow = true;
        if (envPairs != null) {
            for (int i = 0; i < envPairs.Length; i++) {
                string pair = envPairs[i];
                if (string.IsNullOrEmpty(pair)) continue;
                int idx = pair.IndexOf('=');
                if (idx <= 0) continue;
                string key = pair.Substring(0, idx);
                string val = pair.Substring(idx + 1);
                psi.EnvironmentVariables[key] = val;
            }
        }
        Process p = new Process();
        p.StartInfo = psi;
        StringBuilder sbOut = new StringBuilder();
        StringBuilder sbErr = new StringBuilder();
        p.OutputDataReceived += delegate(object s, DataReceivedEventArgs e) {
            if (e.Data != null) { lock (sbOut) { sbOut.AppendLine(e.Data); } }
        };
        p.ErrorDataReceived += delegate(object s, DataReceivedEventArgs e) {
            if (e.Data != null) { lock (sbErr) { sbErr.AppendLine(e.Data); } }
        };
        p.Start();
        result.Pid = p.Id;
        p.BeginOutputReadLine();
        p.BeginErrorReadLine();
        bool exited = p.WaitForExit(timeoutMs);
        if (!exited) {
            result.TimedOut = true;
            try {
                ProcessStartInfo kill = new ProcessStartInfo();
                kill.FileName = Path.Combine(Environment.SystemDirectory, "taskkill.exe");
                kill.Arguments = "/PID " + p.Id.ToString() + " /T /F";
                kill.UseShellExecute = false;
                kill.CreateNoWindow = true;
                Process killer = Process.Start(kill);
                killer.WaitForExit(15000);
            } catch {}
            try { p.WaitForExit(15000); } catch {}
            result.ExitCode = -1;
        } else {
            p.WaitForExit();
            result.ExitCode = p.ExitCode;
        }
        result.Stdout = sbOut.ToString();
        result.Stderr = sbErr.ToString();
        p.Dispose();
        return result;
    }
}
'@
}

function Invoke-Exe {
    param(
        [string]$File,
        [string[]]$ArgumentList,
        [string]$WorkingDirectory = '',
        [string[]]$EnvPairs = @(),
        [int]$TimeoutSec = 60
    )
    $argString = ''
    if ($ArgumentList -and $ArgumentList.Count -gt 0) {
        $argString = Format-Args $ArgumentList
    }
    $cwd = $WorkingDirectory
    if ([string]::IsNullOrEmpty($cwd)) {
        $cwd = $script:Root
    }
    $command = (Format-Arg $File) + ' ' + $argString
    $command = $command.Trim()
    Add-CommandLog $command
    $raw = [S0VerifyNative]::Run($File, $argString, $cwd, $EnvPairs, ($TimeoutSec * 1000))
    $logName = 'cmd-' + $script:Results.Count.ToString()
    $logBody = "COMMAND: $command`r`nEXIT: $($raw.ExitCode)`r`nTIMED_OUT: $($raw.TimedOut)`r`n--- stdout ---`r`n$($raw.Stdout)`r`n--- stderr ---`r`n$($raw.Stderr)"
    $logPath = Join-Path $script:LogDir ($logName + '.log')
    Write-Text $logPath $logBody
    return [pscustomobject]@{
        Command  = $command
        ExitCode = [int]$raw.ExitCode
        Stdout   = [string]$raw.Stdout
        Stderr   = [string]$raw.Stderr
        TimedOut = [bool]$raw.TimedOut
        LogPath  = $logPath
    }
}

function Get-HttpResponse {
    param([string]$Url)
    $req = [System.Net.HttpWebRequest][System.Net.WebRequest]::Create($Url)
    $req.UserAgent = 'laragon-alternative-s0-win-runtimes'
    $req.Timeout = 600000
    $req.ReadWriteTimeout = 600000
    $req.AllowAutoRedirect = $true
    $req.AutomaticDecompression = ([System.Net.DecompressionMethods]::GZip -bor [System.Net.DecompressionMethods]::Deflate)
    try {
        return $req.GetResponse()
    } catch {
        $ex = $_.Exception
        $status = 0
        $web = $ex
        if ($web -isnot [System.Net.WebException] -and $ex.InnerException) {
            $web = $ex.InnerException
        }
        if ($web -is [System.Net.WebException] -and $web.Response) {
            $status = [int]$web.Response.StatusCode
            $web.Response.Close()
        }
        throw ("download failed HTTP {0} for {1}: {2}" -f $status, $Url, $ex.Message)
    }
}

function Get-HttpText {
    param([string]$Url)
    $resp = Get-HttpResponse $Url
    try {
        $reader = New-Object System.IO.StreamReader($resp.GetResponseStream())
        try {
            return $reader.ReadToEnd()
        } finally {
            $reader.Close()
        }
    } finally {
        $resp.Close()
    }
}

function Save-Http {
    param([string]$Url, [string]$Destination)
    $tmp = $Destination + '.partial'
    if (Test-Path -LiteralPath $tmp) {
        Remove-Item -LiteralPath $tmp -Force
    }
    Ensure-Dir (Split-Path -Parent $Destination)
    $resp = Get-HttpResponse $Url
    try {
        $fs = [System.IO.File]::Create($tmp)
        try {
            $resp.GetResponseStream().CopyTo($fs)
        } finally {
            $fs.Close()
        }
    } finally {
        $resp.Close()
    }
    if (Test-Path -LiteralPath $Destination) {
        Remove-Item -LiteralPath $Destination -Force
    }
    Move-Item -LiteralPath $tmp -Destination $Destination
}

function Get-FileSha256 {
    param([string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLower()
}

function Get-ManifestHash {
    param([string]$Text, [string]$FileName)
    $lines = $Text -split "`r?`n"
    foreach ($line in $lines) {
        $trim = $line.Trim()
        if ($trim -eq '' -or $trim.StartsWith('#')) {
            continue
        }
        if ($trim -match '^([a-fA-F0-9]{64})\s+\*?(.+)$') {
            $hash = $Matches[1].ToLower()
            $name = $Matches[2].Trim().Replace('\', '/')
            $base = $name
            $slash = $name.LastIndexOf('/')
            if ($slash -ge 0) {
                $base = $name.Substring($slash + 1)
            }
            if ($base -eq $FileName -or $name -eq $FileName) {
                return $hash
            }
        }
    }
    return ''
}

function Get-AssetDigest {
    param([string]$Json, [string]$AssetName)
    $marker = '"name":"' + $AssetName + '"'
    $start = $Json.IndexOf($marker)
    if ($start -lt 0) {
        return ''
    }
    $next = $Json.IndexOf('"name":"', $start + $marker.Length)
    if ($next -lt 0) {
        $next = $Json.Length
    }
    $window = $Json.Substring($start, $next - $start)
    $match = [regex]::Match($window, '"digest":"sha256:([a-f0-9]{64})"')
    if ($match.Success) {
        return $match.Groups[1].Value.ToLower()
    }
    return ''
}

function Get-JsonNames {
    param([string]$Json)
    $names = New-Object System.Collections.Generic.List[string]
    foreach ($match in [regex]::Matches($Json, '"name":"([^"]+)"')) {
        $names.Add($match.Groups[1].Value) | Out-Null
    }
    return $names
}

function Get-EvidenceObservation {
    param([string]$Component, [string]$Version)
    if ($null -eq $script:Evidence) {
        return $null
    }
    $items = @($script:Evidence.artifact_observations)
    foreach ($item in $items) {
        if ([string]$item.component -eq $Component -and [string]$item.target -eq 'windows-x64' -and [string]$item.version -eq $Version) {
            return $item
        }
    }
    return $null
}

function Get-EvidenceSource {
    param($Item)
    if ($null -eq $Item) {
        return ''
    }
    $names = @($Item.PSObject.Properties.Name)
    if ($names -contains 'source') {
        return [string]$Item.source
    }
    if ($names -contains 'url_from_release_api') {
        return [string]$Item.url_from_release_api
    }
    return ''
}

function Move-Rejected {
    param([string]$Path, [string]$Reason)
    $name = Split-Path -Leaf $Path
    $dest = Join-Path $script:RejectedDir ($name + '.rejected')
    if (Test-Path -LiteralPath $dest) {
        Remove-Item -LiteralPath $dest -Force
    }
    Move-Item -LiteralPath $Path -Destination $dest
    Write-Text ($dest + '.txt') $Reason
    return $dest
}

function Test-DigestGate {
    param([string]$File, [string]$Expected, [string]$InstallDir)
    $actual = Get-FileSha256 $File
    $expectedNorm = ''
    if ($Expected) {
        $expectedNorm = $Expected.ToLower()
    }
    $match = ($actual -eq $expectedNorm -and $expectedNorm -ne '')
    if (-not $match -and (Test-Path -LiteralPath $InstallDir)) {
        Remove-Item -LiteralPath $InstallDir -Recurse -Force
    }
    return [pscustomobject]@{
        Match    = $match
        Actual   = $actual
        Expected = $expectedNorm
    }
}

function Test-EntryNameSafe {
    param([string]$Name)
    if ([string]::IsNullOrEmpty($Name)) {
        return $false
    }
    $normalized = $Name.Replace('\', '/')
    if ($normalized.StartsWith('/')) {
        return $false
    }
    if ($normalized -match '^[A-Za-z]:') {
        return $false
    }
    $segments = $normalized.Split('/')
    foreach ($segment in $segments) {
        if ($segment -eq '..') {
            return $false
        }
    }
    return $true
}

function Remove-Tree {
    param([string]$Path)
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
}

function Expand-ZipSafe {
    param([string]$ZipPath, [string]$Destination)
    $zip = $null
    try {
        $zip = [System.IO.Compression.ZipFile]::OpenRead($ZipPath)
        $unsafe = @()
        foreach ($entry in $zip.Entries) {
            if (-not (Test-EntryNameSafe $entry.FullName)) {
                $unsafe += $entry.FullName
            }
        }
        if ($unsafe.Count -gt 0) {
            return [pscustomobject]@{
                Ok     = $false
                Reason = ('unsafe zip entry: ' + ($unsafe -join ' | '))
            }
        }
        Remove-Tree $Destination
        Ensure-Dir $Destination
        $sep = [System.IO.Path]::DirectorySeparatorChar
        $destFull = [System.IO.Path]::GetFullPath($Destination)
        if (-not $destFull.EndsWith([string]$sep)) {
            $destFull = $destFull + $sep
        }
        foreach ($entry in $zip.Entries) {
            $relative = $entry.FullName.Replace('/', '\')
            if ([string]::IsNullOrEmpty($relative)) {
                continue
            }
            $combined = Join-Path $Destination $relative
            $target = [System.IO.Path]::GetFullPath($combined)
            $targetDir = $target
            if (-not $targetDir.EndsWith([string]$sep)) {
                $targetDir = $targetDir + $sep
            }
            $inside = $target.StartsWith($destFull, [System.StringComparison]::OrdinalIgnoreCase) -or $targetDir.Equals($destFull, [System.StringComparison]::OrdinalIgnoreCase)
            if (-not $inside) {
                Remove-Tree $Destination
                return [pscustomobject]@{
                    Ok     = $false
                    Reason = ('zip path escaped after normalization: ' + $entry.FullName)
                }
            }
            if ($relative.EndsWith('\')) {
                Ensure-Dir $target.TrimEnd('\')
                continue
            }
            $parent = Split-Path -Parent $target
            Ensure-Dir $parent
            $inStream = $entry.Open()
            try {
                $outStream = [System.IO.File]::Create($target)
                try {
                    $inStream.CopyTo($outStream)
                } finally {
                    $outStream.Close()
                }
            } finally {
                $inStream.Close()
            }
        }
    } catch {
        Remove-Tree $Destination
        return [pscustomobject]@{
            Ok     = $false
            Reason = $_.Exception.Message
        }
    } finally {
        if ($null -ne $zip) {
            $zip.Dispose()
        }
    }
    return [pscustomobject]@{ Ok = $true; Reason = '' }
}

function Get-RelativePath {
    param([string]$FromDirectory, [string]$ToPath)
    $fromFull = [System.IO.Path]::GetFullPath($FromDirectory).TrimEnd('\')
    $toFull = [System.IO.Path]::GetFullPath($ToPath)
    $fromParts = $fromFull.Split('\')
    $toParts = $toFull.Split('\')
    $index = 0
    while ($index -lt $fromParts.Length -and $index -lt $toParts.Length -and $fromParts[$index].Equals($toParts[$index], [System.StringComparison]::OrdinalIgnoreCase)) {
        $index++
    }
    $parts = @()
    for ($up = $index; $up -lt $fromParts.Length; $up++) {
        $parts += '..'
    }
    for ($down = $index; $down -lt $toParts.Length; $down++) {
        $parts += $toParts[$down]
    }
    if ($parts.Count -eq 0) {
        return '.'
    }
    return ($parts -join '\')
}

function Expand-TarGzSafe {
    param([string]$Archive, [string]$Destination)
    $tar = Join-Path $env:SystemRoot 'System32\tar.exe'
    $archiveDir = Split-Path -Parent $Archive
    $archiveLeaf = Split-Path -Leaf $Archive
    $listed = Invoke-Exe -File $tar -ArgumentList @('-tzf', $archiveLeaf) -WorkingDirectory $archiveDir -TimeoutSec 180
    if ($listed.ExitCode -ne 0) {
        return [pscustomobject]@{
            Ok     = $false
            Reason = ('tar list failed exit ' + $listed.ExitCode + ' ' + (Get-FirstLine $listed.Stderr))
        }
    }
    $unsafe = @()
    $names = @()
    foreach ($line in ($listed.Stdout -split "`r?`n")) {
        $trim = $line.Trim()
        if ($trim -eq '') {
            continue
        }
        $names += $trim
        if (-not (Test-EntryNameSafe $trim)) {
            $unsafe += $trim
        }
    }
    if ($unsafe.Count -gt 0) {
        return [pscustomobject]@{
            Ok     = $false
            Reason = ('unsafe tar entry: ' + ($unsafe -join ' | '))
        }
    }
    $sep = [System.IO.Path]::DirectorySeparatorChar
    $destFull = ''
    Remove-Tree $Destination
    Ensure-Dir $Destination
    $destFull = [System.IO.Path]::GetFullPath($Destination)
    if (-not $destFull.EndsWith([string]$sep)) {
        $destFull = $destFull + $sep
    }
    foreach ($name in $names) {
        $relative = $name.Replace('/', '\')
        $target = [System.IO.Path]::GetFullPath((Join-Path $Destination $relative))
        if (-not $target.StartsWith($destFull, [System.StringComparison]::OrdinalIgnoreCase)) {
            Remove-Tree $Destination
            return [pscustomobject]@{
                Ok     = $false
                Reason = ('tar path escaped after normalization: ' + $name)
            }
        }
    }
    $relativeArchive = Get-RelativePath -FromDirectory $Destination -ToPath $Archive
    $extracted = Invoke-Exe -File $tar -ArgumentList @('-xzf', $relativeArchive) -WorkingDirectory $Destination -TimeoutSec 180
    if ($extracted.ExitCode -ne 0) {
        Remove-Tree $Destination
        return [pscustomobject]@{
            Ok     = $false
            Reason = ('tar extract failed exit ' + $extracted.ExitCode + ' ' + (Get-FirstLine $extracted.Stderr))
        }
    }
    return [pscustomobject]@{ Ok = $true; Reason = '' }
}

function Find-Named {
    param([string]$Root, [string]$Name)
    if (-not (Test-Path -LiteralPath $Root)) {
        return ''
    }
    $items = @(Get-ChildItem -LiteralPath $Root -Filter $Name -Recurse -File -ErrorAction SilentlyContinue)
    if ($items.Count -eq 0) {
        return ''
    }
    $sorted = @($items | Sort-Object { $_.FullName.Length })
    return [string]$sorted[0].FullName
}

function Move-Tree {
    param([string]$Source, [string]$Destination)
    $parent = Split-Path -Parent $Destination
    Ensure-Dir $parent
    Remove-Tree $Destination
    Move-Item -LiteralPath $Source -Destination $Destination
}

function Resolve-Checksum {
    param(
        [string]$Component,
        [string]$Version,
        [string]$FileName,
        [string]$SumsUrl,
        [string]$ApiUrl
    )
    $sumsDigest = ''
    $apiDigest = ''
    $checksumFiles = @()
    $errorText = ''
    if ($SumsUrl) {
        try {
            $sumsText = Get-HttpText $SumsUrl
            $sumsPath = Join-Path $script:DownloadDir ($Component + '-checksums.txt')
            Write-Text $sumsPath $sumsText
            $sumsDigest = Get-ManifestHash $sumsText $FileName
        } catch {
            $errorText = $_.Exception.Message
        }
    }
    if ($ApiUrl) {
        try {
            $json = Get-HttpText $ApiUrl
            $apiDigest = Get-AssetDigest $json $FileName
            foreach ($name in (Get-JsonNames $json)) {
                if ($name -match 'sha256|SHA256|checksum|CHECKSUM|SHASUM') {
                    $checksumFiles += $name
                }
            }
        } catch {
            if ($errorText) {
                $errorText = $errorText + ' | ' + $_.Exception.Message
            } else {
                $errorText = $_.Exception.Message
            }
        }
    }
    $expected = ''
    $source = ''
    $agree = $true
    if ($sumsDigest -and $apiDigest -and ($sumsDigest -ne $apiDigest)) {
        $agree = $false
    } elseif ($sumsDigest) {
        $expected = $sumsDigest
        $source = $SumsUrl
    } elseif ($apiDigest) {
        $expected = $apiDigest
        $source = $ApiUrl + ' asset digest field for ' + $FileName
    }
    $evidenceItem = Get-EvidenceObservation $Component $Version
    $evidenceHash = ''
    $evidenceSource = ''
    if ($evidenceItem) {
        $evidenceHash = ([string]$evidenceItem.sha256).ToLower()
        $evidenceSource = Get-EvidenceSource $evidenceItem
    }
    $notes = @()
    if ($sumsDigest) {
        $notes += ('checksum file digest ' + $sumsDigest)
    } else {
        $notes += 'checksum file digest not found'
    }
    if ($apiDigest) {
        $notes += ('GitHub asset digest ' + $apiDigest)
    } else {
        $notes += 'GitHub asset digest API value not found'
    }
    if ($checksumFiles.Count -gt 0) {
        $notes += ('checksum-like release names: ' + ($checksumFiles -join ', '))
    } else {
        $notes += 'no checksum-like filename found in the release name list'
    }
    if ($evidenceHash) {
        if ($expected -and ($evidenceHash -ne $expected)) {
            $notes += ('verification-evidence.json disagrees (' + $evidenceHash + ' from ' + $evidenceSource + '); expected digest is the publisher value')
        } else {
            $notes += ('verification-evidence.json sha256 ' + $evidenceHash + ' source ' + $evidenceSource)
        }
    } else {
        $notes += 'no windows-x64 observation in verification-evidence.json for this component and version'
    }
    if ($errorText) {
        $notes += $errorText
    }
    $ok = $agree -and ($expected -ne '')
    if (-not $agree) {
        $notes += 'publisher checksum file and GitHub asset digest disagree; payload will not be executed'
    }
    return [pscustomobject]@{
        Ok                     = $ok
        Expected               = $expected
        Source                 = $source
        SumsDigest             = $sumsDigest
        ApiDigest              = $apiDigest
        EvidenceSha256         = $evidenceHash
        EvidenceSha256Source   = $evidenceSource
        Notes                  = ($notes -join '; ')
        PublisherDigestPresent = ($expected -ne '' -and $agree)
    }
}

function Invoke-SyntheticChecks {
    $helloZip = Join-Path $script:Synthetic 'safe.zip'
    $helloDest = Join-Path $script:Synthetic 'safe-dest'
    $helloBytes = [System.Text.Encoding]::ASCII.GetBytes('hello')
    [S0ZipWriter]::WriteStored($helloZip, 'nested/hello.txt', $helloBytes)
    $safe = Expand-ZipSafe -ZipPath $helloZip -Destination $helloDest
    $helloPath = Join-Path $helloDest 'nested\hello.txt'
    $helloText = ''
    if (Test-Path -LiteralPath $helloPath) {
        $helloText = [System.IO.File]::ReadAllText($helloPath)
    }
    $safePass = $safe.Ok -and ($helloText.Trim() -eq 'hello')
    $safeStatus = 'fail'
    if ($safePass) {
        $safeStatus = 'pass'
    }
    Add-Row (New-Check -Component 'extractor-safe-fixture' -Version 'synthetic' -LaunchCommand 'Expand-ZipSafe synthetic nested/hello.txt' -ExitCode $(if ($safePass) { 0 } else { 1 }) -StdoutVersion $helloText.Trim() -Status $safeStatus -Notes $safe.Reason -Cleanup 'synthetic files retained under artifact root' -Limitations 'synthetic zip only; not a runtime archive')

    $dotZip = Join-Path $script:Synthetic 'unsafe-dotdot.zip'
    $dotDest = Join-Path $script:Synthetic 'unsafe-dotdot-dest'
    $escapePath = Join-Path $script:Synthetic 'escape.txt'
    if (Test-Path -LiteralPath $escapePath) {
        Remove-Item -LiteralPath $escapePath -Force
    }
    [S0ZipWriter]::WriteStored($dotZip, '../escape.txt', [System.Text.Encoding]::ASCII.GetBytes('nope'))
    $dot = Expand-ZipSafe -ZipPath $dotZip -Destination $dotDest
    $dotRejected = (-not $dot.Ok) -and ($dot.Reason -match 'unsafe zip entry') -and (-not (Test-Path -LiteralPath $escapePath)) -and (-not (Test-Path -LiteralPath $dotDest))
    Add-Row (New-Check -Component 'extractor-unsafe-dotdot' -Version 'synthetic' -LaunchCommand 'Expand-ZipSafe synthetic ../escape.txt' -ExitCode $(if ($dotRejected) { 0 } else { 1 }) -StdoutVersion $(if ($dotRejected) { 'rejected' } else { 'not-rejected' }) -Status $(if ($dotRejected) { 'pass' } else { 'fail' }) -Notes $dot.Reason -Cleanup 'unsafe zip was not extracted' -Limitations 'synthetic zip only; not a runtime archive')

    $absZip = Join-Path $script:Synthetic 'unsafe-absolute.zip'
    $absDest = Join-Path $script:Synthetic 'unsafe-absolute-dest'
    $marker = Join-Path $script:Synthetic 'abs-marker\pwned.txt'
    $markerUri = (([System.IO.Path]::GetFullPath((Join-Path $script:Synthetic 'abs-marker'))).Replace('\', '/')) + '/pwned.txt'
    if (Test-Path -LiteralPath $marker) {
        Remove-Item -LiteralPath $marker -Force
    }
    [S0ZipWriter]::WriteStored($absZip, $markerUri, [System.Text.Encoding]::ASCII.GetBytes('nope'))
    $abs = Expand-ZipSafe -ZipPath $absZip -Destination $absDest
    $absRejected = (-not $abs.Ok) -and ($abs.Reason -match 'unsafe zip entry') -and (-not (Test-Path -LiteralPath $marker)) -and (-not (Test-Path -LiteralPath $absDest))
    Add-Row (New-Check -Component 'extractor-unsafe-absolute' -Version 'synthetic' -LaunchCommand ('Expand-ZipSafe synthetic ' + $markerUri) -ExitCode $(if ($absRejected) { 0 } else { 1 }) -StdoutVersion $(if ($absRejected) { 'rejected' } else { 'not-rejected' }) -Status $(if ($absRejected) { 'pass' } else { 'fail' }) -Notes $abs.Reason -Cleanup 'unsafe zip was not extracted' -Limitations 'synthetic absolute entry points inside the artifact root so a faulty extractor cannot write outside the disposable directory')

    $payload = Join-Path $script:Synthetic 'digest-mismatch\payload.bin'
    $install = Join-Path $script:Synthetic 'digest-mismatch\install'
    Ensure-Dir (Split-Path -Parent $payload)
    [System.IO.File]::WriteAllBytes($payload, [System.Text.Encoding]::ASCII.GetBytes('not-a-runtime'))
    $wrong = ('0' * 64)
    $gate = Test-DigestGate -File $payload -Expected $wrong -InstallDir $install
    $digestRejected = (-not $gate.Match) -and (-not (Test-Path -LiteralPath $install))
    Add-Row (New-Check -Component 'digest-mismatch' -Version 'synthetic' -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source 'intentional wrong digest for the negative test' -LaunchCommand 'Test-DigestGate payload.bin against 64 zero hex digits' -ExitCode $(if ($digestRejected) { 0 } else { 1 }) -StdoutVersion $(if ($digestRejected) { 'rejected' } else { 'not-rejected' }) -Status $(if ($digestRejected) { 'pass' } else { 'fail' }) -Notes 'digest gate returned before any extract; install directory was not created' -Cleanup 'payload retained; no install directory created' -Limitations 'negative test uses a local file, not a publisher runtime archive')
}

function Invoke-Reuse {
    param([string]$Suffix)
    $nodeComponent = 'reuse-node'
    $pythonComponent = 'reuse-python'
    if ($Suffix) {
        $nodeComponent = 'reuse-node-' + $Suffix
        $pythonComponent = 'reuse-python-' + $Suffix
    }
    $pyEnv = @('PYTHONDONTWRITEBYTECODE=1', 'PYTHONNOUSERSITE=1')
    if (-not (Test-Path -LiteralPath $script:SystemNode)) {
        Add-Row (New-Check -Component $nodeComponent -Status 'fail' -LaunchCommand $script:SystemNode -Notes 'system Node executable is missing' -Cleanup 'system install was not modified')
    } else {
        $version = Invoke-Exe -File $script:SystemNode -ArgumentList @('-v') -WorkingDirectory $script:Root -TimeoutSec 30
        $trivial = Invoke-Exe -File $script:SystemNode -ArgumentList @('-e', "process.stdout.write('reuse-ok')") -WorkingDirectory $script:Root -TimeoutSec 30
        $ok = ($version.ExitCode -eq 0) -and ($trivial.ExitCode -eq 0) -and ($trivial.Stdout.Trim() -eq 'reuse-ok')
        Add-Row (New-Check -Component $nodeComponent -Version (Get-FirstLine $version.Stdout) -LaunchCommand $version.Command -ExitCode $version.ExitCode -StdoutVersion (Get-FirstLine $version.Stdout) -Status $(if ($ok) { 'pass' } else { 'fail' }) -Notes ('trivial ' + $trivial.Command + ' exit ' + $trivial.ExitCode + ' stdout ' + $trivial.Stdout.Trim()) -Cleanup 'executed in place; system install was not copied or modified')
    }
    if (-not (Test-Path -LiteralPath $script:SystemPython)) {
        Add-Row (New-Check -Component $pythonComponent -Status 'fail' -LaunchCommand $script:SystemPython -Notes 'system Python executable is missing' -Cleanup 'system install was not modified')
    } else {
        $version = Invoke-Exe -File $script:SystemPython -ArgumentList @('-c', 'import sys; print(sys.version)') -WorkingDirectory $script:Root -EnvPairs $pyEnv -TimeoutSec 30
        $trivial = Invoke-Exe -File $script:SystemPython -ArgumentList @('-c', 'import ssl, sqlite3, venv; print("reuse-ok")') -WorkingDirectory $script:Root -EnvPairs $pyEnv -TimeoutSec 30
        $ok = ($version.ExitCode -eq 0) -and ($trivial.ExitCode -eq 0) -and ($trivial.Stdout.Trim() -eq 'reuse-ok')
        Add-Row (New-Check -Component $pythonComponent -Version (Get-FirstLine $version.Stdout) -LaunchCommand $version.Command -ExitCode $version.ExitCode -StdoutVersion (Get-FirstLine $version.Stdout) -Status $(if ($ok) { 'pass' } else { 'fail' }) -Notes ('trivial imports exit ' + $trivial.ExitCode + ' stdout ' + $trivial.Stdout.Trim() + '; PYTHONDONTWRITEBYTECODE=1') -Cleanup 'executed in place; system install was not copied or modified')
    }
}

function Invoke-NodeManaged {
    $version = '24.22.2'
    $fileName = 'node-v24.22.2-win-x64.zip'
    $url = 'https://nodejs.org/dist/v24.22.2/node-v24.22.2-win-x64.zip'
    $sumsUrl = 'https://nodejs.org/dist/v24.22.2/SHASUMS256.txt'
    $archive = Join-Path $script:DownloadDir $fileName
    $extract = Join-Path $script:SpaceRoot 'node'
    $relocated = Join-Path $script:Relocated 'node'
    $resolved = Resolve-Checksum -Component 'node' -Version $version -FileName $fileName -SumsUrl $sumsUrl -ApiUrl ''
    if (-not $resolved.Ok) {
        Add-Row (New-Check -Component 'node-managed' -Version $version -Url $url -ExpectedSha256 $resolved.Expected -ExpectedSha256Source $resolved.Source -EvidenceSha256 $resolved.EvidenceSha256 -EvidenceSha256Source $resolved.EvidenceSha256Source -Status 'blocked' -Notes $resolved.Notes -Cleanup 'payload was not executed' -Limitations 'publisher digest unavailable or publisher sources disagreed')
        Add-Row (New-Check -Component 'node-relocated' -Version $version -Url $url -Status 'blocked' -Notes 'skipped because the managed Node digest was not accepted' -Cleanup 'no tree moved')
        return
    }
    try {
        Save-Http $url $archive
    } catch {
        Add-Row (New-Check -Component 'node-managed' -Version $version -Url $url -ExpectedSha256 $resolved.Expected -ExpectedSha256Source $resolved.Source -EvidenceSha256 $resolved.EvidenceSha256 -EvidenceSha256Source $resolved.EvidenceSha256Source -Status 'blocked' -Notes $_.Exception.Message -Cleanup 'no install created' -Limitations 'download failed')
        Add-Row (New-Check -Component 'node-relocated' -Version $version -Url $url -Status 'blocked' -Notes 'skipped because the download failed' -Cleanup 'no tree moved')
        return
    }
    $gate = Test-DigestGate -File $archive -Expected $resolved.Expected -InstallDir $extract
    if (-not $gate.Match) {
        $rejected = Move-Rejected $archive ('expected ' + $gate.Expected + ' actual ' + $gate.Actual)
        Add-Row (New-Check -Component 'node-managed' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -EvidenceSha256 $resolved.EvidenceSha256 -EvidenceSha256Source $resolved.EvidenceSha256Source -Status 'fail' -Notes ('digest mismatch; moved to ' + $rejected + '; ' + $resolved.Notes) -Cleanup 'archive rejected and not extracted' -Limitations 'digest mismatch')
        Add-Row (New-Check -Component 'node-relocated' -Version $version -Url $url -Sha256 $gate.Actual -Status 'blocked' -Notes 'skipped because the digest did not match' -Cleanup 'no tree moved')
        return
    }
    $expanded = Expand-ZipSafe -ZipPath $archive -Destination $extract
    if (-not $expanded.Ok) {
        Add-Row (New-Check -Component 'node-managed' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -Status 'fail' -Notes $expanded.Reason -Cleanup 'extract directory removed or not left as a successful install')
        Add-Row (New-Check -Component 'node-relocated' -Version $version -Url $url -Status 'blocked' -Notes 'skipped because extraction failed' -Cleanup 'no tree moved')
        return
    }
    $nodeExe = Find-Named $extract 'node.exe'
    $spaceOk = ($nodeExe -match ' ')
    if (-not $nodeExe -or -not $spaceOk) {
        Add-Row (New-Check -Component 'node-managed' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -Status 'fail' -Notes ('node.exe path: ' + $nodeExe) -Cleanup 'extracted tree retained')
        Add-Row (New-Check -Component 'node-relocated' -Version $version -Url $url -Status 'blocked' -Notes 'skipped because node.exe was not found under the space path' -Cleanup 'no tree moved')
        return
    }
    $launched = Invoke-Exe -File $nodeExe -ArgumentList @('-v') -WorkingDirectory $script:Root -TimeoutSec 30
    $eval = Invoke-Exe -File $nodeExe -ArgumentList @('-e', "process.stdout.write('node-ok')") -WorkingDirectory $script:Root -TimeoutSec 30
    $versionOk = ((Get-FirstLine $launched.Stdout) -eq ('v' + $version)) -and ($launched.ExitCode -eq 0) -and ($eval.ExitCode -eq 0) -and ($eval.Stdout.Trim() -eq 'node-ok')
    Add-Row (New-Check -Component 'node-managed' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -EvidenceSha256 $resolved.EvidenceSha256 -EvidenceSha256Source $resolved.EvidenceSha256Source -LaunchCommand $launched.Command -ExitCode $launched.ExitCode -StdoutVersion (Get-FirstLine $launched.Stdout) -Status $(if ($versionOk) { 'pass' } else { 'fail' }) -Notes ($resolved.Notes + '; eval ' + $eval.Stdout.Trim() + ' exit ' + $eval.ExitCode + '; path contains a space') -Limitations 'SHA256 compared with the immutable SHASUMS256.txt; Authenticode was not checked. macOS was not tested.')
    if (-not $versionOk) {
        Add-Row (New-Check -Component 'node-relocated' -Version $version -Url $url -Sha256 $gate.Actual -Status 'blocked' -Notes 'skipped because the space-path launch failed' -Cleanup 'original extract retained')
        return
    }
    Move-Tree $extract $relocated
    $movedExe = Find-Named $relocated 'node.exe'
    $stillAtSource = Find-Named $extract 'node.exe'
    $moved = Invoke-Exe -File $movedExe -ArgumentList @('-v') -WorkingDirectory $script:Root -TimeoutSec 30
    $movedOk = ($moved.ExitCode -eq 0) -and ((Get-FirstLine $moved.Stdout) -eq ('v' + $version)) -and (-not $stillAtSource) -and ($movedExe -ne $nodeExe)
    Add-Row (New-Check -Component 'node-relocated' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -LaunchCommand $moved.Command -ExitCode $moved.ExitCode -StdoutVersion (Get-FirstLine $moved.Stdout) -Status $(if ($movedOk) { 'pass' } else { 'fail' }) -Notes ('moved from ' + $nodeExe + ' to ' + $movedExe) -Cleanup 'relocated tree retained under artifact root')
    $script:ManagedNode = $movedExe
}

function Invoke-PythonManaged {
    $version = '3.12.16'
    $fileName = 'cpython-3.12.16+20261026-x86_64-pc-windows-msvc-install_only_stripped.tar.gz'
    $url = 'https://github.com/astral-sh/python-build-standalone/releases/download/20261026/cpython-3.12.16%2B20261026-x86_64-pc-windows-msvc-install_only_stripped.tar.gz'
    $sumsUrl = 'https://github.com/astral-sh/python-build-standalone/releases/download/20261026/SHA256SUMS'
    $apiUrl = 'https://api.github.com/repos/astral-sh/python-build-standalone/releases/tags/20261026'
    $archive = Join-Path $script:DownloadDir $fileName
    $extract = Join-Path $script:SpaceRoot 'python'
    $relocated = Join-Path $script:Relocated 'python'
    $resolved = Resolve-Checksum -Component 'python' -Version $version -FileName $fileName -SumsUrl $sumsUrl -ApiUrl $apiUrl
    if (-not $resolved.Ok) {
        Add-Row (New-Check -Component 'python-managed' -Version $version -Url $url -ExpectedSha256 $resolved.Expected -ExpectedSha256Source $resolved.Source -EvidenceSha256 $resolved.EvidenceSha256 -EvidenceSha256Source $resolved.EvidenceSha256Source -Status 'blocked' -Notes $resolved.Notes -Cleanup 'payload was not executed')
        Add-Row (New-Check -Component 'python-relocated' -Version $version -Url $url -Status 'blocked' -Notes 'skipped because the managed Python digest was not accepted' -Cleanup 'no tree moved')
        Add-Row (New-Check -Component 'python-venv' -Version $version -Status 'blocked' -Notes 'skipped because managed Python was not accepted' -Cleanup 'no venv created')
        Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Status 'blocked' -Notes 'skipped because managed Python was not accepted' -Cleanup 'system interpreter was not used for pip')
        return
    }
    try {
        Save-Http $url $archive
    } catch {
        Add-Row (New-Check -Component 'python-managed' -Version $version -Url $url -ExpectedSha256 $resolved.Expected -ExpectedSha256Source $resolved.Source -Status 'blocked' -Notes $_.Exception.Message -Cleanup 'no install created')
        Add-Row (New-Check -Component 'python-relocated' -Version $version -Url $url -Status 'blocked' -Notes 'skipped because the download failed' -Cleanup 'no tree moved')
        Add-Row (New-Check -Component 'python-venv' -Version $version -Status 'blocked' -Notes 'skipped because the download failed' -Cleanup 'no venv created')
        Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Status 'blocked' -Notes 'skipped because the download failed' -Cleanup 'system interpreter was not used for pip')
        return
    }
    $gate = Test-DigestGate -File $archive -Expected $resolved.Expected -InstallDir $extract
    if (-not $gate.Match) {
        $rejected = Move-Rejected $archive ('expected ' + $gate.Expected + ' actual ' + $gate.Actual)
        Add-Row (New-Check -Component 'python-managed' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -Status 'fail' -Notes ('digest mismatch; moved to ' + $rejected) -Cleanup 'archive rejected and not extracted')
        Add-Row (New-Check -Component 'python-relocated' -Version $version -Url $url -Status 'blocked' -Notes 'skipped because the digest did not match' -Cleanup 'no tree moved')
        Add-Row (New-Check -Component 'python-venv' -Version $version -Status 'blocked' -Notes 'skipped because the digest did not match' -Cleanup 'no venv created')
        Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Status 'blocked' -Notes 'skipped because the digest did not match' -Cleanup 'system interpreter was not used for pip')
        return
    }
    $expanded = Expand-TarGzSafe -Archive $archive -Destination $extract
    if (-not $expanded.Ok) {
        Add-Row (New-Check -Component 'python-managed' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -Status 'fail' -Notes $expanded.Reason -Cleanup 'extract directory removed after extract failure')
        Add-Row (New-Check -Component 'python-relocated' -Version $version -Url $url -Status 'blocked' -Notes 'skipped because extraction failed' -Cleanup 'no tree moved')
        Add-Row (New-Check -Component 'python-venv' -Version $version -Status 'blocked' -Notes 'skipped because extraction failed' -Cleanup 'no venv created')
        Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Status 'blocked' -Notes 'skipped because extraction failed' -Cleanup 'system interpreter was not used for pip')
        return
    }
    $pythonExe = Find-Named $extract 'python.exe'
    if (-not $pythonExe -or ($pythonExe -notmatch ' ')) {
        Add-Row (New-Check -Component 'python-managed' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -Status 'fail' -Notes ('python.exe path: ' + $pythonExe) -Cleanup 'extracted tree retained')
        Add-Row (New-Check -Component 'python-relocated' -Version $version -Url $url -Status 'blocked' -Notes 'skipped because python.exe was not under a space path' -Cleanup 'no tree moved')
        Add-Row (New-Check -Component 'python-venv' -Version $version -Status 'blocked' -Notes 'skipped' -Cleanup 'no venv created')
        Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Status 'blocked' -Notes 'skipped' -Cleanup 'system interpreter was not used for pip')
        return
    }
    $pyEnv = @(
        'PYTHONDONTWRITEBYTECODE=1',
        'PYTHONNOUSERSITE=1',
        ('PYTHONUSERBASE=' + (Join-Path $script:Root 'python-user'))
    )
    $importCmd = 'import ssl, sqlite3, venv, sys; print(sys.version); print("imports-ok")'
    $launched = Invoke-Exe -File $pythonExe -ArgumentList @('-c', $importCmd) -WorkingDirectory $script:Root -EnvPairs $pyEnv -TimeoutSec 60
    $importOk = ($launched.ExitCode -eq 0) -and ($launched.Stdout -match '3\.12\.16') -and ($launched.Stdout -match 'imports-ok')
    Add-Row (New-Check -Component 'python-managed' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -EvidenceSha256 $resolved.EvidenceSha256 -EvidenceSha256Source $resolved.EvidenceSha256Source -LaunchCommand $launched.Command -ExitCode $launched.ExitCode -StdoutVersion (Get-FirstLine $launched.Stdout) -Status $(if ($importOk) { 'pass' } else { 'fail' }) -Notes ($resolved.Notes + '; build 20261026 install_only_stripped; path contains a space') -Limitations 'SHA256 compared with release SHA256SUMS and the GitHub asset digest. Authenticode/PGP was not checked. macOS was not tested.')
    if (-not $importOk) {
        Add-Row (New-Check -Component 'python-relocated' -Version $version -Url $url -Status 'blocked' -Notes 'skipped because the space-path import check failed' -Cleanup 'original extract retained')
        Add-Row (New-Check -Component 'python-venv' -Version $version -Status 'blocked' -Notes 'skipped' -Cleanup 'no venv created')
        Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Status 'blocked' -Notes 'skipped' -Cleanup 'system interpreter was not used for pip')
        return
    }
    Move-Tree $extract $relocated
    $movedExe = Find-Named $relocated 'python.exe'
    $stillAtSource = Find-Named $extract 'python.exe'
    $moved = Invoke-Exe -File $movedExe -ArgumentList @('-c', $importCmd) -WorkingDirectory $script:Root -EnvPairs $pyEnv -TimeoutSec 60
    $movedOk = ($moved.ExitCode -eq 0) -and ($moved.Stdout -match 'imports-ok') -and (-not $stillAtSource)
    Add-Row (New-Check -Component 'python-relocated' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -LaunchCommand $moved.Command -ExitCode $moved.ExitCode -StdoutVersion (Get-FirstLine $moved.Stdout) -Status $(if ($movedOk) { 'pass' } else { 'fail' }) -Notes ('moved from ' + $pythonExe + ' to ' + $movedExe) -Cleanup 'relocated tree retained under artifact root')
    if (-not $movedOk) {
        Add-Row (New-Check -Component 'python-venv' -Version $version -Status 'blocked' -Notes 'skipped because relocation failed' -Cleanup 'no venv created')
        Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Status 'blocked' -Notes 'skipped because relocation failed' -Cleanup 'system interpreter was not used for pip')
        return
    }
    $script:ManagedPython = $movedExe
    $venv = Join-Path $script:VenvRoot 'pip-smoke'
    if (-not (Test-SameOrUnder $script:Root $venv)) {
        Add-Row (New-Check -Component 'python-venv' -Status 'fail' -Notes 'venv path escaped the artifact root' -Cleanup 'no venv created')
        Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Status 'blocked' -Notes 'skipped' -Cleanup 'system interpreter was not used for pip')
        return
    }
    Remove-Tree $venv
    $venvRun = Invoke-Exe -File $movedExe -ArgumentList @('-m', 'venv', $venv) -WorkingDirectory $script:Root -EnvPairs $pyEnv -TimeoutSec 120
    $venvPython = Join-Path $venv 'Scripts\python.exe'
    $venvOk = ($venvRun.ExitCode -eq 0) -and (Test-Path -LiteralPath $venvPython)
    Add-Row (New-Check -Component 'python-venv' -Version $version -LaunchCommand $venvRun.Command -ExitCode $venvRun.ExitCode -StdoutVersion $(if ($venvOk) { 'venv-created' } else { 'venv-failed' }) -Status $(if ($venvOk) { 'pass' } else { 'fail' }) -Notes ('venv python ' + $venvPython + '; stderr ' + (Get-FirstLine $venvRun.Stderr)) -Cleanup 'venv retained under artifact root' -Limitations 'venv was created from the relocated managed interpreter only')
    if (-not $venvOk) {
        Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Status 'blocked' -Notes 'skipped because venv creation failed' -Cleanup 'system interpreter was not used for pip')
        return
    }
    $pipConfig = Join-Path $script:Root 'pip-isolated.ini'
    Write-Text $pipConfig "[global]`r`ndisable-pip-version-check = 1`r`n"
    $pipCache = Join-Path $script:Root 'pip-cache'
    Ensure-Dir $pipCache
    $pipEnv = $pyEnv + @(
        ('PIP_CACHE_DIR=' + $pipCache),
        ('PIP_CONFIG_FILE=' + $pipConfig),
        'PIP_DISABLE_PIP_VERSION_CHECK=1'
    )
    if (-not (Test-SameOrUnder $script:Root $venvPython)) {
        Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Status 'fail' -Notes ('refusing pip outside the artifact root: ' + $venvPython) -Cleanup 'system interpreter was not used for pip')
        return
    }
    $pip = Invoke-Exe -File $venvPython -ArgumentList @('-m', 'pip', 'install', 'six==1.17.0', '--disable-pip-version-check', '--no-warn-script-location', '--index-url', 'https://pypi.org/simple') -WorkingDirectory $script:Root -EnvPairs $pipEnv -TimeoutSec 300
    $shown = Invoke-Exe -File $venvPython -ArgumentList @('-c', 'import six; print(six.__version__)') -WorkingDirectory $venv -EnvPairs $pipEnv -TimeoutSec 30
    $pipOk = ($pip.ExitCode -eq 0) -and ($shown.ExitCode -eq 0) -and ($shown.Stdout.Trim() -eq '1.17.0')
    Add-Row (New-Check -Component 'python-pip' -Version 'six==1.17.0' -Url 'https://pypi.org/simple/six/' -LaunchCommand $pip.Command -ExitCode $pip.ExitCode -StdoutVersion ($shown.Stdout.Trim()) -Status $(if ($pipOk) { 'pass' } else { 'fail' }) -Notes ('venv interpreter ' + $venvPython + '; import exit ' + $shown.ExitCode) -Cleanup 'package installed only into the artifact venv' -Limitations 'one pinned package; pip did not rewrite a project requirements file, which is normal for pip install')
}

function Invoke-UvManaged {
    $version = '0.13.6'
    $fileName = 'uv-x86_64-pc-windows-msvc.zip'
    $url = 'https://github.com/astral-sh/uv/releases/download/0.13.6/uv-x86_64-pc-windows-msvc.zip'
    $sumsUrl = 'https://github.com/astral-sh/uv/releases/download/0.13.6/sha256.sum'
    $apiUrl = 'https://api.github.com/repos/astral-sh/uv/releases/tags/0.13.6'
    $archive = Join-Path $script:DownloadDir $fileName
    $extract = Join-Path $script:SpaceRoot 'uv'
    $resolved = Resolve-Checksum -Component 'uv' -Version $version -FileName $fileName -SumsUrl $sumsUrl -ApiUrl $apiUrl
    if (-not $resolved.PublisherDigestPresent) {
        Add-Row (New-Check -Component 'uv' -Version $version -Url $url -ExpectedSha256 $resolved.Expected -ExpectedSha256Source $resolved.Source -EvidenceSha256 $resolved.EvidenceSha256 -EvidenceSha256Source $resolved.EvidenceSha256Source -Status 'blocked' -Notes ($resolved.Notes + '; uv was not executed') -Cleanup 'payload was not executed' -Limitations 'uv runs only after the bytes match one agreed publisher digest')
        return
    }
    try {
        Save-Http $url $archive
    } catch {
        Add-Row (New-Check -Component 'uv' -Version $version -Url $url -ExpectedSha256 $resolved.Expected -ExpectedSha256Source $resolved.Source -Status 'blocked' -Notes $_.Exception.Message -Cleanup 'no install created')
        return
    }
    $gate = Test-DigestGate -File $archive -Expected $resolved.Expected -InstallDir $extract
    if (-not $gate.Match) {
        $rejected = Move-Rejected $archive ('expected ' + $gate.Expected + ' actual ' + $gate.Actual)
        Add-Row (New-Check -Component 'uv' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -Status 'fail' -Notes ('digest mismatch; uv was not executed; ' + $rejected) -Cleanup 'archive rejected and not extracted')
        return
    }
    $expanded = Expand-ZipSafe -ZipPath $archive -Destination $extract
    if (-not $expanded.Ok) {
        Add-Row (New-Check -Component 'uv' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -Status 'fail' -Notes $expanded.Reason -Cleanup 'extract directory not left as a successful install')
        return
    }
    $uvExe = Find-Named $extract 'uv.exe'
    if (-not $uvExe) {
        Add-Row (New-Check -Component 'uv' -Version $version -Url $url -Sha256 $gate.Actual -Status 'fail' -Notes 'uv.exe was not in the archive' -Cleanup 'extracted tree retained')
        return
    }
    $uvCache = Join-Path $script:UvHome 'cache'
    $uvPython = Join-Path $script:UvHome 'python'
    $uvTools = Join-Path $script:UvHome 'tools'
    Ensure-Dir $uvCache
    Ensure-Dir $uvPython
    Ensure-Dir $uvTools
    $uvEnv = @(
        ('UV_CACHE_DIR=' + $uvCache),
        ('UV_PYTHON_INSTALL_DIR=' + $uvPython),
        ('UV_TOOL_DIR=' + $uvTools),
        'UV_PYTHON_DOWNLOADS=never',
        'UV_NO_CACHE=1'
    )
    $launched = Invoke-Exe -File $uvExe -ArgumentList @('--version') -WorkingDirectory $script:Root -EnvPairs $uvEnv -TimeoutSec 30
    $ok = ($launched.ExitCode -eq 0) -and ($launched.Stdout -match '0\.13\.6')
    Add-Row (New-Check -Component 'uv' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -EvidenceSha256 $resolved.EvidenceSha256 -EvidenceSha256Source $resolved.EvidenceSha256Source -LaunchCommand $launched.Command -ExitCode $launched.ExitCode -StdoutVersion (Get-FirstLine $launched.Stdout) -Status $(if ($ok) { 'pass' } else { 'fail' }) -Notes ($resolved.Notes + '; env UV_CACHE_DIR, UV_PYTHON_INSTALL_DIR, UV_TOOL_DIR under the artifact root; UV_PYTHON_DOWNLOADS=never; UV_NO_CACHE=1; no uv python install was requested') -Limitations 'version probe only. Publisher digest is sha256.sum corroborated by the GitHub asset digest. macOS was not tested.')
}

function Invoke-PnpmManaged {
    $version = '12.16.1'
    $fileName = 'pnpm-win32-x64.zip'
    $url = 'https://github.com/pnpm/pnpm/releases/download/v12.16.1/pnpm-win32-x64.zip'
    $apiUrl = 'https://api.github.com/repos/pnpm/pnpm/releases/tags/v12.16.1'
    $archive = Join-Path $script:DownloadDir $fileName
    $extract = Join-Path $script:SpaceRoot 'pnpm'
    if (-not $script:ManagedNode) {
        Add-Row (New-Check -Component 'pnpm' -Version $version -Url $url -Status 'blocked' -Notes 'managed Node was not available, so pnpm was not run' -Cleanup 'no pnpm install created' -Limitations 'pnpm is pinned to GitHub release v12.16.1')
        return
    }
    $resolved = Resolve-Checksum -Component 'pnpm' -Version $version -FileName $fileName -SumsUrl '' -ApiUrl $apiUrl
    if (-not $resolved.PublisherDigestPresent) {
        Add-Row (New-Check -Component 'pnpm' -Version $version -Url $url -ExpectedSha256 $resolved.Expected -ExpectedSha256Source $resolved.Source -EvidenceSha256 $resolved.EvidenceSha256 -EvidenceSha256Source $resolved.EvidenceSha256Source -Status 'blocked' -Notes ($resolved.Notes + '; publisher digest API was not available or disagreed; pnpm was not executed') -Cleanup 'payload was not executed' -Limitations 'v12.16.1 was selected by reading the official pnpm/pnpm GitHub release. A separate checksum file was not required when the asset digest field is present.')
        return
    }
    try {
        Save-Http $url $archive
    } catch {
        Add-Row (New-Check -Component 'pnpm' -Version $version -Url $url -ExpectedSha256 $resolved.Expected -ExpectedSha256Source $resolved.Source -Status 'blocked' -Notes $_.Exception.Message -Cleanup 'no install created')
        return
    }
    $gate = Test-DigestGate -File $archive -Expected $resolved.Expected -InstallDir $extract
    if (-not $gate.Match) {
        $rejected = Move-Rejected $archive ('expected ' + $gate.Expected + ' actual ' + $gate.Actual)
        Add-Row (New-Check -Component 'pnpm' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -Status 'fail' -Notes ('digest mismatch; pnpm was not executed; ' + $rejected) -Cleanup 'archive rejected and not extracted')
        return
    }
    $expanded = Expand-ZipSafe -ZipPath $archive -Destination $extract
    if (-not $expanded.Ok) {
        Add-Row (New-Check -Component 'pnpm' -Version $version -Url $url -Sha256 $gate.Actual -Status 'fail' -Notes $expanded.Reason -Cleanup 'extract directory not left as a successful install')
        return
    }
    $pnpmExe = Find-Named $extract 'pnpm.exe'
    if (-not $pnpmExe) {
        Add-Row (New-Check -Component 'pnpm' -Version $version -Url $url -Sha256 $gate.Actual -Status 'fail' -Notes 'pnpm.exe was not in the archive' -Cleanup 'extracted tree retained')
        return
    }
    $project = Join-Path $script:SpaceRoot 'pnpm-project'
    $store = Join-Path $script:Root 'pnpm-store'
    $pnpmHome = Join-Path $script:Root 'pnpm-home'
    $npmCache = Join-Path $script:Root 'npm-cache'
    Ensure-Dir $project
    Ensure-Dir $store
    Ensure-Dir $pnpmHome
    Ensure-Dir $npmCache
    $userconfig = Join-Path $script:Root 'empty-user.npmrc'
    Write-Text $userconfig "# isolated from the user npmrc`r`n"
    $packageJson = @"
{
  "name": "s0-win-runtimes-pnpm-smoke",
  "private": true,
  "version": "0.0.0",
  "dependencies": {
    "is-number": "7.0.0"
  }
}
"@
    Write-Text (Join-Path $project 'package.json') $packageJson
    Write-Text (Join-Path $project '.npmrc') "ignore-scripts=true`r`nregistry=https://registry.npmjs.org/`r`n"
    $nodeDir = Split-Path -Parent $script:ManagedNode
    $pnpmEnv = @(
        ('Path=' + $nodeDir + ';' + $script:OriginalPath),
        ('PNPM_HOME=' + $pnpmHome),
        ('npm_config_registry=https://registry.npmjs.org/'),
        ('npm_config_userconfig=' + $userconfig),
        ('npm_config_cache=' + $npmCache),
        'npm_config_update_notifier=false'
    )
    $versionRun = Invoke-Exe -File $pnpmExe -ArgumentList @('--version') -WorkingDirectory $project -EnvPairs $pnpmEnv -TimeoutSec 30
    $install = Invoke-Exe -File $pnpmExe -ArgumentList @('install', '--store-dir', $store, '--registry', 'https://registry.npmjs.org/', '--ignore-scripts') -WorkingDirectory $project -EnvPairs $pnpmEnv -TimeoutSec 300
    $lock = Join-Path $project 'pnpm-lock.yaml'
    $require = Invoke-Exe -File $script:ManagedNode -ArgumentList @('-e', "process.stdout.write(String(require('is-number')('1')))") -WorkingDirectory $project -EnvPairs $pnpmEnv -TimeoutSec 30
    $lockOk = Test-Path -LiteralPath $lock
    $ok = ($versionRun.ExitCode -eq 0) -and ($versionRun.Stdout.Trim() -eq $version) -and ($install.ExitCode -eq 0) -and $lockOk -and ($require.ExitCode -eq 0) -and ($require.Stdout.Trim() -eq 'true')
    Add-Row (New-Check -Component 'pnpm' -Version $version -Url $url -Sha256 $gate.Actual -ExpectedSha256 $gate.Expected -ExpectedSha256Source $resolved.Source -EvidenceSha256 $resolved.EvidenceSha256 -EvidenceSha256Source $resolved.EvidenceSha256Source -LaunchCommand $install.Command -ExitCode $install.ExitCode -StdoutVersion ('pnpm ' + $versionRun.Stdout.Trim() + '; is-number ' + $require.Stdout.Trim()) -Status $(if ($ok) { 'pass' } else { 'fail' }) -Notes ($resolved.Notes + '; lockfile ' + $lock + ' present=' + $lockOk + '; managed Node prepended only for the pnpm and require processes; Corepack was not enabled; npm install -g was not used') -Cleanup 'pnpm project and lockfile retained under the artifact root, outside Git' -Limitations 'v12.16.1 Windows x64 zip from the official pnpm/pnpm GitHub release. Digest source is the GitHub asset digest field. One pinned dependency, is-number 7.0.0, with ignore-scripts. macOS was not tested.')
}

function Write-ResultJson {
    $rows = @()
    foreach ($row in $script:Results) {
        $rows += $row
    }
    $failed = 0
    foreach ($row in $rows) {
        if ($row.status -ne 'pass') {
            $failed++
        }
    }
    $doc = [ordered]@{
        task            = 'S0-WIN-RUNTIMES'
        checked_on      = $script:CheckedOn
        os              = $script:OsText
        arch            = 'x64'
        powershell      = $PSVersionTable.PSVersion.ToString()
        artifact_root   = $script:Root
        result_json     = $script:ResultPath
        cleanup         = $script:CleanupSummary
        limitations     = 'Payload digests were compared with publisher checksum files or GitHub asset digests. Authenticode and PGP signatures were not checked. macOS was not tested. The product was not built.'
        fatal           = $script:Fatal
        failed_count    = $failed
        result_count    = $rows.Count
        results         = $rows
    }
    $json = $doc | ConvertTo-Json -Depth 8
    Write-Text $script:ResultPath $json
    $summary = New-Object System.Text.StringBuilder
    [void]$summary.AppendLine('failed=' + $failed + ' count=' + $rows.Count)
    foreach ($row in $rows) {
        [void]$summary.AppendLine($row.status + ' ' + $row.component + ' ' + $row.stdout_version)
    }
    Write-Text (Join-Path $script:LogDir 'summary.txt') $summary.ToString()
}

$ArtifactRoot = [System.IO.Path]::GetFullPath($ArtifactRoot)
if (-not (Test-SameOrUnder $script:AllowedRoot $ArtifactRoot)) {
    Write-Output ('ArtifactRoot must stay under ' + $script:AllowedRoot)
    exit 2
}
if ([string]::IsNullOrWhiteSpace($ResultJson)) {
    $ResultJson = Join-Path $ArtifactRoot 'result.json'
}
$ResultJson = [System.IO.Path]::GetFullPath($ResultJson)
if (-not (Test-SameOrUnder $ArtifactRoot $ResultJson)) {
    Write-Output 'ResultJson must stay under ArtifactRoot'
    exit 2
}

$script:Root = $ArtifactRoot
$script:ResultPath = $ResultJson
$script:RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))

try {
    $current = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
    $osCim = Get-CimInstance Win32_OperatingSystem
    $script:OsText = '{0} build {1} version {2} {3}' -f $current.ProductName, $current.CurrentBuild, $current.DisplayVersion, $osCim.OSArchitecture
} catch {
    $script:OsText = $_.Exception.Message
}

Install-NativeHelpers

Ensure-Dir $script:Root
$script:DownloadDir = Join-Path $script:Root 'downloads'
$script:RejectedDir = Join-Path $script:Root 'rejected'
$script:Synthetic = Join-Path $script:Root 'synthetic'
$script:SpaceRoot = Join-Path $script:Root 'path with spaces'
$script:Relocated = Join-Path $script:Root 'relocated'
$script:VenvRoot = Join-Path $script:Root 'venvs'
$script:UvHome = Join-Path $script:Root 'uv-home'
$script:LogDir = Join-Path $script:Root 'logs'
$script:CommandLog = Join-Path $script:LogDir 'commands.txt'
$script:ManagedNode = ''
$script:ManagedPython = ''

foreach ($dir in @($script:DownloadDir, $script:RejectedDir, $script:Synthetic, $script:SpaceRoot, $script:Relocated, $script:VenvRoot, $script:UvHome, $script:LogDir)) {
    Remove-Tree $dir
    Ensure-Dir $dir
}
Write-Text $script:CommandLog ''

$evidencePath = Join-Path $script:RepoRoot 'docs\verification-evidence.json'
try {
    $script:Evidence = (Get-Content -LiteralPath $evidencePath -Raw) | ConvertFrom-Json
} catch {
    $script:Evidence = $null
    $script:Fatal = 'could not read verification-evidence.json: ' + $_.Exception.Message
}

try {
    Invoke-SyntheticChecks
    Invoke-Reuse ''
    Invoke-NodeManaged
    Invoke-PythonManaged
    Invoke-UvManaged
    Invoke-PnpmManaged
    Invoke-Reuse 'after'
} catch {
    $script:Fatal = $_.Exception.ToString()
    Add-Row (New-Check -Component 'unhandled' -Status 'fail' -Notes $script:Fatal -Cleanup $script:CleanupSummary)
} finally {
    try {
        Write-ResultJson
    } catch {
        Write-Output ('result json write failed: ' + $_.Exception.Message)
    }
}

$exitCode = 0
foreach ($row in $script:Results) {
    if ($row.status -ne 'pass') {
        $exitCode = 1
        break
    }
}
exit $exitCode
