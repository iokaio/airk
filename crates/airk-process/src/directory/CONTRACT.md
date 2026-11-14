# Opened directory and native object identity primitives

Owner: `airk-process::directory`. This foundation has no AIRK dependencies,
record formats, ownership policy or source content reads. Its narrow raw
flush and exact-object disposition APIs require the caller's authorization.
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
  The shared `validate_child_name` checks raw native separators as well as
  parsed components: `child/` and `child/.` cannot normalize into accepted
  names. Windows also refuses alternate streams and embedded NUL; Host's
  rooted publication adapter uses the same checker.
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
- `sync_all()` flushes the pinned directory object. Unix syncs the held
  descriptor. Windows opens a separate write-capable, nofollow directory
  handle while all ancestor name pins remain live, checks its exact native
  identity against the original handle, then calls `sync_all`. It does not
  widen the readonly pin's rights or change ACLs. Permission, unsupported
  filesystem and flush errors propagate. API success is not evidence of
  power-loss/reboot ordering on every filesystem.
- `remove_child_if_matches(single_component, expected_native_identity)` is a
  narrowly qualified empty-directory operation. Windows retains the parent
  name pins, opens the child without following reparse points with DELETE and
  list/read/control access, omits delete sharing, and compares its exact native
  identity before handle-disposition deletion. Mismatch, redirection, sharing
  conflicts and nonempty children refuse; there is no pathname deletion
  fallback. Callers first release their own readonly child pins, then this
  reacquisition checks identity before any effect. Unix returns Unsupported
  without attempting fstat/unlinkat as an atomic identity check; callers retain
  an empty owned root and disclose it when no qualified primitive exists.
- Windows `remove_file_if(single_component, predicate(&mut File, &PendingFileProof))`
  opens a nofollow single-linked file with read/delete access, excluding
  write/delete sharing, then marks ordinary reversible deletion pending.
  Its opaque non-clone proof validates that exact native identity, regular
  unredirected object, DeletePending=true and zero remaining links (Windows
  removes the one link from its reported count while pending). A raced alias
  is refused; this does not relax ordinary one-link validators. The caller
  checks its ACL/owner/content policy through the same handle and proof.
  False/error cancels pending; true closes that same pending object and
  flushes the pinned parent. Panic unwinding attempts cancellation. Unknown
  OS cancellation failures are reported, never called preservation. Unix
  returns Unsupported; no path unlink follows the predicate.
- Handles are owned, non-inheritable and shared through retained `Arc` parents;
  dropping a parent value does not release a live child's ancestor pins. No
  handle or identity is serialized, cached by pathname or transferred over IPC.

## Checks and scope

```console
python scripts/dev-check.py -p airk-process --target-dir artifacts/v190/p10-target
cargo nextest run --profile ci -p airk-process --no-default-features --lib --locked --timings -E 'test(directory::tests::)'
```

The selection is eleven tests on Windows and eight on Unix. They cover stable
identity, single-component child admission, file refusal, retained ancestors,
Windows name pinning/junction refusal and Unix rename/rebinding/symlink
refusal. Junction creation must succeed; no privilege skip establishes a pass.
The tests use isolated fixture directories and no sleeps. A Windows pass does
not compile or qualify the Unix implementation or network filesystems.

This foundation precedes handle-based private policy and recovery consumers.
No config archive, installation receipt writer, permission mutation or
filesystem publication is implemented or qualified by these tests.


## Rooted mutation foundation

`publication::{publish_new,publish_new_at}` owns the existing exclusive rename
backend; Host retains compatibility reexports and its original tests.
`OpenedDirectory::{open_file,create_file,create_child_directory,publish_file}`
accept single component names. Unix file/directory creation and publication use
the original dirfd after namespace rebinding. Windows uses absolute native
operations only while all ancestor name pins remain alive. Directory creation
runs the caller's Windows security initializer against the exact child name;
Unix applies the supplied mode with mkdirat instead. This foundation supplies no
privacy policy. Every opened file is nofollow, regular and single-linked;
Windows file opens exclude write/delete sharing. Replace publication replaces
only the selected directory entry and does not follow the old leaf. The owner
must provide serialization and any expected-old-byte authorization. File data
must be synced before publication, followed by directory sync. Unsupported,
sharing and durability errors propagate. No arbitrary uncooperative same-user
writer exclusion or power-loss qualification follows from these APIs.

Windows publication and stream enumeration convert lexical absolute paths to
extended native paths, preserving UTF-16 filenames beyond MAX_PATH.
`require_default_stream(path,File)` is the relocated existing config NTFS stream
observer; the caller retains the exact named file and ancestor pins. Any extra
stream or incomplete enumeration refuses. Installer payload hashing/copying,
portable full-tree proof, Inno custody and native payload deletion use it;
unchanged config observers call the same owner. This adds no permission policy
or stream deletion/ignore behavior.


Windows exact file removal now opens one READ|DELETE/shareREAD object and sets reversible ordinary delete-pending before invoking the content predicate. This prevents creation of another named stream, which default-stream write sharing alone does not prevent. It rechecks single-link/current-object proof, queries the sole default stream by GetFileInformationByHandleEx(FileStreamInfo) through that actual handle, and lets the caller hash/read that handle. False/error clears pending before close and preserves content; a failure to restore pending is explicitly reported, never called successful preservation. True closes the pending handle and flushes the held parent. This remains qualified only on tested Windows filesystems; Unix does not gain this claim.

The sole shared stream observer uses bounded aligned handle-based FileStreamInfo, including while deletion is pending. Extra records, a different default name/size, malformed or incomplete/oversized native query all refuse. Path parameters remain compatibility inputs; no second per-path stream observer exists. This raw adapter does not grant record ownership or choose deletion targets.

A pending-state RAII owner attempts cancellation on unwinding and retries when the normal false/error cancellation itself fails. An unrecoverable OS cancellation failure remains an explicitly unknown outcome, not proof of preservation. Predicate panics do not deliberately commit deletion. The actual Windows panic regression catches unwinding, verifies the same original identity/bytes, then verifies stream mutation remains possible only after cancellation.
