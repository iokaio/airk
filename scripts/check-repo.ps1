# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$airkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $airkRoot
try {
    $airkFiles = @(git ls-files --cached --others --exclude-standard)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate repository files' }
    if ($airkFiles | Where-Object { $_ -match '(^|/)(temp|\.airk)(/|$)' }) { throw 'Private reference/index material is eligible for Git' }
    if ((Get-FileHash AGENTS.md).Hash -ne (Get-FileHash CLAUDE.md).Hash) { throw 'AGENTS.md and CLAUDE.md differ' }
    # cargo-vet's locked check requires its canonical generated-data headers;
    # its formatter removes an SPDX prefix. Keep this exception to these two
    # exact data files. All source and other TOML retain the SPDX requirement.
    $airkGeneratedHeaders = @{
        'supply-chain/config.toml' = '# cargo-vet config file'
        'supply-chain/audits.toml' = '# cargo-vet audits file'
    }
    foreach ($airkFile in $airkFiles) {
        if (-not (Test-Path -LiteralPath $airkFile -PathType Leaf)) { continue }
        if ($airkFile -match '\.(rs|toml|py|ps1|ya?ml|ts|tsx|js|cjs|mjs|css)$') {
            $airkFirstLine = Get-Content -LiteralPath $airkFile -TotalCount 1
            if ($airkGeneratedHeaders.ContainsKey($airkFile)) {
                $airkDataHeader = Get-Content -LiteralPath $airkFile -TotalCount 2 | Where-Object { $_ -ne '' } | Select-Object -First 1
                if ($airkDataHeader -cne $airkGeneratedHeaders[$airkFile]) { throw "Unexpected generated-data header: $airkFile" }
            } elseif ($airkFirstLine -notmatch 'SPDX-License-Identifier: Apache-2.0') { throw "Missing SPDX: $airkFile" }
        }
        if ($airkFile -like '*.md') {
            $airkText = Get-Content -LiteralPath $airkFile -Raw
            foreach ($airkMatch in [regex]::Matches($airkText, '\[[^\]]*\]\(([^)]+)\)')) {
                $airkLink = $airkMatch.Groups[1].Value
                if ($airkLink -match '^(https?://|mailto:|#)') { continue }
                $airkTarget = ($airkLink -split '#', 2)[0]
                if (-not $airkTarget) { continue }
                $airkResolved = Join-Path (Split-Path (Join-Path $airkRoot $airkFile) -Parent) $airkTarget
                if (-not (Test-Path -LiteralPath $airkResolved)) { throw "Broken link in ${airkFile}: $airkLink" }
            }
        }
    }
    # One resident airk per user/machine: no production self-spawn outside the reviewed roster.
    & python (Join-Path $PSScriptRoot 'check-self-spawn.py')
    if ($LASTEXITCODE -ne 0) { throw 'Production Rust starts its own executable outside the reviewed roster' }
    # Generated contract types (split plan U47) must match their checked-in schema.
    & python (Join-Path $PSScriptRoot 'dev/contract_types.py') i2 --check
    if ($LASTEXITCODE -ne 0) { throw 'Generated I2 TypeScript differs from its schema' }
    git check-ignore temp/README.md | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'temp/ is not ignored' }
    Write-Output "Repository checks passed for $($airkFiles.Count) public files."
} finally { Pop-Location }
