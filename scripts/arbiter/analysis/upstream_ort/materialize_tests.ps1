# SPDX-License-Identifier: Apache-2.0
# Pure captured-byte controls; no bundle materialization or filesystem writes.
$ErrorActionPreference='Stop'
$tokens=$null;$parseErrors=$null
$tree=[System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'materialize.ps1'),[ref]$tokens,[ref]$parseErrors)
if($parseErrors.Count){throw 'Materializer syntax errors'}
$definition=$tree.Find({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Assert-AirkCapturedBundleBytes'},$true)
if($null -eq $definition){throw 'Missing captured-byte owner'}
. ([scriptblock]::Create($definition.Extent.Text))
$bytes=[System.Text.Encoding]::UTF8.GetBytes('abc')
$expected='ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
Assert-AirkCapturedBundleBytes -Bytes $bytes -ExpectedSha256 $expected
$refused=$false
try{Assert-AirkCapturedBundleBytes -Bytes ([System.Text.Encoding]::UTF8.GetBytes('abd')) -ExpectedSha256 $expected}catch{$refused=$true}
if(-not $refused){throw 'Replacement bytes accepted'}
$refused=$false
try{Assert-AirkCapturedBundleBytes -Bytes ([byte[]]::new(0)) -ExpectedSha256 $expected}catch{$refused=$true}
if(-not $refused){throw 'Empty bundle accepted'}
'3 captured-byte controls passed'
