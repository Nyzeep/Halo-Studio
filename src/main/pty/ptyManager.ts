import { randomUUID } from "node:crypto";
import os from "node:os";
import path from "node:path";
import { execFile } from "node:child_process";
import type { StartSessionRequest } from "../../shared/api.js";
import type { AgentId, TerminalSessionInfo } from "../../shared/agents.js";

interface PtyManagerEvents {
  onData(sessionId: string, data: string): void;
  onExit(sessionId: string, exitCode: number | null): void;
}

interface IPtyMinimal {
  onData(callback: (data: string) => void): void;
  onExit(callback: (event: { exitCode: number }) => void): any;
  write(data: string): void;
  resize(cols: number, rows: number): void;
  kill(): void;
}

interface SessionRecord {
  info: TerminalSessionInfo;
  process: IPtyMinimal;
}

const commandByAgent: Record<AgentId, string> = {
  "claude-code": "claude",
  "codex-cli": "codex",
  opencode: "opencode",
  pi: "pi"
};

class MockPty implements IPtyMinimal {
  private dataCallback?: (data: string) => void;
  private exitCallback?: (event: { exitCode: number }) => void;
  private currentInput = "";
  private shellPrompt: string;
  private agentId: AgentId;

  constructor(agentId: AgentId) {
    this.agentId = agentId;
    this.shellPrompt = `${commandByAgent[agentId]} > `;

    setTimeout(() => {
      this.sendWelcome();
    }, 100);
  }

  onData(callback: (data: string) => void) {
    this.dataCallback = callback;
  }

  onExit(callback: (event: { exitCode: number }) => void) {
    this.exitCallback = callback;
  }

  private send(data: string) {
    if (this.dataCallback) {
      this.dataCallback(data);
    }
  }

  private sendWelcome() {
    const banners: Record<string, string[]> = {
      "claude-code": [
        "\r\n",
        "\x1b[1;35m   ______  __                       __         ______             __     \x1b[0m\r\n",
        "\x1b[1;35m  /      |/  |                     /  |       /      |           /  |    \x1b[0m\r\n",
        "\x1b[1;35m /$$$$$$/ $$ |  ______   __    __  $$ |  ______/$$$$$$/   ______  $$ |  ____  \x1b[0m\r\n",
        "\x1b[1;35m $$ |    _$$ | /$$__  | /  |  /  | $$ | /$$__  $$ |     /$$__  | $$ | /    | \x1b[0m\r\n",
        "\x1b[1;35m $$ |   / $$ | $$ |  $$ |$$ |  $$ | $$ | $$    $$ |     $$ |  $$ |$$ |/$$$$ | \x1b[0m\r\n",
        "\x1b[1;35m $$ \\__/$$$$ | $$ \\__$$ |$$ \\__$$ | $$ | $$$$$$$$ \\__/$$ $$ \\__$$ |$$ |$$ |$$ | \x1b[0m\r\n",
        "\x1b[1;35m  $$    $$/$$|  $$    $$/  $$    $$/  $$ |  $$    $$/$$|  $$    $$/  $$ |$$ |$$ | \x1b[0m\r\n",
        "\x1b[1;35m   $$$$$$/  /    $$$$$$/    $$$$$$/   $$/    $$$$$$/  /    $$$$$$/   $$/ $$/ $$/  \x1b[0m\r\n",
        "\r\n",
        "  \x1b[1;32mClaude Code CLI v0.1.0 - [AI Studio Demo Mode]\x1b[0m\r\n",
        "  --------------------------------------------------\r\n",
        "  A beautiful sandbox environment has been provisioned.\r\n",
        "  Feel free to explore the Workspace, Profiles, or MCP registries.\r\n",
        "\r\n",
        "  Available commands:\r\n",
        "    \x1b[36mhelp\x1b[0m       - Show this command list\r\n",
        "    \x1b[36mstatus\x1b[0m     - Check connected MCP servers & workspace info\r\n",
        "    \x1b[36mwhoami\x1b[0m     - Show active profile and user details\r\n",
        "    \x1b[36mclear\x1b[0m      - Clear screen\r\n",
        "    \x1b[36mexit\x1b[0m       - Quit session\r\n",
        "\r\n"
      ],
      "codex-cli": [
        "\r\n",
        "  \x1b[1;34m=== Codex CLI v1.0.0 ===\x1b[0m\r\n",
        "  [Demo Shell] - Powered by OpenAI Codex\r\n",
        "  ---------------------------------------\r\n",
        "  Type 'help' to see what you can do!\r\n",
        "\r\n"
      ],
      "opencode": [
        "\r\n",
        "  \x1b[1;32m=== OpenCode Agent Terminal ===\x1b[0m\r\n",
        "  [Demo Shell] - Open-source developer partner\r\n",
        "  ---------------------------------------------\r\n",
        "  Type 'help' to list commands.\r\n",
        "\r\n"
      ],
      "pi": [
        "\r\n",
        "  \x1b[1;36m=== Pi Agent Terminal ===\x1b[0m\r\n",
        "  [Demo Shell] - Earendil Works\r\n",
        "  -----------------------------\r\n",
        "  Type 'help' to list commands.\r\n",
        "\r\n"
      ]
    };

    const lines = banners[this.agentId] || ["Welcome!"];
    for (const line of lines) {
      this.send(line);
    }
    this.send(`\r\n${this.shellPrompt}`);
  }

  write(data: string) {
    for (let i = 0; i < data.length; i++) {
      const char = data[i];
      if (char === "\r" || char === "\n") {
        this.send("\r\n");
        this.handleCommand(this.currentInput.trim());
        this.currentInput = "";
      } else if (char === "\x7f" || char === "\x08") {
        if (this.currentInput.length > 0) {
          this.currentInput = this.currentInput.slice(0, -1);
          this.send("\b \b");
        }
      } else if (char === "\x03") {
        this.send("^C\r\n");
        this.currentInput = "";
        this.send(this.shellPrompt);
      } else if (char.charCodeAt(0) >= 32 && char.charCodeAt(0) <= 126) {
        this.currentInput += char;
        this.send(char);
      }
    }
  }

  private handleCommand(cmd: string) {
    if (cmd === "") {
      this.send(this.shellPrompt);
      return;
    }

    if (cmd.toLowerCase() === "help") {
      this.send("\r\n\x1b[1;33mSimulated Commands:\x1b[0m\r\n");
      this.send("  help         - Show help details\r\n");
      this.send("  status       - Inspect local MCP endpoints\r\n");
      this.send("  whoami       - Active user/profile details\r\n");
      this.send("  clear        - Clear the terminal screen\r\n");
      this.send("  exit         - Terminate session\r\n");
      this.send("\r\n  Feel free to write any prompt to simulate agent action!\r\n");
    } else if (cmd.toLowerCase() === "status") {
      this.send("\r\n\x1b[1;32m[Halo Studio Local Agent Status]\x1b[0m\r\n");
      this.send("  Agent ID:       " + this.agentId + "\r\n");
      this.send("  Mode:           Simulated Sandbox (Terminal Mode)\r\n");
      this.send("  Workspace:      D:\\Halo Studio (Local Workdir)\r\n");
      this.send("  Pty status:     Ready (Listening on WebSocket)\r\n");
      this.send("  MCP connectors: ~/.mcp.json, ~/.codex/config.toml\r\n");
    } else if (cmd.toLowerCase() === "whoami") {
      this.send("\r\n\x1b[1;34m[Active profile]\x1b[0m\r\n");
      this.send("  User Email:     kiepeifortest@gmail.com\r\n");
      this.send("  Active Profile: default-profile\r\n");
      this.send("  Platform:       Web/Docker Sandbox Mode\r\n");
    } else if (cmd.toLowerCase() === "clear") {
      this.send("\x1b[2J\x1b[H");
    } else if (cmd.toLowerCase() === "exit") {
      this.send("Exiting session...\r\n");
      if (this.exitCallback) {
        this.exitCallback({ exitCode: 0 });
      }
      return;
    } else {
      this.send("\r\n\x1b[5mThinking...\x1b[0m\r\n");
      setTimeout(() => {
        this.send("\x1b[1A\x1b[2K");
        this.send(`\x1b[1;32m[${this.agentId} Response]\x1b[0m\r\n`);
        this.send(`I received your instruction: "${cmd}".\r\n`);
        this.send("As I am running in a secure, local-first sandbox container in AI Studio,\r\n");
        this.send("I can help you preview config write-back actions and MCP server bindings.\r\n");
        this.send("Please interact with the \x1b[35m'MCP Config Preview'\x1b[0m and \x1b[36m'Inspector Panel'\x1b[0m\r\n");
        this.send("tabs on your right to verify config edits or plan real file write actions!\r\n\r\n");
        this.send(this.shellPrompt);
      }, 600);
      return;
    }

    this.send(this.shellPrompt);
  }

  resize(cols: number, rows: number) {}

  kill() {
    if (this.exitCallback) {
      this.exitCallback({ exitCode: 0 });
    }
  }
}

export class PtyManager {
  private readonly sessions = new Map<string, SessionRecord>();
  private ptyModule: any = null;
  private ptyLoaded = false;

  constructor(private readonly events: PtyManagerEvents) {
    // Attempt dynamic import of node-pty
    import("node-pty")
      .then((mod) => {
        this.ptyModule = mod;
        this.ptyLoaded = true;
        console.log("Successfully loaded node-pty binary module.");
      })
      .catch((err) => {
        console.warn("Could not load node-pty binary, falling back to MockPty terminal emulation.");
      });
  }

  list(): TerminalSessionInfo[] {
    return Array.from(this.sessions.values()).map((session) => session.info);
  }

  private async commandExists(command: string): Promise<boolean> {
    const locator = process.platform === "win32" ? "where.exe" : "which";
    return new Promise((resolve) => {
      execFile(locator, [command], { windowsHide: true }, (error) => {
        resolve(!error);
      });
    });
  }

  async start(request: StartSessionRequest): Promise<TerminalSessionInfo> {
    const sessionId = randomUUID();
    const shell = commandByAgent[request.agentId] || request.agentId;
    const cwd = path.resolve(request.cwd || os.homedir());

    let child: IPtyMinimal;
    let mode: "real" | "mock" = "mock";

    const hasCommand = await this.commandExists(shell);

    if (this.ptyLoaded && this.ptyModule && hasCommand) {
      try {
        child = this.ptyModule.spawn(shell, [], {
          name: "xterm-256color",
          cols: 100,
          rows: 32,
          cwd,
          env: { ...process.env }
        });
        mode = "real";
      } catch (err) {
        console.warn("Failed to spawn real PTY command, falling back to mock shell:", err);
        child = new MockPty(request.agentId);
      }
    } else {
      child = new MockPty(request.agentId);
    }

    const info: TerminalSessionInfo = {
      id: sessionId,
      agentId: request.agentId,
      title: shell,
      cwd,
      status: "running",
      createdAt: new Date().toISOString()
    };

    this.sessions.set(sessionId, { info, process: child });

    child.onData((data) => this.events.onData(sessionId, data));
    child.onExit(({ exitCode }) => {
      this.sessions.delete(sessionId);
      this.events.onExit(sessionId, exitCode);
    });

    return info;
  }

  write(sessionId: string, data: string): void {
    this.sessions.get(sessionId)?.process.write(data);
  }

  resize(sessionId: string, cols: number, rows: number): void {
    this.sessions.get(sessionId)?.process.resize(cols, rows);
  }

  stop(sessionId: string): void {
    this.sessions.get(sessionId)?.process.kill();
    this.sessions.delete(sessionId);
  }
}
