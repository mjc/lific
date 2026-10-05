param(
    [switch]$Collect,
    [string]$IdentityFile,
    [string]$DumpPath,
    [string]$OutputDirectory = "target/windows-stopped-reader-diagnostic"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Confirm-Identity($Process, $Expected) {
    $Process.Refresh()
    if ($Process.HasExited) { throw "Recorded process $($Process.Id) exited" }
    $actual = Get-CimInstance Win32_Process -Filter "ProcessId = $($Process.Id)" -OperationTimeoutSec 2
    if ($Process.Id -ne $Expected.ProcessId -or $null -eq $actual -or
        -not [StringComparer]::OrdinalIgnoreCase.Equals($actual.ExecutablePath, $Expected.ExecutablePath) -or
        $actual.CommandLine -cne $Expected.CommandLine -or
        $Process.StartTime.ToUniversalTime().Ticks -ne $Expected.StartTimeTicks) {
        throw "Process identity/start time changed for $($Process.Id)"
    }
    return $actual
}

function Identity($Process) {
    $Process.Refresh()
    if ($Process.HasExited) { return $null }
    $actual = Get-CimInstance Win32_Process -Filter "ProcessId = $($Process.Id)" -OperationTimeoutSec 2
    $Process.Refresh()
    if ($Process.HasExited) { return $null }
    if ($null -eq $actual -or $null -eq $actual.ExecutablePath -or $null -eq $actual.CommandLine) {
        throw "Cannot establish process identity for $($Process.Id)"
    }
    return [pscustomobject]@{
        ProcessId = $Process.Id
        ExecutablePath = $actual.ExecutablePath
        CommandLine = $actual.CommandLine
        StartTimeTicks = $Process.StartTime.ToUniversalTime().Ticks
        StartTimeUtc = $Process.StartTime.ToUniversalTime().ToString("O")
    }
}

function Stop-Verified($Process, $Expected) {
    $Process.Refresh()
    if (-not $Process.HasExited) {
        Confirm-Identity $Process $Expected | Out-Null
        $Process.Kill()
        if (-not $Process.WaitForExit(3000)) { throw "Process $($Process.Id) did not retire within 3 seconds" }
    }
}

function Quote-Path([string]$Value) {
    if ($Value -match '["\r\n]' -or $Value.EndsWith('\')) { throw "Cannot quote this diagnostic file path" }
    return '"{0}"' -f $Value
}

if ($Collect) {
    $expected = Get-Content -LiteralPath $IdentityFile -Raw | ConvertFrom-Json
    $target = Get-Process -Id $expected.ProcessId
    $targetHandle = $target.Handle
    Confirm-Identity $target $expected | Out-Null
    # System32 only; do not load a DLL from the checkout, registry cache, or PATH.
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class WindowsStoppedReaderDump {
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("dbghelp.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool MiniDumpWriteDump(
        IntPtr process, uint processId, IntPtr file, uint dumpType,
        IntPtr exception, IntPtr userStreams, IntPtr callback);
}
'@
    # Recheck after collector bootstrap; use the retained Process handle.
    Confirm-Identity $target $expected | Out-Null
    $file = [IO.File]::Open($DumpPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        # MiniDumpNormal (0): documented native stacks for all existing threads.
        $ok = [WindowsStoppedReaderDump]::MiniDumpWriteDump(
            $targetHandle, [uint32]$target.Id, $file.SafeFileHandle.DangerousGetHandle(),
            [uint32]0, [IntPtr]::Zero, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $ok) {
            $code = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
            throw "MiniDumpWriteDump failed with HRESULT/error $code"
        }
    } finally { $file.Dispose() }
    Write-Host "Captured all-thread native minidump for PID $($target.Id)"
    exit 0
}

if (-not $IsWindows) { throw "This diagnostic requires native Windows" }
$root = (Get-Location).Path
if (-not (Test-Path -LiteralPath (Join-Path $root "Cargo.toml"))) { throw "Run from the actual repository root" }
if (-not (Test-Path -LiteralPath (Join-Path $env:SystemRoot "System32/dbghelp.dll"))) {
    throw "Built-in Microsoft DbgHelp.dll is missing; no capture capability"
}
$pwsh = (Get-Command pwsh -ErrorAction Stop).Source
Get-Command cargo -ErrorAction Stop | Out-Null
$drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($root))
if ($drive.AvailableFreeSpace -lt 2GB) { throw "Need 2GiB free for the diagnostic executable/PDB/dump" }
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$output = (Resolve-Path -LiteralPath $OutputDirectory).Path
$case = "server::topcoat_frontend::native::raw_lifecycle::native_raw_stopped_reader_bounds_send_and_aborts_its_live_render"

# The original full Test already ran. Resolve its current fingerprinted artifact
# without fetching dependencies; do not select an arbitrary cached executable.
if (@(Get-ChildItem -LiteralPath (Join-Path $root "target/debug/deps") -Filter "lific-*.exe" -File).Count -eq 0) {
    throw "Original Windows Test produced no native test executable; no stopped-reader stack can be captured"
}
$build = @(& cargo test --locked --offline --bin lific --no-run --message-format=json)
$buildCode = $LASTEXITCODE
$build | Set-Content -LiteralPath (Join-Path $output "build.jsonl")
if ($buildCode -ne 0) { throw "Current test artifact resolution failed with $buildCode" }
$artifacts = @($build | ForEach-Object {
    if (-not $_.TrimStart().StartsWith("{")) { return }
    $entry = $_ | ConvertFrom-Json -AsHashtable
    if ($entry["reason"] -eq "compiler-artifact") {
        $target = $entry["target"]
        $profile = $entry["profile"]
        if ($null -ne $target -and $null -ne $profile -and
            $target["kind"] -contains "bin" -and $target["name"] -eq "lific" -and
            $profile["test"] -and -not [string]::IsNullOrWhiteSpace($entry["executable"])) { $entry }
    }
})
if ($artifacts.Count -ne 1) { throw "Expected exactly one Cargo native test executable" }
if ($artifacts[0]["fresh"] -ne $true) {
    throw "Cargo did not report reuse of the already-compiled current test binary; no current Windows stack evidence"
}
$binary = (Resolve-Path -LiteralPath $artifacts[0]["executable"]).Path
$stdout = Join-Path $output "test.stdout"
$stderr = Join-Path $output "test.stderr"
$test = $null
$testIdentity = $null
$collector = $null
$collectorIdentity = $null
$captureAttempted = $false
$captureSucceeded = $false
$timedOut = $false
$testCode = $null
try {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $test = Start-Process -FilePath $binary -WorkingDirectory $root -NoNewWindow -PassThru `
        -ArgumentList @($case, "--exact", "--nocapture", "--test-threads=1") `
        -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $testIdentity = Identity $test
    $identityPath = Join-Path $output "test-identity.json"
    if ($null -ne $testIdentity) {
        if (-not [StringComparer]::OrdinalIgnoreCase.Equals($testIdentity.ExecutablePath, $binary) -or
            -not $testIdentity.CommandLine.Contains($case) -or
            -not $testIdentity.CommandLine.Contains("--exact")) {
            throw "Launched process is not the exact isolated native test"
        }
        $testIdentity | ConvertTo-Json | Set-Content -LiteralPath $identityPath
    }
    while (-not $test.HasExited -and $clock.Elapsed.TotalSeconds -lt 30) {
        if (-not $captureAttempted -and (Test-Path -LiteralPath $stderr)) {
            $log = Get-Content -LiteralPath $stderr -Raw
            $sample = [regex]::Match([string]$log, 'raw stopped-reader pid=(\d+) wall=[^\r\n]+')
            if ($sample.Success) {
                $captureAttempted = $true
                $sample.Value | Set-Content -LiteralPath (Join-Path $output "trigger.txt")
                if ([int]$sample.Groups[1].Value -ne $test.Id) { throw "Watchdog PID does not match the launched test" }
                Confirm-Identity $test $testIdentity | Out-Null
                $collector = Start-Process -FilePath $pwsh -WorkingDirectory $root -NoNewWindow -PassThru `
                    -ArgumentList @("-NoLogo", "-NoProfile", "-File", (Quote-Path $PSCommandPath), "-Collect", `
                        "-IdentityFile", (Quote-Path $identityPath), "-DumpPath", (Quote-Path (Join-Path $output "native-threads.dmp"))) `
                    -RedirectStandardOutput (Join-Path $output "collector.stdout") `
                    -RedirectStandardError (Join-Path $output "collector.stderr")
                $collectorIdentity = Identity $collector
                if ($null -ne $collectorIdentity) {
                    if (-not [StringComparer]::OrdinalIgnoreCase.Equals($collectorIdentity.ExecutablePath, $pwsh) -or
                        -not $collectorIdentity.CommandLine.Contains($PSCommandPath) -or
                        -not $collectorIdentity.CommandLine.Contains("-Collect")) { throw "Unexpected collector identity" }
                    $collectorIdentity | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output "collector-identity.json")
                }
                if (-not $collector.WaitForExit(10000)) {
                    Stop-Verified $collector $collectorIdentity
                    throw "Native stack collector exceeded its independent 10-second deadline"
                }
                if ($collector.ExitCode -ne 0) { throw "Native stack collection failed with $($collector.ExitCode)" }
                $captureSucceeded = $true
            }
        }
        Start-Sleep -Milliseconds 200
        $test.Refresh()
    }
    if (-not $test.HasExited) { $timedOut = $true; throw "Original native test did not exit within bounded 30-second diagnostic supervision" }
    $test.WaitForExit()
    $testCode = $test.ExitCode
    if ($testCode -ne 0) { throw "Original native test failed with exit $testCode" }
} finally {
    $cleanupErrors = [Collections.Generic.List[string]]::new()
    $ownedProcesses = @(
        [pscustomobject]@{ Process = $collector; Identity = $collectorIdentity },
        [pscustomobject]@{ Process = $test; Identity = $testIdentity }
    )
    foreach ($owned in $ownedProcesses) {
        if ($null -ne $owned.Process) {
            try {
                $owned.Process.Refresh()
                if (-not $owned.Process.HasExited) {
                    if ($null -eq $owned.Identity) { throw "No verified identity recorded for process $($owned.Process.Id); refusing an unverified signal" }
                    Stop-Verified $owned.Process $owned.Identity
                }
            } catch { $cleanupErrors.Add($_.Exception.Message) }
        }
    }
    Copy-Item -LiteralPath $binary -Destination $output
    $pdb = [IO.Path]::ChangeExtension($binary, ".pdb")
    if (Test-Path -LiteralPath $pdb) { Copy-Item -LiteralPath $pdb -Destination $output }
    [pscustomobject]@{
        Case = $case
        TestExitCode = $testCode
        SupervisionTimedOut = $timedOut
        CaptureAttempted = $captureAttempted
        CaptureSucceeded = $captureSucceeded
        BinarySHA256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash
        MatchingPdbPresent = (Test-Path -LiteralPath $pdb)
        CleanupErrors = @($cleanupErrors.ToArray())
        DiagnosticOnly = $true
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output "result.json")
    foreach ($path in @($stdout, $stderr)) {
        if (Test-Path -LiteralPath $path) { Get-Content -LiteralPath $path | Out-Host }
    }
    if ($cleanupErrors.Count -ne 0) { throw "Diagnostic cleanup failed: $($cleanupErrors -join '; ')" }
}
