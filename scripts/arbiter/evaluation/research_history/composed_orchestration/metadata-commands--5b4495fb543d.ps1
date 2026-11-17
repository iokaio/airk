# SPDX-License-Identifier: Apache-2.0
# Preparation only: execute only after root releases genuine source-sealed build/test artifacts.
param([Parameter(Mandatory)][string]$NativeBuildReceipt,
      [Parameter(Mandatory)][string]$RetrievalTestExe)
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$true
if([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne 'S-1-5-21-3678370870-3858642417-826968105-1002'){throw 'Execution SID differs'}
python -B 'D:\code\Github\airk\scripts\dev\prepare-arbiter-metadata.py' '--repository-root' 'D:\code\Github\airk' '--catalog' 'D:\code\Github\airk\crates\airk-retrieval\src\arbiter\release-catalog.json' '--acquisition-receipt' 'D:\code\Github\airk\artifacts\v190\final-plan\lifecycle-acquisition-r1\receipt.json' '--asset-census' 'D:\code\Github\airk\artifacts\v190\final-plan\lifecycle-acquisition-r1\saved-asset-census-r1.json' '--native-build-receipt' $NativeBuildReceipt '--asset-root' 'D:\code\Github\airk\artifacts\advanced-split\models\actual-composed-service-r2\final-plan-acquired-assets-20261116T071657Z' '--request' 'D:\code\Github\airk\artifacts\advanced-split\models\actual-composed-service-r2\final-plan-current-policy-preparation-20261116T085632Z\metadata-request.private.json' '--output' 'D:\code\Github\airk\artifacts\advanced-split\models\actual-composed-service-r2\final-plan-current-policy-preparation-20261116T085632Z\metadata'
$env:AIRK_CONTEXT_METADATA_REGEN_INPUT='D:\code\Github\airk\artifacts\advanced-split\models\actual-composed-service-r2\final-plan-current-policy-preparation-20261116T085632Z\metadata-request.private.json'
$env:AIRK_CONTEXT_METADATA_REGEN_INPUT_SHA256=(Get-FileHash -LiteralPath $env:AIRK_CONTEXT_METADATA_REGEN_INPUT -Algorithm SHA256).Hash.ToLowerInvariant()
& $RetrievalTestExe --ignored --exact 'arbiter::runtime::qualification::regeneration::emit_current_policy_profiles_from_verified_original_assets' --nocapture --test-threads=1
