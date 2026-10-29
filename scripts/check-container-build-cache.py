# SPDX-License-Identifier: Apache-2.0
"""Exercise replaced/restored Cargo caches without compiling the workspace."""
from pathlib import Path
import subprocess
import tempfile
import uuid


ROOT = Path(__file__).resolve().parents[1]
CACHES = ("/usr/local/cargo/registry", "/usr/local/cargo/git", "/build/target")


def mounts(key):
    return " ".join(f"--mount=type=cache,id={key}-{number},target={path},sharing=locked"
                    for number, path in enumerate(CACHES))


def main():
    key = "airk-cache-test-" + uuid.uuid4().hex
    parent = ROOT / "artifacts" / "container-cache-tests"
    parent.mkdir(parents=True, exist_ok=True)
    if not parent.resolve().is_relative_to(ROOT.resolve()):
        raise ValueError("Cache test context escapes the workspace")
    with tempfile.TemporaryDirectory(prefix="case-", dir=parent) as temporary:
        context = Path(temporary)
        (context / "container-build-user.sh").write_bytes(
            (ROOT / "scripts/container-build-user.sh").read_bytes().replace(b"\r\n", b"\n"))
        (context / "verify.sh").write_text("""set -eu
test "$(id -u)" = 10001
test "$(id -g)" = 10001
test "$(id -G)" = 10001
test "$HOME" = /home/airkbuild
test -w "$HOME"
test "$(cat /unchanged-source)" = original
test "$(stat -c %u /unchanged-source)" = 0
test ! -w /unchanged-source
for cache in /usr/local/cargo/registry /usr/local/cargo/git /build/target; do
    if test "$1" = warm; then test "$(cat "$cache/result")" = complete; fi
    if test "$1" = restored; then
        test "$(cat "$cache/private/value")" = retained
        printf updated > "$cache/private/value"
    fi
    printf complete > "$cache/result"
done
""", encoding="utf-8", newline="\n")
        common = """FROM rust:1.98.0-slim-bookworm AS base
RUN useradd --create-home --uid 10001 airkbuild && printf original > /unchanged-source && chmod 644 /unchanged-source
COPY container-build-user.sh verify.sh /
"""
        # The broken target reproduces an unwritable fresh root-owned mount.
        # The fixed target uses the production privilege wrapper.
        recipe = common + f"""FROM base AS broken
USER 10001:10001
RUN {mounts(key + '-broken')} touch /usr/local/cargo/registry/probe
FROM base AS fixed
RUN {mounts(key)} sh /container-build-user.sh sh /verify.sh cold
RUN {mounts(key)} for cache in {' '.join(CACHES)}; do mkdir -p "$cache/private"; printf retained > "$cache/private/value"; chown -R 0:0 "$cache"; chmod 700 "$cache" "$cache/private"; chmod 600 "$cache/private/value"; done
RUN {mounts(key)} sh /container-build-user.sh sh /verify.sh restored
RUN {mounts(key)} sh /container-build-user.sh sh /verify.sh warm
RUN {mounts(key + '-replacement')} sh /container-build-user.sh sh /verify.sh cold
"""
        (context / "Dockerfile").write_text(recipe, encoding="utf-8", newline="\n")
        command = ["docker", "buildx", "build", "--progress=plain"]
        broken = subprocess.run(command + ["--target", "broken", str(context)],
                                text=True, capture_output=True, timeout=180)
        if not broken.returncode or "Permission denied" not in broken.stdout + broken.stderr:
            raise RuntimeError("Expected root-owned-cache permission failure was not reproduced: "
                               + (broken.stdout + broken.stderr)[-4000:])
        subprocess.run(command + ["--target", "fixed", str(context)], check=True, timeout=180)
    print("Verified empty, replaced, restored and warm caches; build commands remain UID/GID 10001 and source stays unchanged")


if __name__ == "__main__":
    main()
