# SPDX-License-Identifier: Apache-2.0
param([switch]$SkipChecks)
if (-not $SkipChecks) { throw 'SkipChecks was not forwarded' }
throw 'Fictional failed build'
