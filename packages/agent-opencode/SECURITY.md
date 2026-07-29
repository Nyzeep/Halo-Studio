# OpenCode Credential Handling

The managed OpenCode server uses a random password generated in the Main
process. JavaScript strings are immutable and may be copied by the runtime, so
the password cannot be reliably zeroized from memory.

The implementation limits exposure instead:

- Main generates the credential and retains ownership of it.
- The password is passed only to the managed child environment and to the
  Basic Authentication header for loopback health and event requests.
- Credentials are never included in runtime snapshots, logs, IPC payloads, or
  persistent storage.
- Credential references and the password entry in the spawn environment are
  cleared after stop, crash, or any startup failure, including a late spawn
  fulfillment after the startup deadline.
- Public errors use stable error codes and messages. Child stderr, HTTP response
  bodies, executable paths, workspace paths, and passwords are not copied into
  public errors.

These controls reduce the lifetime and propagation of the password, but they do
not claim memory zeroization guarantees that JavaScript cannot provide.
