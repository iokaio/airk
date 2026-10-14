# SPDX-License-Identifier: Apache-2.0
param(
    [Parameter(Mandatory=$true)][string]$Binary,
    [string]$Thumbprint,
    [string]$Publisher,
    [string]$IdentityEku
)
$ErrorActionPreference = 'Stop'
if ($Thumbprint -and $Thumbprint -notmatch '^[a-fA-F0-9]{40}$') { throw 'Invalid signing certificate thumbprint' }
if (-not $Thumbprint -and (-not $Publisher -or -not $IdentityEku)) { throw 'Expected signing identity is required' }
$airkSignature = Get-AuthenticodeSignature -LiteralPath $Binary
if ($airkSignature.Status -ne 'Valid' -or ($Thumbprint -and $airkSignature.SignerCertificate.Thumbprint -ne $Thumbprint) -or -not $airkSignature.TimeStamperCertificate) {
    throw 'Native signature is invalid, untimestamped, or belongs to another certificate'
}
if ($Publisher -and $airkSignature.SignerCertificate.Subject -cne $Publisher) { throw 'Unexpected signing publisher' }
if ($IdentityEku) {
    $airkEkus = @($airkSignature.SignerCertificate.Extensions |
        Where-Object { $_ -is [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension] } |
        ForEach-Object { $_.EnhancedKeyUsages.Value })
    if ($IdentityEku -notin $airkEkus) { throw 'Unexpected signing identity EKU' }
}
@{ thumbprint = $airkSignature.SignerCertificate.Thumbprint; timestamp_present = $true } | ConvertTo-Json -Compress
