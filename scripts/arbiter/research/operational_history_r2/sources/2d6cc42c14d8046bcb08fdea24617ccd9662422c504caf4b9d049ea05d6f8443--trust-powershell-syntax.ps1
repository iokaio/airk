# SPDX-License-Identifier: Apache-2.0
$tokens=$null;$errors=$null;[System.Management.Automation.Language.Parser]::ParseInput(@'
& { param($Action,$Expected,$File)
$ErrorActionPreference='Stop'; $s=New-Object System.Security.Cryptography.X509Certificates.X509Store('Root','CurrentUser');
$s.Open($(if($Action -eq 'inspect'){[System.Security.Cryptography.X509Certificates.OpenFlags]::ReadOnly}else{[System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite}));
try {
 $sha=[System.Security.Cryptography.SHA256]::Create();
 $found=@($s.Certificates | Where-Object { ([BitConverter]::ToString($sha.ComputeHash($_.RawData))).Replace('-','').ToLowerInvariant() -ceq $Expected });
 if($found.Count -gt 1){throw 'ambiguous issuer entries'}
 if($Action -eq 'install') {
  if($found.Count -ne 0){throw 'issuer appeared before installation'}
  $c=[System.Security.Cryptography.X509Certificates.X509Certificate2]::new($File);
  if(([BitConverter]::ToString($sha.ComputeHash($c.RawData))).Replace('-','').ToLowerInvariant() -cne $Expected){throw 'issuer file changed'}
  $s.Add($c);
 } elseif($Action -eq 'remove') {
  if($found.Count -ne 1){throw 'issuer disappeared before removal'}
  $s.Remove($found[0]);
 } elseif($Action -ne 'inspect'){throw 'invalid trust action'}
 $found=@($s.Certificates | Where-Object { ([BitConverter]::ToString($sha.ComputeHash($_.RawData))).Replace('-','').ToLowerInvariant() -ceq $Expected });
 if($found.Count -gt 1){throw 'ambiguous issuer entries'}
 @{present=($found.Count -eq 1);trusted=($found.Count -eq 1)} | ConvertTo-Json -Compress
} finally {$s.Close()}
} 'inspect' 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' 'C:\fictional\ca.pem'
'@,[ref]$tokens,[ref]$errors)|Out-Null
if($errors.Count){$errors|ForEach-Object{$_.Message};exit 1};'Trust command PowerShell parse passed; no trust command executed.'
