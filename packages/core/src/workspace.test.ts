import { constants } from "node:fs";
import {
  mkdtemp,
  mkdir,
  realpath,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join, relative, resolve } from "node:path";
import { types as utilTypes } from "node:util";

import { workspaceSchema, type AgentKind } from "@halo-studio/contracts";
import { afterEach, describe, expect, it, vi } from "vitest";

import { CoreError } from "./error.js";
import {
  isPathWithin,
  normalizePathKey,
  workspaceIdForPath,
} from "./pathPolicy.js";
import {
  MemoryTrustStore,
  mergeRuntimeEnvironment,
  resolveTrust,
  runtimeTrustPolicy,
  type TrustDecision,
  type TrustStore,
} from "./trust.js";
import { openWorkspace, type FsPort } from "./workspace.js";

const temporaryRoots: string[] = [];

async function temporaryDirectory(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "halo core 中文 "));
  temporaryRoots.push(root);
  return root;
}

type DirectoryLinkKind = "dir" | "junction";
type DirectoryLinkCreator = (
  target: string,
  link: string,
  kind: DirectoryLinkKind,
) => Promise<void>;

interface TestDirectoryLinkOptions {
  readonly createLink: DirectoryLinkCreator;
  readonly onSkip: (reason: string) => void;
  readonly platform: string;
}

function windowsLinkPermissionCode(error: unknown): string | undefined {
  if (
    error === null ||
    typeof error !== "object" ||
    utilTypes.isProxy(error)
  ) {
    return undefined;
  }

  const descriptor = Object.getOwnPropertyDescriptor(error, "code");
  if (descriptor === undefined || !("value" in descriptor)) {
    return undefined;
  }
  return descriptor.value === "EPERM" || descriptor.value === "EACCES"
    ? descriptor.value
    : undefined;
}

async function createDirectoryLinkForTest(
  target: string,
  link: string,
  options: TestDirectoryLinkOptions,
): Promise<boolean> {
  try {
    await options.createLink(
      target,
      link,
      options.platform === "win32" ? "junction" : "dir",
    );
    return true;
  } catch (error) {
    const permissionCode =
      options.platform === "win32"
        ? windowsLinkPermissionCode(error)
        : undefined;
    if (permissionCode === undefined) {
      throw error;
    }
    options.onSkip(
      `Skipping Windows directory-link test because link creation was denied (${permissionCode}).`,
    );
    return false;
  }
}

async function captureError(action: () => Promise<unknown>): Promise<unknown> {
  try {
    await action();
  } catch (error) {
    return error;
  }
  return undefined;
}

function trustStoreWith(decisions: readonly TrustDecision[]): TrustStore {
  return {
    listDecisions: vi.fn().mockResolvedValue(decisions),
    setDecision: vi.fn(),
  };
}

afterEach(async () => {
  await Promise.all(
    temporaryRoots.splice(0).map((root) =>
      rm(root, { force: true, recursive: true }),
    ),
  );
});

describe("path policy", () => {
  it.runIf(process.platform === "win32")(
    "keeps canonical NTFS names distinct when JavaScript case folding collides",
    async () => {
      const parent = await temporaryDirectory();
      const asciiK = join(parent, "K");
      const kelvinSign = join(parent, "\u212A");
      await mkdir(asciiK);
      await mkdir(kelvinSign);

      const canonicalAsciiK = await realpath(asciiK);
      const canonicalKelvinSign = await realpath(kelvinSign);
      const store = new MemoryTrustStore("win32");
      await store.setDecision(canonicalAsciiK, "trusted");

      expect(workspaceIdForPath(canonicalAsciiK, "win32")).not.toBe(
        workspaceIdForPath(canonicalKelvinSign, "win32"),
      );
      await expect(
        resolveTrust(canonicalKelvinSign, store, "win32"),
      ).resolves.toBe("untrusted");
      expect(
        isPathWithin(canonicalKelvinSign, canonicalAsciiK, "win32"),
      ).toBe(false);
    },
  );

  it.runIf(process.platform === "win32")(
    "converges alternate input casing through filesystem canonicalization",
    async () => {
      const parent = await temporaryDirectory();
      const target = join(parent, "CanonicalCase");
      await mkdir(target);

      const canonicalExact = await realpath(target);
      const canonicalAlternate = await realpath(target.toLowerCase());

      expect(canonicalAlternate).toBe(canonicalExact);
      expect(workspaceIdForPath(canonicalAlternate, "win32")).toBe(
        workspaceIdForPath(canonicalExact, "win32"),
      );
    },
  );

  it("normalizes comparison keys without removing a filesystem root", () => {
    expect(normalizePathKey("C:\\Work\\Project\\", "win32")).toBe(
      "C:\\Work\\Project",
    );
    expect(normalizePathKey("C:\\", "win32")).toBe("C:\\");
    expect(normalizePathKey("/srv/project///", "linux")).toBe("/srv/project");
    expect(normalizePathKey("/", "linux")).toBe("/");
  });

  it("uses path segments for ancestor checks", () => {
    expect(isPathWithin("C:\\foo\\bar", "C:\\foo", "win32")).toBe(true);
    expect(isPathWithin("c:\\FOO\\bar", "C:\\foo", "win32")).toBe(false);
    expect(isPathWithin("C:\\foo-bar", "C:\\foo", "win32")).toBe(false);
    expect(isPathWithin("/srv/foo-bar", "/srv/foo", "linux")).toBe(false);
    expect(isPathWithin("/srv/project", "/", "linux")).toBe(true);
    expect(isPathWithin("D:\\project", "C:\\", "win32")).toBe(false);
    expect(
      isPathWithin(
        "\\\\server\\share\\project",
        "\\\\server\\share\\",
        "win32",
      ),
    ).toBe(true);
    expect(
      isPathWithin(
        "\\\\server\\share\\project",
        "\\\\SERVER\\share\\",
        "win32",
      ),
    ).toBe(false);
  });

  it("derives SHA-256 ids from platform-specific canonical keys", () => {
    const windowsUpper = workspaceIdForPath("C:\\Work\\Project", "win32");
    const windowsLower = workspaceIdForPath("c:\\work\\project", "win32");
    const unixUpper = workspaceIdForPath("/Work/Project", "linux");
    const unixLower = workspaceIdForPath("/work/project", "linux");

    expect(windowsUpper).toMatch(/^[0-9a-f]{64}$/u);
    expect(windowsUpper).not.toBe(windowsLower);
    expect(unixUpper).not.toBe(unixLower);
  });
});

describe("workspace opening", () => {
  it("opens relative and absolute paths as canonical contract workspaces", async () => {
    const parent = await temporaryDirectory();
    const target = join(parent, "project 项目");
    await mkdir(target);
    const store = new MemoryTrustStore();
    await store.setDecision(target, "trusted");

    const fromRelative = await openWorkspace(relative(parent, target), store, {
      cwd: parent,
    });
    const fromAbsolute = await openWorkspace(`${target}${process.platform === "win32" ? "\\" : "/"}`, store);

    expect(fromRelative).toEqual(fromAbsolute);
    expect(fromRelative.rootPath).toBe(resolve(target));
    expect(fromRelative.realPath).toBe(resolve(target));
    expect(fromRelative.trustState).toBe("trusted");
    expect(workspaceSchema.safeParse(fromRelative).success).toBe(true);
  });

  it("rejects missing paths and regular files with UnsafePath", async () => {
    const parent = await temporaryDirectory();
    const file = join(parent, "not-a-directory.txt");
    await writeFile(file, "data", "utf8");
    const store = new MemoryTrustStore();

    await expect(openWorkspace(join(parent, "missing"), store)).rejects.toMatchObject({
      code: "UnsafePath",
    });
    await expect(openWorkspace(file, store)).rejects.toMatchObject({
      code: "UnsafePath",
    });
  });

  it("maps inaccessible filesystem operations to UnsafePath", async () => {
    const parent = await temporaryDirectory();
    const permissionError = Object.assign(new Error("denied"), { code: "EACCES" });
    const fsPort: FsPort = {
      access: vi.fn().mockRejectedValue(permissionError),
      realpath: vi.fn().mockResolvedValue(parent),
      stat: vi.fn().mockResolvedValue({ isDirectory: () => true }),
    };

    await expect(
      openWorkspace(parent, new MemoryTrustStore(), { fs: fsPort }),
    ).rejects.toMatchObject({ code: "UnsafePath" });
  });

  it("sanitizes CoreError failures from filesystem dependencies", async () => {
    const parent = await temporaryDirectory();
    const fsPort: FsPort = {
      access: vi.fn(),
      realpath: vi
        .fn()
        .mockRejectedValue(
          new CoreError("ProtocolViolation", "filesystem-core-canary-secret"),
        ),
      stat: vi.fn(),
    };

    const thrown = await captureError(() =>
      openWorkspace(parent, new MemoryTrustStore(), { fs: fsPort }),
    );

    expect(thrown).toMatchObject({
      code: "UnsafePath",
      message: "Workspace path is unavailable.",
    });
    expect(String(thrown)).not.toContain("filesystem-core-canary-secret");
  });

  it("validates directory access on the canonical real path", async () => {
    const parent = await temporaryDirectory();
    const linkPath = join(parent, "selected link");
    const targetPath = join(parent, "canonical target");
    const access = vi.fn().mockResolvedValue(undefined);
    const stat = vi.fn().mockResolvedValue({ isDirectory: () => true });
    const fsPort: FsPort = {
      access,
      realpath: vi.fn().mockResolvedValue(targetPath),
      stat,
    };

    await openWorkspace(linkPath, new MemoryTrustStore(), { fs: fsPort });

    expect(stat).toHaveBeenCalledWith(targetPath);
    expect(access).toHaveBeenCalledWith(
      targetPath,
      constants.R_OK | constants.X_OK,
    );
  });

  it("maps trust-store failures to ProtocolViolation without leaking details", async () => {
    const parent = await temporaryDirectory();
    const trustStore = {
      listDecisions: vi
        .fn()
        .mockRejectedValue(new Error("trust-storage-canary-secret")),
      setDecision: vi.fn(),
    };

    const thrown = await captureError(() => openWorkspace(parent, trustStore));

    expect(thrown).toMatchObject({ code: "ProtocolViolation" });
    expect(String(thrown)).not.toContain("trust-storage-canary-secret");
  });

  it("sanitizes CoreError failures from trust-store dependencies", async () => {
    const parent = await temporaryDirectory();
    const trustStore = {
      listDecisions: vi
        .fn()
        .mockRejectedValue(
          new CoreError("UnsafePath", "trust-core-canary-secret"),
        ),
      setDecision: vi.fn(),
    };

    const thrown = await captureError(() => openWorkspace(parent, trustStore));

    expect(thrown).toMatchObject({
      code: "ProtocolViolation",
      message: "Workspace trust decision is unavailable.",
    });
    expect(String(thrown)).not.toContain("trust-core-canary-secret");
  });

  it("does not skip POSIX directory-link failures", async () => {
    const error = Object.assign(new Error("permission denied"), {
      code: "EPERM",
    });
    const onSkip = vi.fn();

    await expect(
      createDirectoryLinkForTest("target", "link", {
        createLink: vi.fn().mockRejectedValue(error),
        onSkip,
        platform: "linux",
      }),
    ).rejects.toBe(error);
    expect(onSkip).not.toHaveBeenCalled();
  });

  it("does not skip non-permission Windows directory-link failures", async () => {
    const error = Object.assign(new Error("link already exists"), {
      code: "EEXIST",
    });
    const onSkip = vi.fn();

    await expect(
      createDirectoryLinkForTest("target", "link", {
        createLink: vi.fn().mockRejectedValue(error),
        onSkip,
        platform: "win32",
      }),
    ).rejects.toBe(error);
    expect(onSkip).not.toHaveBeenCalled();
  });

  it.each(["EPERM", "EACCES"])(
    "skips Windows directory links only for permission error %s",
    async (code) => {
      const onSkip = vi.fn();
      const created = await createDirectoryLinkForTest("target", "link", {
        createLink: vi
          .fn()
          .mockRejectedValue(Object.assign(new Error("permission denied"), { code })),
        onSkip,
        platform: "win32",
      });

      expect(created).toBe(false);
      expect(onSkip).toHaveBeenCalledWith(expect.stringContaining(code));
    },
  );

  it("uses the real target for ids while preserving a link root path", async ({
    skip,
  }) => {
    const parent = await temporaryDirectory();
    const target = join(parent, "target 目录");
    const link = join(parent, "linked workspace");
    await mkdir(target);

    const linkedOnDisk = await createDirectoryLinkForTest(target, link, {
      createLink: symlink,
      onSkip: (reason) => {
        console.warn(reason);
        skip();
      },
      platform: process.platform,
    });
    if (!linkedOnDisk) {
      return;
    }

    const store = new MemoryTrustStore();
    const linked = await openWorkspace(link, store);
    const direct = await openWorkspace(target, store);

    expect(linked.rootPath).toBe(resolve(link));
    expect(linked.realPath).toBe(resolve(target));
    expect(linked.id).toBe(direct.id);
  });

  it("uses the nearest trust ancestor of a linked canonical target", async ({
    skip,
  }) => {
    const parent = await temporaryDirectory();
    const decisionParent = join(parent, "decision parent");
    const trustedAncestor = join(decisionParent, "trusted child");
    const target = join(trustedAncestor, "workspace target");
    const link = join(parent, "selected workspace");
    await mkdir(target, { recursive: true });

    const linkedOnDisk = await createDirectoryLinkForTest(target, link, {
      createLink: symlink,
      onSkip: (reason) => {
        console.warn(reason);
        skip();
      },
      platform: process.platform,
    });
    if (!linkedOnDisk) {
      return;
    }

    const store = new MemoryTrustStore();
    await store.setDecision(decisionParent, "untrusted");
    await store.setDecision(trustedAncestor, "trusted");

    const workspace = await openWorkspace(link, store);

    expect(workspace.rootPath).toBe(resolve(link));
    expect(workspace.realPath).toBe(resolve(target));
    expect(workspace.trustState).toBe("trusted");
  });
});

describe("trust decisions", () => {
  const olderDuplicateDecision: TrustDecision = {
    decidedAt: new Date("2026-07-22T00:00:00.000Z"),
    realPath: "/projects/app",
    state: "untrusted",
  };
  const newerDuplicateDecision: TrustDecision = {
    decidedAt: new Date("2026-07-23T00:00:00.000Z"),
    realPath: "/projects/app",
    state: "trusted",
  };
  const duplicateDecisionOrders: readonly [
    string,
    readonly TrustDecision[],
  ][] = [
    ["older-first", [olderDuplicateDecision, newerDuplicateDecision]],
    ["newer-first", [newerDuplicateDecision, olderDuplicateDecision]],
  ];

  it("defaults to untrusted when no decision exists", async () => {
    await expect(resolveTrust("/workspace", new MemoryTrustStore(), "linux")).resolves.toBe(
      "untrusted",
    );
  });

  it("selects the deepest current-path or ancestor decision", async () => {
    const store = new MemoryTrustStore("linux");
    await store.setDecision("/projects", "trusted");
    await store.setDecision("/projects/restricted", "untrusted");

    await expect(resolveTrust("/projects/app", store, "linux")).resolves.toBe("trusted");
    await expect(
      resolveTrust("/projects/restricted/app", store, "linux"),
    ).resolves.toBe("untrusted");
  });

  it.each(duplicateDecisionOrders)(
    "selects the latest duplicate decision in %s order",
    async (_, decisions) => {
      await expect(
        resolveTrust("/projects/app", trustStoreWith(decisions), "linux"),
      ).resolves.toBe("trusted");
    },
  );

  it("fails closed for duplicate decisions tied at the latest time", async () => {
    const decidedAt = new Date("2026-07-23T00:00:00.000Z");
    const store = trustStoreWith([
      { decidedAt, realPath: "/projects/app", state: "trusted" },
      { decidedAt, realPath: "/projects/app", state: "untrusted" },
    ]);

    await expect(resolveTrust("/projects/app", store, "linux")).resolves.toBe(
      "untrusted",
    );
  });

  it("fails closed when a deepest duplicate has an invalid decision time", async () => {
    const store = trustStoreWith([
      {
        decidedAt: new Date("2026-07-23T00:00:00.000Z"),
        realPath: "/projects/app",
        state: "trusted",
      },
      {
        decidedAt: new Date(Number.NaN),
        realPath: "/projects/app",
        state: "untrusted",
      },
    ]);

    await expect(resolveTrust("/projects/app", store, "linux")).resolves.toBe(
      "untrusted",
    );
  });

  it("does not treat a non-ancestor prefix as a decision", async () => {
    const store = new MemoryTrustStore("linux");
    await store.setDecision("/projects/app", "trusted");

    await expect(resolveTrust("/projects/app-copy", store, "linux")).resolves.toBe(
      "untrusted",
    );
  });

  it("does not case-fold non-canonical Windows decision paths", async () => {
    const store = new MemoryTrustStore("win32");
    await store.setDecision("C:\\Work", "trusted");

    await expect(
      resolveTrust("c:\\WORK\\Project", store, "win32"),
    ).resolves.toBe("untrusted");
  });

  it("preserves the normalized real path casing in stored decisions", async () => {
    const store = new MemoryTrustStore("win32");
    await store.setDecision("C:\\Work\\Project\\", "trusted");

    await expect(store.listDecisions()).resolves.toEqual([
      expect.objectContaining({ realPath: "C:\\Work\\Project" }),
    ]);
  });

  it("records replacement decisions with timestamps", async () => {
    const store = new MemoryTrustStore("linux");
    await store.setDecision("/projects/app/", "trusted");
    await store.setDecision("/projects/app", "untrusted");

    const decisions = await store.listDecisions();
    expect(decisions).toHaveLength(1);
    expect(decisions[0]).toMatchObject({
      realPath: "/projects/app",
      state: "untrusted",
    });
    expect(decisions[0]?.decidedAt).toBeInstanceOf(Date);
  });
});

describe("runtime trust policy", () => {
  it("fails closed for an unknown runtime kind", () => {
    expect(() =>
      runtimeTrustPolicy("future-runtime" as AgentKind, "trusted"),
    ).toThrowError(
      expect.objectContaining({
        code: "ProtocolViolation",
        message: "Runtime kind is not supported.",
      }),
    );
  });

  it("allows Pi project resources only for trusted workspaces", () => {
    expect(runtimeTrustPolicy("pi", "trusted")).toEqual({
      args: ["--approve"],
      env: {},
      loadProjectResources: true,
    });
    expect(runtimeTrustPolicy("pi", "untrusted")).toEqual({
      args: ["--no-approve", "--no-context-files"],
      env: {},
      loadProjectResources: false,
    });
  });

  it("disables OpenCode project configuration only when untrusted", () => {
    expect(runtimeTrustPolicy("opencode", "trusted")).toEqual({
      args: [],
      env: {},
      loadProjectResources: true,
    });
    expect(runtimeTrustPolicy("opencode", "untrusted")).toEqual({
      args: [],
      env: { OPENCODE_DISABLE_PROJECT_CONFIG: "1" },
      loadProjectResources: false,
    });
  });

  it("applies trust policy environment after the runtime environment", () => {
    const runtimeEnvironment = {
      OPENCODE_DISABLE_PROJECT_CONFIG: "0",
      PATH: "/usr/bin",
    };

    const merged = mergeRuntimeEnvironment(
      runtimeEnvironment,
      runtimeTrustPolicy("opencode", "untrusted"),
    );

    expect(merged).toEqual({
      PATH: "/usr/bin",
      OPENCODE_DISABLE_PROJECT_CONFIG: "1",
    });
    expect(runtimeEnvironment.OPENCODE_DISABLE_PROJECT_CONFIG).toBe("0");
  });
});
