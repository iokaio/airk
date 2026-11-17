# SPDX-License-Identifier: Apache-2.0
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('encoders', 'retrieval', 'capture', 'transcripts', 'optional-parity', 'resource', 'grade-retrieval', 'label-calibration')]
    [string]$Step,
    [ValidateSet('arbiter-small-intern-08b', 'arbiter-small-strands-2b', 'arbiter-large-intern-4b', 'arbiter-large-kev-4b')]
    [string]$ResourceModel = 'arbiter-small-intern-08b'
)
$ErrorActionPreference = 'Stop'
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
Set-Location -LiteralPath $RepoRoot
$ModelRoot = Join-Path $RepoRoot 'artifacts/advanced-split/models'
$EvidenceRoot = $PSScriptRoot
$TargetRoot = Join-Path $RepoRoot 'artifacts/v190/p10-target'
$Python = Join-Path $ModelRoot 'training-env/venv/Scripts/python.exe'
$EncoderInput = Join-Path $ModelRoot 'native-encoder-qualification-input-r1.json'
$RetrievalInput = Join-Path $ModelRoot 'native-encoder-205-input-r1.json'
$Profiles = Join-Path $ModelRoot 'native-weight8-cpu8-qualification-profiles-r1/inputs.json'
$Rubric = Join-Path $ModelRoot 'human-goal-rubric-r4/rubric.private.json'
$Fixture = Join-Path $ModelRoot 'native-human-fixtures-r2'
$env:AIRK_NATIVE_ENCODER_INPUT = $EncoderInput
$env:AIRK_ARBITER_QUALIFICATION_INPUT = $Profiles
function Set-NewOutput([string]$Name, [string]$Path) {
    if (Test-Path -LiteralPath $Path) { throw "Retain existing receipt and select a fresh evidence directory before another run: $Path" }
    [Environment]::SetEnvironmentVariable($Name, $Path, 'Process')
}
# Root remains the sole Cargo owner. Inherit its authenticated static ORT1.30
# provisioning environment unchanged. Each selected ignored test must report1.
switch ($Step) {
    'encoders' {
        Set-NewOutput AIRK_NATIVE_ENCODER_OUTPUT (Join-Path $EvidenceRoot 'native-encoders.json')
        cargo test -p airk-kernel --lib --features 'local-inference,test-support' --locked --timings --target-dir $TargetRoot inference_owner::evaluation_tests::six_encoder_families_match_independent_native_bucket_oracles_without_promoting_baselines -- --ignored --exact --nocapture --test-threads=1
    }
    'retrieval' {
        $env:AIRK_NATIVE_RETRIEVAL_INPUT = $RetrievalInput
        Set-NewOutput AIRK_NATIVE_RETRIEVAL_OUTPUT (Join-Path $EvidenceRoot 'native-retrieval-205.json')
        cargo test -p airk-kernel --lib --features 'local-inference,test-support' --locked --timings --target-dir $TargetRoot inference_owner::evaluation_tests::native_205_questions_use_the_same_current_kernel_collection_chunking_and_diskann_retrieval -- --ignored --exact --nocapture --test-threads=1
    }
    'capture' {
        $env:AIRK_ARBITER_CALIBRATION_CAPTURE_INPUT = Join-Path $ModelRoot 'native-calibration-input-r1.json'
        Set-NewOutput AIRK_ARBITER_CALIBRATION_CAPTURE_OUTPUT (Join-Path $EvidenceRoot 'native-calibration-capture.json')
        cargo test -p airk-mcp --lib --features 'airk-kernel/local-inference,airk-kernel/test-support' --locked --timings --target-dir $TargetRoot arbiter_calibration_capture::calibration_choices_retain_actual_before_and_counterfactual_canonical_read_spans -- --ignored --exact --nocapture --test-threads=1
    }
    'transcripts' {
        $env:AIRK_ARBITER_CALIBRATION_TRANSCRIPT_INPUT = Join-Path $EvidenceRoot 'native-calibration-capture.json'
        $env:AIRK_ARBITER_CALIBRATION_TRANSCRIPT_PROFILES = $Profiles
        Set-NewOutput AIRK_ARBITER_CALIBRATION_TRANSCRIPT_OUTPUT (Join-Path $EvidenceRoot 'native-calibration-transcripts.json')
        cargo test -p airk-retrieval --lib --features 'local-inference,test-support' --locked --timings --target-dir $TargetRoot arbiter::tasks::tests::native_calibration_transcripts_bind_exact_task_policy_aliases_and_compiler_inputs -- --ignored --exact --nocapture --test-threads=1
    }
    'optional-parity' {
        Set-NewOutput AIRK_ARBITER_QUALIFICATION_OUTPUT (Join-Path $EvidenceRoot 'native-optional-parity.json')
        cargo test -p airk-inference --lib --features 'local-inference,test-support' --locked --timings --target-dir $TargetRoot inference_engine::pool::arbiter::qualification::four_complete_trained_models_match_independent_compilers_and_raw_scores -- --ignored --exact --nocapture --test-threads=1
    }
    'resource' {
        $env:AIRK_ARBITER_RESOURCE_MODEL = $ResourceModel
        Set-NewOutput AIRK_ARBITER_RESOURCE_OUTPUT (Join-Path $EvidenceRoot ($ResourceModel + '-native-resource.json'))
        cargo test -p airk-inference --lib --features 'local-inference,test-support' --locked --timings --target-dir $TargetRoot inference_engine::pool::arbiter::qualification::complete_trained_model_maximum_context_has_twenty_native_warm_measurements -- --ignored --exact --nocapture --test-threads=1
    }
    'grade-retrieval' {
        & $Python tools/retrieval-train/grade_native_retrieval.py --native (Join-Path $EvidenceRoot 'native-retrieval-205.json') --questions $RetrievalInput --encoders $EncoderInput --rubric $Rubric --fixture $Fixture --output (Join-Path $EvidenceRoot 'retrieval-human-grading')
    }
    'label-calibration' {
        & $Python tools/retrieval-train/calibration_labels.py --capture (Join-Path $EvidenceRoot 'native-calibration-capture.json') --native-input (Join-Path $ModelRoot 'native-calibration-input-r1.json') --rubric $Rubric --fixture $Fixture --output (Join-Path $EvidenceRoot 'calibration-labels')
    }
}
if ($LASTEXITCODE -ne 0) { throw "Selected native/evaluation step failed: $Step ($LASTEXITCODE); retain diagnostics and stop the batch." }
