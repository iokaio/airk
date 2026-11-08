# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.4
# PowerShell 7.4+ on Windows: read the local JSON API from protected native metadata.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Connection,
    [string]$Tool,
    [string]$Arguments = '{}',
    [string]$ArgumentsFile
)
$ErrorActionPreference = 'Stop'
try {
    if (-not $IsWindows) { throw 'Use the Node/Python example on Unix; this protected-file reader requires Windows.' }
    Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Net.Security;
using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using Microsoft.Win32.SafeHandles;
public static class AirkLocalHttpsExample {
    [StructLayout(LayoutKind.Sequential)] struct Information {
        public uint Attributes; public System.Runtime.InteropServices.ComTypes.FILETIME Creation, Access, Write;
        public uint Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
    }
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetFileInformationByHandle(SafeFileHandle handle, out Information information);
    public static byte[] ReadPrivate(string path, int maximum) {
        using (var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read)) {
            Information before, after;
            if (!GetFileInformationByHandle(file.SafeFileHandle, out before) || before.Links != 1 || (before.Attributes & 0x410) != 0 || before.SizeHigh != 0 || before.SizeLow > maximum) throw new IOException("Unsafe protected file");
            byte[] bytes = new byte[maximum+1]; int used=0, read;
            while (used < bytes.Length && (read=file.Read(bytes,used,bytes.Length-used)) != 0) used+=read;
            if (!GetFileInformationByHandle(file.SafeFileHandle,out after) || before.Volume!=after.Volume || before.IndexHigh!=after.IndexHigh || before.IndexLow!=after.IndexLow || before.SizeHigh!=after.SizeHigh || before.SizeLow!=after.SizeLow || before.Write.dwHighDateTime!=after.Write.dwHighDateTime || before.Write.dwLowDateTime!=after.Write.dwLowDateTime || after.Links!=1 || used!=before.SizeLow) throw new IOException("Protected file changed");
            Array.Resize(ref bytes,used); return bytes;
        }
    }
    public static string CaHash(byte[] pem) {
        using (var ca=X509Certificate2.CreateFromPem(Encoding.ASCII.GetString(pem))) {
            bool isCa=false; foreach(var extension in ca.Extensions) if(extension is X509BasicConstraintsExtension basic) isCa=basic.CertificateAuthority;
            if(!isCa) throw new IOException("Expected CA certificate");
            return Convert.ToHexString(SHA256.HashData(ca.RawData)).ToLowerInvariant();
        }
    }
    public sealed class Reply { public int Status; public string Body; }
    public static Reply Query(int port, byte[] pem, string token, string tool, string arguments) {
        using(var ca=X509Certificate2.CreateFromPem(Encoding.ASCII.GetString(pem)))
        using(var handler=new HttpClientHandler { UseProxy=false, AllowAutoRedirect=false, AutomaticDecompression=DecompressionMethods.None, MaxResponseHeadersLength=64 }) {
            handler.ServerCertificateCustomValidationCallback=(request,certificate,chain,errors)=> {
                // Preserve the platform's normal trusted roots. The second path
                // adds this CA; it never waives hostname, validity or EKU checks.
                if(errors==SslPolicyErrors.None) return true;
                if(errors!=SslPolicyErrors.RemoteCertificateChainErrors || certificate==null || chain==null) return false;
                using(var local=new X509Chain()) {
                    local.ChainPolicy.TrustMode=X509ChainTrustMode.CustomRootTrust;
                    local.ChainPolicy.CustomTrustStore.Add(ca);
                    local.ChainPolicy.VerificationFlags=X509VerificationFlags.NoFlag;
                    local.ChainPolicy.RevocationMode=chain.ChainPolicy.RevocationMode;
                    local.ChainPolicy.RevocationFlag=chain.ChainPolicy.RevocationFlag;
                    local.ChainPolicy.DisableCertificateDownloads=true;
                    local.ChainPolicy.ApplicationPolicy.Add(new Oid("1.3.6.1.5.5.7.3.1"));
                    return local.Build(certificate);
                }
            };
            using(var client=new HttpClient(handler))
            using(var deadline=new CancellationTokenSource(125000)) {
                client.Timeout=Timeout.InfiniteTimeSpan;
                bool readTool=!String.IsNullOrEmpty(tool);
                string route="/api/v1/tools"+(readTool?"/"+tool:"");
                using(var request=new HttpRequestMessage(readTool?HttpMethod.Post:HttpMethod.Get,new Uri("https://127.0.0.1:"+port+route))) {
                    request.Headers.Authorization=new System.Net.Http.Headers.AuthenticationHeaderValue("Bearer",token);
                    if(readTool) request.Content=new StringContent(arguments,Encoding.UTF8,"application/json");
                    using(var response=client.Send(request,HttpCompletionOption.ResponseHeadersRead,deadline.Token))
                    using(var stream=response.Content.ReadAsStream(deadline.Token))
                    using(var output=new MemoryStream()) {
                        byte[] buffer=new byte[16384]; int read;
                        while((read=stream.ReadAsync(buffer,0,buffer.Length,deadline.Token).GetAwaiter().GetResult())!=0) {
                            if(output.Length+read>1048576) throw new IOException("Response exceeds transport budget");
                            output.Write(buffer,0,read);
                        }
                        return new Reply { Status=(int)response.StatusCode, Body=new UTF8Encoding(false,true).GetString(output.ToArray()) };
                    }
                }
            }
        }
    }
}
'@
    function Read-NativePrivate([string]$Filename, [int]$Maximum) {
        if (-not [IO.Path]::IsPathFullyQualified($Filename)) { throw 'Expected absolute native reference' }
        $item = Get-Item -LiteralPath $Filename -Force
        if ($item -isnot [IO.FileInfo] -or $item.LinkType -or $item.Length -gt $Maximum) { throw 'Unsafe protected file' }
        $cursor = $item.Directory
        while ($null -ne $cursor) {
            if ($cursor.LinkTarget -or ($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Redirected protected reference' }
            $cursor = $cursor.Parent
        }
        $acl = Get-Acl -LiteralPath $Filename
        $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid) { throw 'Foreign protected file owner' }
        foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
            if ($rule.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and $rule.IdentityReference.Value -notin @($sid, 'S-1-5-18', 'S-1-5-32-544')) { throw 'Foreign protected file access' }
        }
        $bytes = [AirkLocalHttpsExample]::ReadPrivate($item.FullName, $Maximum)
        $after = Get-Item -LiteralPath $Filename -Force
        if ($after.LinkType -or $after.Length -ne $item.Length -or $after.LastWriteTimeUtc -ne $item.LastWriteTimeUtc) { throw 'Protected file changed' }
        return ,$bytes
    }
    $info = [Text.Encoding]::UTF8.GetString((Read-NativePrivate $Connection 61440)) | ConvertFrom-Json -AsHashtable -Depth 100
    if ($info.schema -ne 1 -or $info.endpoint.schema -ne 1 -or $info.endpoint.state -cne 'ready') { throw 'Obtain current ready native connection' }
    if ($info.endpoint.url -cnotmatch '^https://127\.0\.0\.1:([1-9][0-9]{0,4})/mcp$') { throw 'Invalid loopback endpoint' }
    $port = [int]$Matches[1]
    if ($port -gt 65535) { throw 'Invalid loopback port' }
    $pem = Read-NativePrivate $info.ca_certificate_file 8192
    if (([regex]::Matches([Text.Encoding]::ASCII.GetString($pem), '-----BEGIN CERTIFICATE-----')).Count -ne 1 -or [AirkLocalHttpsExample]::CaHash($pem) -cne $info.endpoint.ca_sha256) { throw 'Native CA identity changed' }
    $token = [Text.Encoding]::ASCII.GetString((Read-NativePrivate $info.credential_file 103))
    if ($token -cnotmatch '^airk1\.([0-9a-f]{32})\.[0-9a-f]{64}$' -or $Matches[1] -cne $info.connection_id) { throw 'Invalid native credential identity' }
    $selectedTool = $null
    if ($ArgumentsFile) {
        if ($PSBoundParameters.ContainsKey('Arguments')) { throw 'Choose one argument input' }
        $Arguments = [Text.Encoding]::UTF8.GetString((Read-NativePrivate $ArgumentsFile 65536))
    }
    if ($PSBoundParameters.ContainsKey('Tool')) {
        if ($Tool -cnotmatch '^[A-Za-z0-9_]{1,64}$' -or ($Arguments | ConvertFrom-Json -AsHashtable -Depth 100) -isnot [Collections.IDictionary] -or [Text.Encoding]::UTF8.GetByteCount($Arguments) -gt 65536) { throw 'Use an advertised tool and bounded JSON argument object' }
        $selectedTool = $Tool
    }
    $reply = [AirkLocalHttpsExample]::Query($port, $pem, $token, $selectedTool, $Arguments)
    $result = $reply.Body | ConvertFrom-Json -AsHashtable -Depth 100
    [Console]::Out.WriteLine($reply.Body) # Complete raw JSON, including refusals.
    if ($reply.Status -ne 200 -or $result.isError -eq $true) { exit 1 }
    exit 0
} catch {
    [Console]::Error.WriteLine('AIRK local HTTPS read failed; obtain current verified connection metadata and inspect native status.')
    exit 1
}
