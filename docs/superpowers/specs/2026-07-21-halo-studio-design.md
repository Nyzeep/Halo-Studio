# Halo Studio Design

Date: 2026-07-21

## Purpose

Halo Studio is a Windows-first desktop workbench for developers who use multiple coding agents in the same project. The first release will integrate OpenCode, Pi, Codex CLI, and Claude Code behind one polished desktop shell. It should make each tool easy to launch, configure, compare, and hand off to without forcing the user to remember four sets of config files, MCP formats, credentials, and command-line flags.

The product should not try to replace the official CLIs. The durable value is the orchestration layer around them: a beautiful UI, reliable terminal hosting, unified configuration, safe profile switching, MCP management, and agent-to-agent handoff.

## Reference Inputs

The design is informed by these projects and docs:

- OpenCode repository and docs: `https://github.com/anomalyco/opencode`, `https://dev.opencode.ai/docs/config`, `https://dev.opencode.ai/docs/mcp-servers`
- Pi repository and docs: `https://github.com/earendil-works/pi`, `https://pi.dev/docs/latest/rpc`
- cc-switch repository: `https://github.com/farion1231/cc-switch`
- Codex CLI config and MCP docs: `https://developers.openai.com/codex/config-reference`, `https://developers.openai.com/codex/mcp`
- Claude Code settings and MCP docs: `https://docs.anthropic.com/en/docs/claude-code/settings`, `https://docs.anthropic.com/en/docs/claude-code/mcp`

These references should be rechecked during implementation because each vendor's CLI and configuration format can change.

## Product Principles

1. Keep the official CLIs as the execution source of truth.
2. Use adapters instead of hard-coded vendor logic in the UI.
3. Treat configuration writes as sensitive operations with preview, backup, validation, and rollback.
4. Make terminal mode reliable before building richer native chat integrations.
5. Prefer local-first storage and Windows-native credential protection.
6. Build a practical developer workbench, not a marketing dashboard.

## Recommended Architecture

Halo Studio should use Electron, React, TypeScript, and Node.js for the first Windows release.

Electron is the recommended shell because Halo must run local CLIs, manage pseudo-terminals, stream output, send interrupts, resize terminal sessions, read and write user-level config files, and package cleanly for Windows. The React UI can move quickly, while the Electron main process can host the privileged local orchestration services.

Tauri remains a possible later optimization if app size and memory become more important than CLI integration speed. It is not the preferred first implementation path because Windows pseudo-terminal handling and Node ecosystem integration are central to this product.

## Runtime Layers

### Desktop Shell

The desktop shell owns the window lifecycle, menus, tray, file dialogs, auto-start behavior, and update flow. It should expose a narrow IPC API to the renderer and keep direct filesystem and process access out of the React UI.

Initial responsibilities:

- Create the main app window.
- Manage Windows tray quick actions.
- Launch and stop agent sessions.
- Expose safe IPC commands for config reads and writes.
- Store app settings in the user data directory.

### Renderer UI

The UI is a React app using Vite, TypeScript, Tailwind CSS, Radix primitives, shadcn-style components, and lucide icons.

The first screen should be the actual workbench:

- Left rail: workspaces, agent switcher, profile switcher.
- Center: terminal tabs and future native chat tabs.
- Right panel: session context, config preview, MCP servers, command palette results.
- Bottom/status area: active agent, model, workspace, token/source status when available.

The design should feel like a modern local IDE companion: dense, calm, high-contrast enough for long coding sessions, and fast to scan.

### Agent Adapter Layer

Each vendor gets an adapter with the same interface. The UI talks only to the adapter registry.

Adapter responsibilities:

- Detect whether the CLI is installed.
- Read version and capability info.
- Provide launch command templates.
- Start terminal sessions.
- Locate known config files.
- Read and validate current config.
- Generate vendor-specific config patches.
- Report supported integration modes.

Initial adapters:

- `claude-code`
- `codex-cli`
- `opencode`
- `pi`

The adapter interface should support multiple modes:

- `terminal`: run the official CLI in a PTY.
- `rpc`: use a structured local protocol when supported.
- `mcp`: expose or consume local MCP tools.
- `config-only`: manage configuration even when no runtime session is active.

### PTY Session Manager

The PTY manager hosts real terminal sessions for each agent. It should use Node PTY support on Windows ConPTY and stream data to `xterm.js`.

Core requirements:

- Create, resize, focus, pause, stop, and restart sessions.
- Preserve session metadata in local storage.
- Support Ctrl+C and process termination.
- Support multiple tabs per workspace.
- Attach launch environment variables per profile.
- Write session logs with opt-in retention.

Terminal mode is the compatibility baseline. If a richer API fails, the user can always fall back to the official CLI in the terminal panel.

### Configuration Service

Halo keeps its own normalized configuration model and compiles that model into vendor-specific files.

The service must never blindly overwrite a vendor file. Every write follows this flow:

1. Discover target file.
2. Parse current file using a structured parser.
3. Build a minimal patch.
4. Validate the generated file.
5. Show a diff preview in the UI.
6. Write a timestamped backup.
7. Write atomically.
8. Verify by reading the file again.
9. Offer rollback when verification fails.

Supported config formats:

- JSON
- JSONC
- TOML
- environment variable templates

The service should preserve unknown fields whenever possible.

### Credential Service

Sensitive credentials should not be stored in Halo's SQLite database or ordinary JSON settings. The Windows-first implementation should use Windows Credential Manager or DPAPI-backed secure storage through a well-maintained Node library.

The credential service stores:

- API keys
- provider tokens
- profile-specific secrets
- optional environment variable values

The UI should clearly distinguish normal config from secrets. Secret values should be masked, revealable only on explicit user action, and never included in logs or exported config bundles by default.

### MCP Registry

Halo should expose a unified MCP registry that can write to each supported agent's native config format.

Normalized MCP server model:

- `id`
- `displayName`
- `transport`: `stdio`, `sse`, `http`
- `command`
- `args`
- `env`
- `url`
- `headers`
- `enabled`
- `scopes`
- `targetAgents`

Vendor mapping:

- Claude Code: support project `.mcp.json`, CLI-scoped MCP commands, and user/local settings where appropriate.
- Codex CLI: write `[mcp_servers.<name>]` entries in `~/.codex/config.toml` or project `.codex/config.toml`.
- OpenCode: write the `mcp` object inside OpenCode JSON/JSONC config while respecting global and project config layering.
- Pi: support the file locations and adapter flow exposed by Pi's MCP integration. If Pi's MCP support changes, prefer invoking Pi's documented adapter commands rather than guessing file structures.

The UI should show which agents each MCP server is enabled for and whether Halo can verify it.

### Halo Broker

Halo should include a local broker service in a later MVP phase. The broker exposes a local MCP server and optional HTTP/IPC APIs so one agent can call another through Halo.

Example tools:

- `ask_codex`
- `ask_claude`
- `ask_opencode`
- `ask_pi`
- `handoff_task`
- `summarize_session`
- `list_active_agents`

The broker should pass summarized context rather than raw terminal logs by default. Full transcript sharing should require explicit user confirmation.

## Vendor Integration Notes

### OpenCode

OpenCode has its own configuration and MCP concepts, and should be treated as more than a generic shell command. Halo should detect OpenCode configuration files, respect layered config behavior, and avoid removing unknown fields.

First release:

- Launch OpenCode in terminal mode.
- Read and patch OpenCode project/global config.
- Manage OpenCode MCP entries.
- Offer OpenCode command presets.

Later:

- Surface OpenCode agent modes in the UI if stable.
- Add deeper session metadata if OpenCode exposes a reliable local API.

### Pi

Pi is the best candidate for an early native adapter because its docs describe an RPC mode over JSONL. Halo should support terminal mode first, then add RPC mode for richer UI interactions.

First release:

- Launch Pi in terminal mode.
- Detect Pi installation and version.
- Provide command presets.
- Manage MCP configuration through documented Pi paths and commands.

Later:

- Add RPC-backed native chat tabs.
- Render structured status, tool calls, and message events.

### Codex CLI

Codex CLI should be integrated through terminal mode and config management first. The important early feature is safe MCP and profile management through `config.toml`.

First release:

- Launch Codex CLI in terminal mode.
- Read and patch `~/.codex/config.toml` and project `.codex/config.toml` when present.
- Manage MCP server entries.
- Expose common model, sandbox, approval, and workspace presets where supported.

Later:

- Add support for Codex MCP server mode if it becomes stable for cross-agent orchestration.

### Claude Code

Claude Code needs careful config handling because it uses multiple config locations and scopes. Halo should treat account/profile switching as a sensitive feature and build backup/rollback from the beginning.

First release:

- Launch Claude Code in terminal mode.
- Manage command presets and MCP entries.
- Read and patch project `.mcp.json` and `.claude/settings*.json` files where appropriate.
- Support profile switching with backups.

Later:

- Add richer cc-switch-like profile snapshots.
- Add import helpers for existing Claude Code providers, commands, and skills.

## Data Storage

Halo should use SQLite for local app state.

Suggested tables:

- `workspaces`: known project roots and display metadata.
- `agents`: detected agent installations and versions.
- `profiles`: named runtime/config profiles.
- `profile_agents`: per-agent profile settings.
- `mcp_servers`: normalized MCP registry.
- `sessions`: terminal/native session records.
- `config_snapshots`: backup metadata and restore pointers.
- `command_presets`: reusable prompts and launch commands.
- `audit_events`: config writes, restore actions, and sensitive operations without secret values.

Large logs and backups should live as files, with SQLite storing paths and metadata.

## UI Information Architecture

### Main Workbench

The default view is not a landing page. It opens directly into the developer workbench.

Primary zones:

- Left rail: workspace selector, agent list, profiles, quick launch.
- Center tabs: terminal sessions, future native chats, diff previews.
- Right inspector: selected session details, MCP server status, config panel, command presets.
- Command palette: search actions, launch commands, switch profiles, add MCP server.

### Configuration Center

The configuration center should have tabs:

- Profiles
- Agents
- MCP
- Credentials
- Backups
- Diagnostics

Each config write should show:

- target agent
- target file
- scope
- generated diff
- backup path
- validation status

### Visual Style

The UI should be modern but utilitarian. It should avoid oversized hero sections and decorative marketing layouts. Use compact panels, crisp separators, icons for tools, tabs for modes, toggles for enabled states, and menus for agent/profile choices.

The palette should avoid becoming a single-hue purple or slate app. A good starting palette is:

- neutral charcoal backgrounds
- white and zinc text
- cyan for active runtime state
- amber for pending config changes
- green for verified writes
- red for destructive or failed actions
- subtle violet only as a secondary accent

## MVP Scope

### Phase 0: Technical Spike

Goal: prove the desktop shell can reliably host Windows CLI agents.

Deliverables:

- Electron app scaffold.
- PTY-backed terminal panel.
- Agent detection for four CLIs.
- Manual launch of each available CLI.
- Ctrl+C, resize, restart, and stop behavior.

Exit criteria:

- At least one agent can run interactively in the app.
- Missing agents show clear install/setup states.
- PTY behavior is reliable enough for daily testing.

### Phase 1: Workbench MVP

Goal: make Halo useful as a daily multi-agent launcher.

Deliverables:

- Workspace selector.
- Agent switcher.
- Multi-tab terminal sessions.
- Command presets.
- Local SQLite app state.
- Basic settings screen.

Exit criteria:

- User can open a project, launch agents, switch sessions, and reuse prompts.

### Phase 2: Config and MCP Center

Goal: unify the painful parts of multi-agent setup.

Deliverables:

- Normalized config model.
- JSON/JSONC/TOML parsing and patching.
- MCP registry UI.
- Vendor-specific MCP writers.
- Diff preview.
- Backup and rollback.
- Credential storage integration.

Exit criteria:

- User can add one MCP server and enable it for supported agents without manually editing files.

### Phase 3: Profile Switching

Goal: provide cc-switch-like profile power across all supported agents.

Deliverables:

- Named profiles.
- Per-agent launch env and config patches.
- Secret-backed environment variables.
- Quick switch from tray or command palette.
- Config snapshot history.

Exit criteria:

- User can switch between at least two profiles and safely roll back.

### Phase 4: Broker and Native Integrations

Goal: make Halo more than a terminal multiplexer.

Deliverables:

- Local Halo Broker MCP server.
- Agent handoff tools.
- Pi RPC native chat adapter.
- Session summarization.
- Cross-agent task handoff UI.

Exit criteria:

- One agent can request a summarized handoff to another agent through Halo-controlled tooling.

## Testing Strategy

The implementation should start with automated tests around the risky local services before polishing UI details.

Required early tests:

- Config parser preserves unknown fields.
- TOML writer generates expected Codex MCP config.
- JSON/JSONC writer generates expected OpenCode and Claude MCP config.
- Backup files are created before writes.
- Rollback restores previous content.
- Adapter registry reports missing CLIs without crashing.
- PTY session lifecycle handles start, stop, and restart states.

UI tests should cover:

- agent detection states
- session tab creation
- MCP form validation
- diff preview before write
- credential masking

Manual Windows verification is required for PTY behavior, Ctrl+C, terminal resize, and real CLI interaction.

## Security and Safety

Sensitive operations:

- writing user-level config files
- switching credentials
- exporting profiles
- running arbitrary agent commands
- exposing broker tools to agents

Safety requirements:

- No secrets in renderer logs.
- No secrets in SQLite.
- No config write without backup.
- No broker tool that can run arbitrary shell commands in the first release.
- Cross-agent handoff should share summarized context by default.
- Full transcript sharing requires explicit confirmation.
- Config exports exclude secrets unless the user opts in.

## Open Questions

1. Whether to support only PowerShell-compatible environments in the first release, or also Git Bash and WSL shells.
2. Whether profile switching should patch vendor files immediately or apply overlays only when launching sessions.
3. Whether the first UI theme should optimize for dark mode only or include light mode from day one.
4. Whether the first release should include an installer or remain a developer-run app during early testing.

Recommended first decisions:

- Support PowerShell first.
- Patch project config files only after diff approval.
- Support dark mode first.
- Use developer-run app until the PTY and config flows stabilize.

## Implementation Recommendation

Start with the smallest vertical slice:

1. Scaffold Electron + React + TypeScript.
2. Build the desktop workbench shell.
3. Add PTY terminal hosting.
4. Add an adapter registry with mocked detection tests.
5. Implement real detection and launch for one CLI.
6. Extend detection to all four CLIs.
7. Add SQLite.
8. Add normalized MCP registry.
9. Add one complete MCP writer, preferably Codex because TOML output is explicit and easy to test.
10. Add Claude, OpenCode, and Pi MCP writers.

This route produces a usable app early while protecting the harder config and broker work behind tested service boundaries.
