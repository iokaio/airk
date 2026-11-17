# CUDA native execution tooling

`../cuda-native-execution.py` owns the bounded native package-metadata launch.
Its [contract](CONTRACT.md) defines custody and evidence requirements.
The [maintained helpers](helpers/README.md) contain byte-identical process,
identity and settlement primitives used by current CUDA and FP16 tooling.
Generated plans, runtime images, model inputs and receipts remain private artifacts.
