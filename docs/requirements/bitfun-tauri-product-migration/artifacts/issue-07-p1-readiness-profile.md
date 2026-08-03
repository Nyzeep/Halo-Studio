# Issue 07 P1 Readiness Profile Evidence

Date: 2026-08-03

Scope: issue-07 Pi RPC adapter, public Probe/Runtime seam contracts, fake-Pi
fixture coverage, and this validation note only.

## Local Version Evidence

- `where.exe pi`: exit code 1, `INFO: Could not find files for the given pattern(s).`
- `Get-Command pi -All`:
  - `C:\Users\Nyzee\AppData\Roaming\npm\pi.ps1`
  - `C:\Users\Nyzee\AppData\Roaming\npm\pi.cmd`
  - `C:\Users\Nyzee\AppData\Roaming\npm\pi`
- `pi --version`: exit code 0, `0.83.0`

Read-only static source evidence:

- `D:\pi-main\packages\coding-agent\package.json` declares
  `@earendil-works/pi-coding-agent` version `0.83.0`.
- `D:\pi-main\packages\coding-agent\docs\rpc.md` documents the P1 command and
  event names used by Halo.
- `D:\pi-main\packages\coding-agent\src\modes\rpc\rpc-types.ts` defines the
  same command/response and extension UI protocol surface for version 0.83.0.

## Compatibility Conclusion

PASS for the fixed `pi-rpc-0.83.0-p0` compatibility profile, based on the local
executable version probe, the static 0.83.0 RPC documentation/source shape, and
fake-Pi public-port contract fixtures.

No real `pi --mode rpc` process was started for this profile update, no prompt
or model request was sent, and no credential files were read. The installed Pi
real-RPC acceptance remains owned by issue 14.

## Public Seam

`PiRpcReply::Available` now carries a bounded `PiRpcAvailabilitySummary`.
Workbench Runtime snapshots project the same summary through
`adapter.readiness`.

The public summary exposes only enumerated version/profile/evidence and the
fixed P0 required capability set:

- `prompt`
- `follow_up`
- `abort`
- `get_state`
- `get_entries`
- `get_entries.entries`
- `get_entries.leaf_id`
- `get_entries.since`
- `message_update`
- `tool_execution_start`
- `tool_execution_update`
- `tool_execution_end`
- `agent_settled`
- `extension_ui_request`
- `extension_ui_response`

Pi session IDs, entry IDs, raw tool-call IDs, command output, provider/model
objects, credentials, Authorization values, base URLs, environment variables,
filesystem paths, and raw JSONL remain behind `PiRpcPort`.

## Coverage Notes

- Public Probe and Workbench Runtime snapshot tests assert safe version/profile
  projection and sensitive-field absence.
- Fake-Pi contract fixtures cover 0.83.0 profile selection, missing required
  `get_entries` capability fail-closed behavior, and `agent_end` not replacing
  `agent_settled`.
- Existing issue-07 fake-Pi coverage continues to exercise LF/CR/Unicode
  framing, response id correlation/out-of-order/idless handling,
  `get_state`/`get_entries`/`since` cursor checks, prompt/follow_up/abort,
  message/tool/agent_settled events, extension_ui_request/extension_ui_response,
  extension_error, EOF/protocol failures, and cleanup races.
