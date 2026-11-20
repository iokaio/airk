// SPDX-License-Identifier: Apache-2.0
#include <cuda_fp16.h>
#include <cublas_v2.h>
#include <cudnn_version.h>

static_assert(CUDNN_MAJOR == 9 && CUDNN_MINOR == 10 && CUDNN_PATCHLEVEL == 2);
extern "C" __global__ void airk_compile_fp16(__half *out, const __half *input) {
    out[threadIdx.x] = __hadd(input[threadIdx.x], input[threadIdx.x]);
}
