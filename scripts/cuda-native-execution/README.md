# CUDA native execution tooling

`../cuda-native-execution.py` owns the bounded native package-metadata launch.
Its [contract](CONTRACT.md) defines custody and evidence requirements.
The [maintained helpers](helpers/README.md) contain byte-identical process,
identity and settlement primitives used by current CUDA and FP16 tooling.
Generated plans, runtime images, model inputs and receipts remain private artifacts.

`windows_notice_sources.py` checks the original CU15 build archives, installed toolkit notices and cuDNN acquisition and extraction receipts. In the verified Windows shell, run `python -I -B -S scripts/cuda-native-execution/windows_notice_sources.py` to obtain the filename and hash inventory. Pass `--stage <absolute-empty-campaign-directory>` after checking its listed output paths to copy the same original bytes. The private report retains the cuDNN archive and metadata license differences and the publisher's recorded distribution limitations. The native product owner separately consumes the reviewed notice roster and build evidence.
Run `stage_windows_product_sources.ps1 -HumanSid <verified SID>` in the checked Windows shell to prepare the clean raw runtime. It verifies the original 13 DLLs and 49 notice files, checks allocated disk space, writes new independent files and records output hashes and owner checks in the private staging receipt. `windows_product_sources.py` prints the private source request for native manifest observation. The staging receipt alone supplies no public build provenance or qualification.

Run `windows_product_observation.py --help` for the metadata observation launch. Pass the original CU15 assembly input, exact linked Inference image, successful Cargo compiler JSON build receipt, checked PowerShell interpreter and a fresh campaign output directory. The native child holds an exclusive service lease. The parent records the original input, newly emitted descriptor and public provenance, whole-job settlement and elapsed time.

After the native descriptor and public provenance match their observation receipt, copy those two files into the clean runtime directory. `windows_product_projection.py --stage request` runs the Rust request emitter there. `--stage emit` authenticates that exact request receipt, runs the package projection and records its custody. Supply the projection and execution receipt hashes to the package builder. Each stage has its own original 120-second limit.

Run `windows_product_pin_audit.py` with isolated Python to compare the original absolute pins in the CU15 producer and current public-source input. Its private diagnostic reports each mismatching hash, missing paths and the separate relative-pin count. It reads the original configure-guard blob from Git for comparison and never executes archival code.
