# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$airkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $airkRoot
try {
    $airkFiles = @(git ls-files --cached --others --exclude-standard)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate repository files' }
    if ($airkFiles | Where-Object { $_ -match '(^|/)(temp|\.airk)(/|$)' }) { throw 'Private reference/index material is eligible for Git' }
    if ((Get-FileHash AGENTS.md).Hash -ne (Get-FileHash CLAUDE.md).Hash) { throw 'AGENTS.md and CLAUDE.md differ' }
    foreach ($airkFile in $airkFiles) {
        if (-not (Test-Path -LiteralPath $airkFile -PathType Leaf)) { continue }
        if ($airkFile -match '\.(rs|toml|py|ps1|ya?ml|ts|tsx|js|cjs|mjs|css)$') {
            $airkFirstLine = Get-Content -LiteralPath $airkFile -TotalCount 1
            if ($airkFirstLine -notmatch 'SPDX-License-Identifier: Apache-2.0') { throw "Missing SPDX: $airkFile" }
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
    git check-ignore temp/README.md | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'temp/ is not ignored' }
    Write-Output "Repository checks passed for $($airkFiles.Count) public files."
} finally { Pop-Location }
