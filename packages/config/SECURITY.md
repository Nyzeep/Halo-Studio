# Configuration File Security Contract

The config package treats registered paths, configuration bytes, and encrypted
backup references as security-sensitive data. Main registers absolute target
and root paths and may issue an opaque `targetId` to a Renderer as a scoped
capability. The Renderer must never submit a path; it may only return the
`targetId` that Main issued. An encrypted backup reference must never be exposed
to a Renderer. For every preview, commit, and rollback request, Main resolves
the `targetId` through its registry and revalidates the path, permissions, and
expected fingerprint before allowing a filesystem mutation.

## Filesystem Prerequisite

The application user must own each `allowedRoot` and target parent directory
and must have exclusive write access to them for the duration of a read,
commit, or rollback. Untrusted users and processes must not be able to rename,
replace, or create entries in those directories. If this prerequisite is not
met, the package's path and identity checks do not provide a security boundary.

## Reads

Reads open the target once and consume bytes only through that file handle.
The package compares handle and pathname identity before and after the bounded
read, rejects links, non-regular files, and files with multiple hard links, and
rejects configuration data larger than 1 MiB. UTF-8 is decoded only after the
final validation succeeds.

## Atomic Replacement And Residual Risk

Node.js does not expose portable `openat` and `renameat` operations or a
Windows `ReplaceFileW` binding. The following pathname operations therefore
cannot be bound to the already-open parent directory handle:

- temporary pathname creation after validating the parent;
- guarded temporary pathname cleanup after checking the temp identity;
- the final pathname rename after checking parent and temp identities; and
- missing-target unlink after checking parent and target identities.

Each operation has a residual race window between its final identity check and
the pathname operation. A process with concurrent write access to the parent
directory can invalidate those checks. All four windows are governed by the
same exclusive-write parent-directory prerequisite above. That prerequisite is
mandatory, not a defense-in-depth recommendation.

The writer records whether replacement occurred. A failure after replacement,
including a parent-directory sync or close failure, is reported separately and
the transaction attempts fingerprint-guarded recovery from the encrypted
backup. On POSIX systems the temporary file and parent directory are synced;
on Windows, pure Node cannot provide an equivalent parent-directory fsync.

## Permissions

For an existing file, atomic replacement preserves its POSIX mode where the
platform supports `chmod`. A newly created file requests mode `0600`, subject
to the process umask and filesystem behavior. Windows mode `0600` is not a
Windows ACL security guarantee. Deployments on Windows must configure and
verify appropriate ACLs on the root and target directories separately.
