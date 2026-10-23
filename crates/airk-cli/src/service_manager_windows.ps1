# SPDX-License-Identifier: Apache-2.0
# Fixed adapter code. The task identity is data; expected XML arrives on bounded stdin.
$ErrorActionPreference = 'Stop'
$airkInput = New-Object System.IO.StreamReader([Console]::OpenStandardInput(), [Text.Encoding]::UTF8)
$airkExpectedXml = $airkInput.ReadToEnd()
$airkScheduler = New-Object -ComObject 'Schedule.Service'
$airkScheduler.Connect()
$airkFolder = $airkScheduler.GetFolder('\')
$airkTask = $null
try {
    $airkTask = $airkFolder.GetTask($env:AIRK_SERVICE_ID)
} catch {
    $airkFailure = $_.Exception
    while ($null -ne $airkFailure.InnerException) { $airkFailure = $airkFailure.InnerException }
    if ($airkFailure.HResult -ne -2147024894) { throw }
}
$airkResult = @{ loaded = $false; running = $false; definition_matches = $null }
if ($null -ne $airkTask) {
    $airkResult.loaded = $true
    $airkResult.running = ($airkTask.State -eq 4)
    $airkResult.definition_matches = $false
    if (-not [String]::IsNullOrEmpty($airkExpectedXml)) {
        # Ask the OS schema implementation to expand the same defaults on both
        # definitions. Registration-generated metadata does not identify execution.
        $airkExpectedTask = $airkScheduler.NewTask(0)
        $airkExpectedTask.XmlText = $airkExpectedXml
        function Convert-AirkDefinition([String] $airkXml) {
            $airkDocument = New-Object System.Xml.XmlDocument
            $airkDocument.PreserveWhitespace = $false
            $airkDocument.XmlResolver = $null
            $airkDocument.LoadXml($airkXml)
            $airkNamespaces = New-Object System.Xml.XmlNamespaceManager($airkDocument.NameTable)
            $airkNamespaces.AddNamespace('t', 'http://schemas.microsoft.com/windows/2004/02/mit/task')
            foreach ($airkName in @('URI', 'Date', 'Author', 'Version', 'Source')) {
                foreach ($airkNode in @($airkDocument.SelectNodes("/t:Task/t:RegistrationInfo/t:$airkName", $airkNamespaces))) {
                    [void] $airkNode.ParentNode.RemoveChild($airkNode)
                }
            }
            return $airkDocument.OuterXml
        }
        $airkResult.definition_matches = ((Convert-AirkDefinition $airkExpectedTask.XmlText) -ceq (Convert-AirkDefinition $airkTask.Definition.XmlText))
    }
}
$airkBytes = (New-Object System.Text.UTF8Encoding($false)).GetBytes(($airkResult | ConvertTo-Json -Compress))
$airkOutput = [Console]::OpenStandardOutput()
$airkOutput.Write($airkBytes, 0, $airkBytes.Length)
$airkOutput.Flush()
