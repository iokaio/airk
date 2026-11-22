# SPDX-License-Identifier: Apache-2.0
# Run one executable to completion and report its exit code, elapsed time and
# peak memory (peak working set and peak private/pagefile bytes) read from its
# own process handle. Windows only. See README.md in this folder.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string[]]$Arguments = @()
)
$ErrorActionPreference = 'Stop'
if (-not ('AirkPeakMemory.Native' -as [type])) {
    Add-Type -Namespace AirkPeakMemory -Name Native -MemberDefinition @"
[StructLayout(LayoutKind.Sequential)] public struct Counters { public uint cb; public uint PageFaultCount; public UIntPtr PeakWorkingSetSize; public UIntPtr WorkingSetSize; public UIntPtr QuotaPeakPagedPoolUsage; public UIntPtr QuotaPagedPoolUsage; public UIntPtr QuotaPeakNonPagedPoolUsage; public UIntPtr QuotaNonPagedPoolUsage; public UIntPtr PagefileUsage; public UIntPtr PeakPagefileUsage; }
[DllImport("psapi.dll", SetLastError=true)] public static extern bool GetProcessMemoryInfo(IntPtr process, out Counters counters, uint cb);
"@
}
$start = [Diagnostics.ProcessStartInfo]::new($Exe)
foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
$start.UseShellExecute = $false
$start.RedirectStandardOutput = $true
$clock = [Diagnostics.Stopwatch]::StartNew()
$process = [Diagnostics.Process]::Start($start)
$handle = $process.Handle   # keep the handle so counters survive exit
$output = $process.StandardOutput.ReadToEnd()
$process.WaitForExit()
$counters = New-Object AirkPeakMemory.Native+Counters
$counters.cb = [Runtime.InteropServices.Marshal]::SizeOf($counters)
if (-not [AirkPeakMemory.Native]::GetProcessMemoryInfo($handle, [ref]$counters, $counters.cb)) {
    throw "GetProcessMemoryInfo failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
}
$output
"# exit=$($process.ExitCode) elapsed_s=$([math]::Round($clock.Elapsed.TotalSeconds, 2)) peak_working_set=$($counters.PeakWorkingSetSize) peak_private=$($counters.PeakPagefileUsage)"
exit $process.ExitCode
