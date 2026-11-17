// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

#include "core/providers/cuda/cufft_loader.h"

#ifndef USE_CUDA_MINIMAL

#include <string>

#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <filesystem>
#else
#include <dlfcn.h>
#endif

namespace {

// cuFFT's SONAME/DLL version tracks the CUDA major version it ships with:
// CUDA 12.x -> cuFFT 11, CUDA 13.x -> cuFFT 12. Select the name that matches the
// CUDA toolkit this library was built against (CUDA_VERSION comes from cuda_pch.h).
#if CUDA_VERSION >= 13000 && CUDA_VERSION < 14000
#ifdef _WIN32
constexpr const char* kCufftLibraryName = "cufft64_12.dll";
#else
constexpr const char* kCufftLibraryName = "libcufft.so.12";
#endif
#elif CUDA_VERSION >= 12000 && CUDA_VERSION < 13000
#ifdef _WIN32
constexpr const char* kCufftLibraryName = "cufft64_11.dll";
#else
constexpr const char* kCufftLibraryName = "libcufft.so.11";
#endif
#else
#error Unsupported CUDA_VERSION for dynamic cuFFT loading
#endif


void* LoadLibraryCandidate(const std::string& candidate, std::string& error) {
#ifdef _WIN32
  // AIRK D08: use the directory of this already authenticated provider module.
  // No PATH, CWD, registry, Python package or user-directory discovery is allowed.
  HMODULE owner = nullptr;
  if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
                         GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                         reinterpret_cast<LPCWSTR>(&LoadLibraryCandidate), &owner)) {
    error = "Unable to identify authenticated CUDA provider module";
    return nullptr;
  }
  std::wstring module_path(32768, L'\0');
  DWORD length = GetModuleFileNameW(owner, module_path.data(), static_cast<DWORD>(module_path.size()));
  if (length == 0 || length >= module_path.size()) {
    error = "CUDA provider module path is unavailable or truncated";
    return nullptr;
  }
  module_path.resize(length);
  const auto full_path = std::filesystem::path{module_path}.parent_path() / candidate;
  if (!full_path.is_absolute()) {
    error = "CUDA dependency requires an absolute authenticated path";
    return nullptr;
  }
  HMODULE handle = LoadLibraryExW(full_path.c_str(), nullptr,
                                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32);
  if (handle == nullptr) {
    error = "Authenticated CUDA dependency unavailable; error " + std::to_string(GetLastError());
  }
  return reinterpret_cast<void*>(handle);
#else
  dlerror();
  void* handle = dlopen(candidate.c_str(), RTLD_NOW | RTLD_LOCAL);
  if (handle == nullptr) {
    const char* dl_error = dlerror();
    error = "dlopen failed for " + candidate + ": " + (dl_error != nullptr ? dl_error : "unknown error");
  }
  return handle;
#endif
}

void* GetLibrarySymbol(void* handle, const char* symbol, std::string& error) {
#ifdef _WIN32
  void* address = reinterpret_cast<void*>(GetProcAddress(reinterpret_cast<HMODULE>(handle), symbol));
  if (address == nullptr) {
    error = "GetProcAddress failed for " + std::string(symbol) + " with error " + std::to_string(GetLastError());
  }
  return address;
#else
  dlerror();
  void* address = dlsym(handle, symbol);
  const char* dl_error = dlerror();
  if (address == nullptr || dl_error != nullptr) {
    error = "dlsym failed for " + std::string(symbol) + ": " + (dl_error != nullptr ? dl_error : "unknown error");
  }
  return address;
#endif
}

}  // namespace

namespace onnxruntime::cuda {

CufftLibrary& CufftLibrary::Get() {
  static CufftLibrary library;
  return library;
}

bool CufftLibrary::Available() {
  return EnsureLoaded();
}

std::string CufftLibrary::Error() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return error_.empty() ? CufftUnavailableErrorString() : error_;
}

bool CufftLibrary::EnsureLoaded() {
  std::lock_guard<std::mutex> lock(mutex_);
  if (load_attempted_) {
    return available_;
  }

  load_attempted_ = true;
  handle_ = LoadLibraryCandidate(kCufftLibraryName, error_);
  if (handle_ != nullptr) {
    available_ = true;
    error_.clear();
    return true;
  }

  available_ = false;
  if (error_.empty()) {
    error_ = "cuFFT library was not found";
  }
  return false;
}

void* CufftLibrary::ResolveSymbol(const char* symbol) {
  {
    std::lock_guard<std::mutex> lock(mutex_);
    auto it = symbols_.find(symbol);
    if (it != symbols_.end()) {
      return it->second;
    }
  }

  if (!EnsureLoaded()) {
    return nullptr;
  }

  std::lock_guard<std::mutex> lock(mutex_);
  std::string symbol_error;
  void* address = GetLibrarySymbol(handle_, symbol, symbol_error);
  if (address == nullptr) {
    available_ = false;
    error_ = symbol_error;
    return nullptr;
  }

  symbols_.emplace(symbol, address);
  return address;
}

const char* CufftUnavailableErrorString() {
  return "cuFFT is not available. Install cuFFT or update the system library search path to enable FFT (Rfft/Irfft) operators.";
}

}  // namespace onnxruntime::cuda

#endif  // USE_CUDA_MINIMAL
