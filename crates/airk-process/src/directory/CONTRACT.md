# Opened directory and native object identity primitives

Owner: `airk-process::directory`. This foundation has no AIRK dependencies,
record formats, ownership policy, source content reads, mutations or cleanup.
The installer and agent-config owners adapt these raw facts into their own
reviewed Rust receipt contracts. No runtime or recovery consumer lands here.

## API and guarantees

- `OpenedDirectory::open(absolute_path)` opens every component without
  following a symlink/reparse point. Parent traversal, unsupported roots and
  regular-file final objects refuse. It neither creates a directory nor
  changes permissions. Native errors stay observable.
- `open_child(single_component)` refuses absolute names, separators and parent
  traversal. On Unix it uses `openat` against the original directory descriptor
  with `O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`. It never resolves through a new
  directory appearing at the original parent path.
- Unix descriptors pin objects, not their names. Another actor can rename the
  original directory and install a replacement at its old path. Relative
  operations still address the pinned object; callers must reobserve the live
  named root before claiming path continuity. Object identity is not authority.
- Windows opens every drive-path component with `OPEN_REPARSE_POINT` and
  `BACKUP_SEMANTICS`, checks the opened metadata and retains all ancestor
  handles without `FILE_SHARE_DELETE`. Those names cannot be renamed/deleted
  while this pin or a descendant pin remains alive. Sharing read/write is
  allowed; this does not freeze directory contents or exclude file writers.
  The handle requests list-directory, read-attributes and read-control rights.
  Windows excludes metadata-only opens from sharing checks; list-directory
  access makes omission of delete sharing effective even for empty directories.
  This grants no write/delete access and performs no directory enumeration.
  Later native ACL/owner checks can query the exact opened object.
  Local drive and verbatim-drive paths are supported. UNC, device and other
  roots refuse until their native semantics have separate qualification.
- `identity()` and `identity(&File)` return raw Windows volume-serial-64 plus
  FILE_ID_128 bytes, or Unix device/inode. A file ID is neither a path nor an
  ownership statement; unsupported identity observation refuses. The free
  function does not assert how its caller opened that file.
- `as_file()` exposes the pinned directory object for native handle-based
  privacy/ownership checks. The API deliberately exposes no path-as-authority
  shortcut. No check here proves current user/SID, private ACL/mode, same-volume
  publication, signature, ignore eligibility, root enrollment or receipt truth.
  Those remain the owning adapter's prerequisites.
- Handles are owned, non-inheritable and shared through retained `Arc` parents;
  dropping a parent value does not release a live child's ancestor pins. No
  handle or identity is serialized, cached by pathname or transferred over IPC.

## Checks and scope

```console
python scripts/dev-check.py -p airk-process --target-dir artifacts/v190/p10-target
cargo nextest run --profile ci -p airk-process --no-default-features --lib --locked --timings -E 'test(directory::tests::)'
```

The selection is eight tests on Windows and six on Unix. They cover stable
identity, single-component child admission, file refusal, retained ancestors,
Windows name pinning/junction refusal and Unix rename/rebinding/symlink
refusal. Junction creation must succeed; no privilege skip establishes a pass.
The tests use isolated fixture directories and no sleeps. A Windows pass does
not compile or qualify the Unix implementation or network filesystems.

This foundation precedes handle-based private policy and recovery consumers.
No config archive, installation receipt writer, permission mutation or
filesystem publication is implemented or qualified by these tests.
