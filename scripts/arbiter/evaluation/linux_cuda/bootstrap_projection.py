# SPDX-License-Identifier: Apache-2.0
"""Validate and project same-core protected Linux CUDA bootstrap and ELF link metadata."""
import shlex


def project_env(path, body):
    namespace = b"namespace onnxruntime {"
    data = path.read_bytes()
    old = b"    *handle = dlopen(provider ? name.c_str() : library_filename.c_str(), RTLD_NOW | (provider ? RTLD_NOLOAD : 0) | (global_symbols ? RTLD_GLOBAL : RTLD_LOCAL));"
    new = b"""    auto* window = airk_cuda_bootstrap;
    const bool first_cuda = name == "libonnxruntime_providers_cuda.so" && window && !window->mapped;
    const char* path = first_cuda ? window->path : (provider ? name.c_str() : library_filename.c_str());
    const int mode = RTLD_NOW | ((provider && !first_cuda) ? RTLD_NOLOAD : 0) | (global_symbols ? RTLD_GLOBAL : RTLD_LOCAL);
    *handle = dlopen(path, mode);
    if (first_cuda && *handle) window->mapped = true;"""
    if data.count(namespace) != 1 or data.count(old) != 1 or not body or body in data:
        raise ValueError("unexpected Linux Env bootstrap source anchor")
    path.write_bytes(data.replace(namespace, body + b"\n\n" + namespace).replace(old, new))


def link_argv(row, build, output, compiler):
    """Replay only the frozen native link argv with a fresh output and explicit SONAME."""
    argv = shlex.split(row)
    if argv[:3] != [":", "&&", str(compiler)] or argv[-2:] != ["&&", ":"]:
        raise ValueError("unexpected native provider link shell envelope")
    argv = argv[2:-2]
    name = "libonnxruntime_providers_cuda.so"
    if argv.count("-o") != 1 or argv[argv.index("-o")+1] != name or argv.count("-shared") != 1:
        raise ValueError("unexpected CUDA shared object output")
    if any(token in {";", "&&", "|", "||", ">", "<"} or "rpath" in token or "soname" in token for token in argv):
        raise ValueError("unexpected Linux link or search path options")
    dep = "-Wl,--dependency-file=CMakeFiles/onnxruntime_providers_cuda.dir/link.d"
    if argv.count(dep) != 1:
        raise ValueError("unexpected dependency-file option")
    argv.remove(dep)
    argv[argv.index("-o")+1] = str(output)
    argv.insert(argv.index("-shared"), "-Wl,-soname," + name)
    return argv
