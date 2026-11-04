param([string]$ArtifactRoot = "$PSScriptRoot/../../../.artifacts/S8A-MEMCACHED")
$ErrorActionPreference = 'Stop'
$artifactUrl = 'https://github.com/jefyt/memcached-windows/releases/download/1.6.8_mingw_libressl/memcached-1.6.8-win64-mingw.zip'
$expectedSha256 = '48ec62cef718f0d73698414b783c0e4a69821013553ca00afe0eed324eb5994b'
$taskRoot = [IO.Path]::GetFullPath($ArtifactRoot)
New-Item -ItemType Directory -Force -Path $taskRoot | Out-Null
$archive = Join-Path $taskRoot 'memcached-1.6.8-win64-mingw.zip'
if (!(Test-Path -LiteralPath $archive)) { Invoke-WebRequest $artifactUrl -OutFile $archive }
$actualSha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualSha256 -ne $expectedSha256) { throw 'Memcached SHA256 mismatch; nothing extracted or launched' }
$extractRoot = Join-Path $taskRoot ('native space ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $extractRoot | Out-Null
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {
    foreach ($entry in $zip.Entries) {
        $entryName = $entry.FullName.Replace('\', '/')
        if ($entryName.StartsWith('/') -or $entryName.Contains(':') -or ($entryName.Split('/') -contains '..')) { throw 'Unsafe Memcached archive entry' }
        $target = [IO.Path]::GetFullPath((Join-Path $extractRoot $entryName))
        if (!$target.StartsWith($extractRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Memcached archive entry escapes destination' }
    }
} finally { $zip.Dispose() }
Expand-Archive -LiteralPath $archive -DestinationPath $extractRoot
$executable = @(Get-ChildItem -LiteralPath $extractRoot -Filter memcached.exe -Recurse)
if ($executable.Count -ne 1) { throw 'Expected exactly one memcached.exe' }
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()
$probeKey = 'stackyard:probe:' + [guid]::NewGuid().ToString('N')
$process = $null
$client = $null
try {
    $process = Start-Process -FilePath $executable[0].FullName -ArgumentList @('-l', '127.0.0.1', '-p', "$port", '-U', '0', '-m', '16', '-t', '1') -WorkingDirectory $extractRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $extractRoot 'stdout.log') -RedirectStandardError (Join-Path $extractRoot 'stderr.log')
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        if ($process.HasExited) { throw 'Memcached exited before readiness; inspect task stderr.log' }
        try { $client = [Net.Sockets.TcpClient]::new(); $client.Connect('127.0.0.1', $port); break } catch { $client.Dispose(); $client = $null; Start-Sleep -Milliseconds 50 }
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($null -eq $client) { throw 'Memcached readiness timeout' }
    $client.ReceiveTimeout = 2000
    $client.SendTimeout = 2000
    $stream = $client.GetStream()
    function Send-Line([string]$Text) { $bytes = [Text.Encoding]::ASCII.GetBytes($Text); $stream.Write($bytes, 0, $bytes.Length) }
    function Read-Line {
        $bytes = [Collections.Generic.List[byte]]::new()
        while ($bytes.Count -lt 512) {
            $next = $stream.ReadByte()
            if ($next -lt 0) { throw 'Memcached closed response' }
            $bytes.Add([byte]$next)
            if ($next -eq 10) { return [Text.Encoding]::ASCII.GetString($bytes.ToArray()).TrimEnd("`r", "`n") }
        }
        throw 'Memcached response exceeded 512 bytes'
    }
    Send-Line "version`r`n"
    $version = Read-Line
    if ($version -ne 'VERSION 1.6.8') { throw "Unexpected version: $version" }
    Send-Line "add $probeKey 0 30 5`r`nhello`r`n"
    if ((Read-Line) -ne 'STORED') { throw 'Memcached add failed' }
    Send-Line "set $probeKey 0 30 5`r`nworld`r`n"
    if ((Read-Line) -ne 'STORED') { throw 'Memcached set failed' }
    Send-Line "get $probeKey`r`n"
    if ((Read-Line) -ne "VALUE $probeKey 0 5" -or (Read-Line) -ne 'world' -or (Read-Line) -ne 'END') { throw 'Memcached get failed' }
    Send-Line "delete $probeKey`r`n"
    if ((Read-Line) -ne 'DELETED') { throw 'Memcached delete failed' }
    Send-Line "get $probeKey`r`n"
    if ((Read-Line) -ne 'END') { throw 'Memcached cleanup verification failed' }
    $conflict = Start-Process -FilePath $executable[0].FullName -ArgumentList @('-l', '127.0.0.1', '-p', "$port", '-U', '0', '-m', '16', '-t', '1') -WorkingDirectory $extractRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $extractRoot 'conflict-stdout.log') -RedirectStandardError (Join-Path $extractRoot 'conflict-stderr.log')
    try {
        if (!$conflict.WaitForExit(5000) -or $conflict.ExitCode -eq 0) {
            [pscustomobject]@{ OriginalPid = $process.Id; ConflictPid = $conflict.Id; ConflictExited = $conflict.HasExited; ConflictExitCode = $conflict.ExitCode } | ConvertTo-Json
            Get-NetTCPConnection -LocalPort $port -State Listen | Select-Object LocalAddress, LocalPort, OwningProcess | ConvertTo-Json
            throw 'Occupied-port launch did not fail promptly; artifact remains unsuitable for managed lifecycle acceptance'
        }
        Send-Line "version`r`n"
        if ((Read-Line) -ne $version -or $process.HasExited) { throw 'Occupied-port attempt disturbed original server' }
    } finally {
        if (!$conflict.HasExited) { Stop-Process -Id $conflict.Id; $conflict.WaitForExit(3000) | Out-Null }
    }
    [pscustomobject]@{ Version = $version; Port = $port; SHA256 = $actualSha256; Roundtrip = 'passed'; KeyCleanup = 'passed'; OccupiedPort = 'refused; original server retained'; Directory = $extractRoot } | ConvertTo-Json
} finally {
    if ($null -ne $client) { $client.Dispose() }
    if ($null -ne $process -and !$process.HasExited) { Stop-Process -Id $process.Id; $process.WaitForExit(3000) | Out-Null }
    if ($null -ne $process -and !$process.HasExited) { throw 'Memcached process cleanup failed' }
    $released = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $port)
    try { $released.Start(); 'Process cleanup and port release: passed' } finally { $released.Stop() }
}
