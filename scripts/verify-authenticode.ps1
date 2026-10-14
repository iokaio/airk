# SPDX-License-Identifier: Apache-2.0
param([Parameter(Mandatory=$true)][string]$Binary, [Parameter(Mandatory=$true)][string]$Thumbprint)
$ErrorActionPreference = 'Stop'
if ($Thumbprint -notmatch '^[a-fA-F0-9]{40}$') { throw 'Invalid signing certificate thumbprint' }
$airkSignature = Get-AuthenticodeSignature -LiteralPath $Binary
if ($airkSignature.Status -ne 'Valid' -or $airkSignature.SignerCertificate.Thumbprint -ne $Thumbprint -or -not $airkSignature.TimeStamperCertificate) {
    throw 'Native signature is invalid, untimestamped, or belongs to another certificate'
}
@{ thumbprint = $airkSignature.SignerCertificate.Thumbprint; timestamp_present = $true } | ConvertTo-Json -Compress
