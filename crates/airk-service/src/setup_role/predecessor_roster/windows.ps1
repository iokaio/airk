# SPDX-License-Identifier: Apache-2.0
# Read-only native discovery. AIRK-like names are candidates, never ownership.
$ErrorActionPreference = 'Stop'
$airkComplete = $true
$airkTasks = New-Object 'System.Collections.Generic.List[object]'
$airkUninstall = New-Object 'System.Collections.Generic.List[string]'
$airkPath = New-Object 'System.Collections.Generic.List[string]'
try {
    $airkScheduler = New-Object -ComObject 'Schedule.Service'
    $airkScheduler.Connect()
    $airkRoot = $airkScheduler.GetFolder('\')
    $airkCount = 0
    foreach ($airkTask in $airkRoot.GetTasks(1)) {
        $airkCount++
        if ($airkCount -gt 256) { $airkComplete = $false; break }
        if ($airkTask.Name -match '^(airksvc-|airk-)[0-9a-fA-F]{32}$') {
            $airkExecutables = New-Object 'System.Collections.Generic.List[string]'
            foreach ($airkAction in $airkTask.Definition.Actions) {
                if ($airkAction.Type -eq 0) { $airkExecutables.Add([string]$airkAction.Path) }
            }
            # Arguments may contain private values; only command locations are
            # observation data. A location does not prove installation ownership.
            $airkTasks.Add(@{item=[string]$airkTask.Path;executables=@($airkExecutables.ToArray())})
        }
    }
} catch { $airkComplete = $false }
try {
    $airkBase = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Uninstall')
    if ($null -ne $airkBase) {
        try {
            $airkNames = $airkBase.GetSubKeyNames()
            if ($airkNames.Length -gt 256) { $airkComplete = $false }
            foreach ($airkName in ($airkNames | Select-Object -First 256)) {
                $airkKey = $airkBase.OpenSubKey($airkName)
                if ($null -eq $airkKey) { $airkComplete = $false; continue }
                try {
                    $airkDisplay = [string]$airkKey.GetValue('DisplayName', '')
                    if ($airkDisplay -match '^AIRK(?:\s|$)') { $airkUninstall.Add('HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $airkName) }
                } finally { $airkKey.Dispose() }
            }
        } finally { $airkBase.Dispose() }
    }
    $airkEnvironment = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    if ($null -ne $airkEnvironment) {
        try {
            $airkValue = [string]$airkEnvironment.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            if ($airkValue.Length -gt 32768) { $airkComplete = $false }
            else {
                foreach ($airkEntry in $airkValue.Split(';')) {
                    if ($airkEntry -match '(?i)(?:^|[\/])AIRK[\/](?:standalone[\/]runtime|launcher(?:-preview)?)[\/]?$|[\/]iokaio\.airk[\/](?:service-cli|cli)[\/]bin[\/]?$') {
                        $airkPath.Add($airkEntry)
                    }
                }
            }
        } finally { $airkEnvironment.Dispose() }
    }
} catch { $airkComplete = $false }
@{ tasks=@($airkTasks.ToArray()); uninstall=@($airkUninstall.ToArray()); path=@($airkPath.ToArray()); complete=$airkComplete } | ConvertTo-Json -Depth 5 -Compress
