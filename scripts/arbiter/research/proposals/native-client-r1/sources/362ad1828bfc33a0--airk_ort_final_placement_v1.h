// AIRK application-private ABI. Same statically linked ORT1.30.0 core only.
#pragma once
#include <stddef.h>
#include <stdint.h>
#include "onnxruntime_c_api.h"
#if !defined(_WIN32)
#error AIRK final placement v1 is currently Windows-only.
#endif
#ifdef __cplusplus
extern "C" {
#endif
// Strings are borrowed only for this synchronous callback. Never unwind.
typedef int32_t (__cdecl *AirkOrtPlacementCallbackV1)(
    void* user, uint64_t subgraph_index,
    const char* node_name, size_t node_name_bytes,
    const char* domain, size_t domain_bytes,
    const char* op, size_t op_bytes,
    const char* provider, size_t provider_bytes);
// 0 complete; 1 invalid; 2 foreign runtime; 3 incomplete; 4 cap;
// 5 callback rejected; 6 native exception. Partial counters never mean success.
int32_t __cdecl airk_ort_collect_final_placement_v1(
    const OrtApiBase* base, const OrtSession* session,
    AirkOrtPlacementCallbackV1 callback, void* user,
    uint64_t max_nodes, uint64_t max_string_bytes,
    uint64_t* out_nodes, uint64_t* out_string_bytes, int32_t* out_ort_error_code)
#ifdef __cplusplus
    noexcept
#endif
    ;
#ifdef __cplusplus
}
#endif
