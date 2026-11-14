# Installed service manual start

Owner: `airk-installer::manual_start`. The thin `airk` executable delegates
start/status to its verified I4 CLI; it implements no second startup policy.

- Native fixed user-folder discovery selects the install pointer. The shared
  IPC I4 reader validates its complete record, both image digests and the
  injected C10 platform signature policy before the CLI is selected.
- The current service image must be that record's canonical service path and
  this build's version. No arbitrary neighboring executable, workspace setting
  or PATH entry substitutes for an unverifiable/mismatched installation.
- The record's executable paths determine the pair. Windows/Linux
  `service/airk` plus `bin/airkctl`, the macOS bundle and a public portable
  I4-registered layout use the same owner. Portable setup is a separate
  transaction, not permission to hand-author installation JSON.
- Only an uninstalled debug build in a directory named `debug` has the explicit
  raw-development sibling fallback. Release images and unverified installation
  records do not. Packaged/debug-pair directories use verified I4 setup.
- No arguments: observe status then request the existing canonical service
  start if needed. User-style arguments: report that `airk` takes no commands
  and observe state only. A resolver failure starts no process; full original
  CLI errors remain visible.

```console
cargo nextest run --profile ci -p airk-installer --no-default-features --lib --locked --timings -E 'test(manual_start::)'
```

The selection is eight tests, including six pair cases. Literal platform
layouts use the real I4 filesystem reader with an injected signature
observation and inert image bytes. They do not execute a packaged image or
qualify an installed no-argument start; that remains an artifact boundary.
