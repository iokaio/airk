// SPDX-License-Identifier: Apache-2.0
// Projected into the pinned Linux Env translation unit by the AIRK CUDA producer.
#include "core/session/onnxruntime_c_api.h"
#include <cstdio>
#include <cstring>

namespace {
struct AirkCudaBootstrap {
  const char* path;
  bool mapped;
};
thread_local AirkCudaBootstrap* airk_cuda_bootstrap = nullptr;
}

extern "C" __attribute__((visibility("default"))) int32_t
airk_ort_bootstrap_linux_cuda_v1(const OrtApiBase* expected, int32_t provider_fd) noexcept {
  if (expected == nullptr || expected != OrtGetApiBase() ||
      std::strcmp(expected->GetVersionString(), "1.30.0") != 0 ||
      provider_fd < 0 || airk_cuda_bootstrap != nullptr) {
    return 1;
  }
  const int seals = fcntl(provider_fd, F_GET_SEALS);
  constexpr int required = F_SEAL_WRITE | F_SEAL_SHRINK | F_SEAL_GROW | F_SEAL_SEAL;
  if (seals < 0 || (seals & required) != required) return 2;
  char path[64];
  const int count = std::snprintf(path, sizeof(path), "/proc/self/fd/%d", provider_fd);
  if (count <= 0 || static_cast<size_t>(count) >= sizeof(path)) return 3;
  const OrtApi* api = expected->GetApi(22);
  if (!api) return 4;
  AirkCudaBootstrap window{path, false};
  struct Scope {
    ~Scope() { airk_cuda_bootstrap = nullptr; }
  } scope;
  airk_cuda_bootstrap = &window;
  try {
    OrtCUDAProviderOptionsV2* options = nullptr;
    OrtStatus* status = api->CreateCUDAProviderOptions(&options);
    if (status) {
      api->ReleaseStatus(status);
      return 5;
    }
    if (!options) return 6;
    struct Options {
      const OrtApi* api;
      OrtCUDAProviderOptionsV2* options;
      ~Options() { api->ReleaseCUDAProviderOptions(options); }
    } guard{api, options};
    const char* empty[1] = {nullptr};
    status = api->UpdateCUDAProviderOptions(options, empty, empty, 0);
    if (status) {
      api->ReleaseStatus(status);
      return 7;
    }
    return window.mapped ? 0 : 8;
  } catch (...) {
    return 9;
  }
}
