import { EventEmitter } from "node:events";
import { describe, expect, it } from "vitest";
import { detectPi, resolvePiExecutables, type PiExecutableFilesystem, type ProcessFactory } from "./detect.js";
import type { ProcessPort } from "./jsonlTransport.js";
import { PiRuntime } from "./runtime.js";

class FakeFilesystem implements PiExecutableFilesystem {
  readonly #realpaths = new Map<string, string>();
  readonly #files = new Set<string>();
  readonly #contents = new Map<string, string>();
  readonly readPaths: string[] = [];

  addDirectory(input: string, canonical = input): void {
    this.#realpaths.set(input, canonical);
    this.#realpaths.set(canonical, canonical);
  }

  addFile(input: string, contents = "", canonical = input): void {
    this.addDirectory(input, canonical);
    this.#files.add(input);
    this.#files.add(canonical);
    this.#contents.set(input, contents);
    this.#contents.set(canonical, contents);
  }

  async stat(file: string): Promise<{ readonly isFile: () => boolean }> {
    if (!this.#files.has(file)) throw new Error(`Missing file: ${file}`);
    return { isFile: () => true };
  }

  async realpath(file: string): Promise<string> {
    const resolved = this.#realpaths.get(file);
    if (resolved === undefined) throw new Error(`Missing path: ${file}`);
    return resolved;
  }

  async readFile(file: string): Promise<string> {
    this.readPaths.push(file);
    const contents = this.#contents.get(file);
    if (contents === undefined) throw new Error(`Missing file: ${file}`);
    return contents;
  }
}

function versionPort(output: string): ProcessPort {
  const stdout = new EventEmitter();
  const stderr = new EventEmitter();
  queueMicrotask(() => {
    stdout.emit("data", output);
    stdout.emit("end");
    stderr.emit("end");
  });
  return {
    stdin: { write: () => undefined, end: () => undefined },
    stdout,
    stderr,
    wait: async () => ({ code: 0, signal: null }),
  };
}

function rpcPort(): ProcessPort {
  const stdout = new EventEmitter();
  const stderr = new EventEmitter();
  return {
    stdin: {
      write: (value) => {
        const command = JSON.parse(value) as { id: string; type: string };
        if (command.type === "get_state") {
          queueMicrotask(() => stdout.emit("data", JSON.stringify({
            type: "response",
            id: command.id,
            command: "get_state",
            success: true,
            data: {},
          }) + "\n"));
        }
      },
      end: () => undefined,
    },
    stdout,
    stderr,
    wait: async () => ({ code: 0, signal: null }),
  };
}

function createNpmPiFilesystem(manifest?: string): {
  readonly filesystem: FakeFilesystem;
  readonly workspace: string;
  readonly shim: string;
  readonly node: string;
  readonly entrypoint: string;
  readonly packageJson: string;
} {
  const filesystem = new FakeFilesystem();
  const workspace = "C:\\workspace";
  const npmBin = "C:\\host-npm";
  const npmRoot = `${npmBin}\\node_modules`;
  const packageRoot = `${npmRoot}\\@earendil-works\\pi-coding-agent`;
  const shim = `${npmBin}\\pi.cmd`;
  const node = `${npmBin}\\node.exe`;
  const packageJson = `${packageRoot}\\package.json`;
  const entrypoint = `${packageRoot}\\dist\\cli.js`;
  filesystem.addDirectory(workspace);
  filesystem.addDirectory(npmBin);
  filesystem.addDirectory(npmRoot);
  filesystem.addDirectory(`${npmRoot}\\@earendil-works`);
  filesystem.addDirectory(packageRoot);
  filesystem.addDirectory(`${packageRoot}\\dist`);
  filesystem.addFile(shim, "@echo malicious batch text must never run");
  filesystem.addFile(node);
  filesystem.addFile(
    packageJson,
    manifest ?? JSON.stringify({
      name: "@earendil-works/pi-coding-agent",
      version: "0.81.1",
      bin: { pi: "dist/cli.js" },
    }),
  );
  filesystem.addFile(entrypoint, "console.log('pi');");
  return { filesystem, workspace, shim, node, entrypoint, packageJson };
}

describe("Pi executable resolution", () => {
  it("uses only canonical host PATH executables outside the workspace", async () => {
    const filesystem = new FakeFilesystem();
    filesystem.addDirectory("C:\\workspace");
    filesystem.addFile("C:\\workspace\\pi.exe");
    filesystem.addFile("C:\\host-bin\\pi.exe");

    await expect(resolvePiExecutables("pi.exe", {
      cwd: "C:\\workspace",
      environment: { PATH: "C:\\workspace;C:\\host-bin;." },
      filesystem,
      platform: "win32",
    })).resolves.toEqual(["C:\\host-bin\\pi.exe"]);
  });

  it("rejects a host PATH file whose canonical path points into the workspace", async () => {
    const filesystem = new FakeFilesystem();
    filesystem.addDirectory("C:\\workspace");
    filesystem.addFile("C:\\workspace\\pi.exe");
    filesystem.addFile("C:\\host-link\\pi.exe", "", "C:\\workspace\\pi.exe");

    await expect(resolvePiExecutables("pi.exe", {
      cwd: "C:\\workspace",
      environment: { PATH: "C:\\host-link" },
      filesystem,
      platform: "win32",
    })).resolves.toEqual([]);
  });

  it("rejects relative PATH entries without probing them", async () => {
    const filesystem = new FakeFilesystem();
    filesystem.addDirectory("C:\\workspace");
    filesystem.addFile("pi.exe");

    await expect(resolvePiExecutables("pi.exe", {
      cwd: "C:\\workspace",
      environment: { PATH: "." },
      filesystem,
      platform: "win32",
    })).resolves.toEqual([]);
  });
});

describe("Pi npm shim resolution", () => {
  it("uses a verified npm package through node.exe without executing pi.cmd", async () => {
    const { filesystem, workspace, shim, node, entrypoint, packageJson } = createNpmPiFilesystem();
    const launches: Array<{ readonly executable: string; readonly args: readonly string[] }> = [];
    const factory: ProcessFactory = (executable, args) => {
      launches.push({ executable, args });
      if (args.length === 1 && args[0] === "--version") return versionPort("v22.19.0\n");
      if (args[0] === entrypoint && args[1] === "--version") return versionPort("pi 0.81.1\n");
      throw new Error("Unexpected probe command");
    };

    const detection = await detectPi({
      cwd: workspace,
      hostEnvironment: { PATH: "C:\\host-npm" },
      filesystem,
      platform: "win32",
      processFactory: factory,
    });

    expect(detection).toMatchObject({
      status: "detected",
      executable: node,
      version: "0.81.1",
      launch: { executable: node, argvPrefix: [entrypoint], displayPath: shim },
    });
    expect(launches).toEqual([
      { executable: node, args: ["--version"] },
      { executable: node, args: [entrypoint, "--version"] },
    ]);
    expect(launches.every(({ executable }) => executable !== shim)).toBe(true);
    expect(filesystem.readPaths).toEqual([packageJson]);

    const rpcLaunches: Array<{ readonly executable: string; readonly args: readonly string[] }> = [];
    const runtime = new PiRuntime({
      detection,
      detect: async () => detection,
      spawn: (executable, args) => {
        rpcLaunches.push({ executable, args });
        return rpcPort();
      },
      cwd: workspace,
      session: "session",
      model: "model",
      thinking: "medium",
      trust: "trusted",
      hostEnvironment: { PATH: "C:\\host-npm" },
    });
    await runtime.start();
    await runtime.stop();

    expect(rpcLaunches).toEqual([{
      executable: node,
      args: [
        entrypoint,
        "--mode",
        "rpc",
        "--session-id",
        "session",
        "--model",
        "model",
        "--thinking",
        "medium",
        "--approve",
      ],
    }]);
  });

  it("fails closed before spawning Node for an invalid official package manifest", async () => {
    const { filesystem, workspace } = createNpmPiFilesystem(JSON.stringify({
      name: "@attacker/pi-coding-agent",
      version: "0.81.1",
      bin: { pi: "dist/cli.js" },
    }));
    const launches: string[] = [];

    await expect(detectPi({
      cwd: workspace,
      hostEnvironment: { PATH: "C:\\host-npm" },
      filesystem,
      platform: "win32",
      processFactory: (executable) => {
        launches.push(executable);
        return versionPort("v22.19.0\n");
      },
    })).resolves.toMatchObject({ status: "unavailable" });
    expect(launches).toEqual([]);
  });

  it("fails closed when the package entrypoint canonicalizes into the workspace", async () => {
    const { filesystem, workspace, entrypoint } = createNpmPiFilesystem();
    filesystem.addFile(entrypoint, "", `${workspace}\\cli.js`);
    const launches: string[] = [];

    await expect(detectPi({
      cwd: workspace,
      hostEnvironment: { PATH: "C:\\host-npm" },
      filesystem,
      platform: "win32",
      processFactory: (executable) => {
        launches.push(executable);
        return versionPort("v22.19.0\n");
      },
    })).resolves.toMatchObject({ status: "unavailable" });
    expect(launches).toEqual([]);
  });

  it("rejects a Node interpreter below Pi's declared minimum", async () => {
    const { filesystem, workspace, node } = createNpmPiFilesystem();
    const launches: Array<{ readonly executable: string; readonly args: readonly string[] }> = [];

    await expect(detectPi({
      cwd: workspace,
      hostEnvironment: { PATH: "C:\\host-npm" },
      filesystem,
      platform: "win32",
      processFactory: (executable, args) => {
        launches.push({ executable, args });
        return versionPort("v22.18.0\n");
      },
    })).resolves.toMatchObject({ status: "unavailable" });
    expect(launches).toEqual([{ executable: node, args: ["--version"] }]);
  });
});
