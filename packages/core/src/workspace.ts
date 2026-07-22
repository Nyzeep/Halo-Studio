import {
  access as nodeAccess,
  realpath as nodeRealpath,
  stat as nodeStat,
} from "node:fs/promises";
import { constants } from "node:fs";

import type { Workspace } from "@halo-studio/contracts";

import { CoreError } from "./error.js";
import {
  normalizeFilesystemPath,
  resolveUserPath,
  workspaceIdForPath,
  type PathPlatform,
} from "./pathPolicy.js";
import { resolveTrust, type TrustStore } from "./trust.js";

export interface FsPort {
  access(path: string, mode?: number): Promise<void>;
  realpath(path: string): Promise<string>;
  stat(path: string): Promise<{ isDirectory(): boolean }>;
}

const defaultFs: FsPort = {
  access: async (path, mode) => nodeAccess(path, mode),
  realpath: async (path) => nodeRealpath(path),
  stat: async (path) => nodeStat(path),
};

export interface OpenWorkspaceOptions {
  readonly cwd?: string;
  readonly fs?: FsPort;
  readonly platform?: PathPlatform;
}

export async function openWorkspace(
  userPath: string,
  trustStore: TrustStore,
  options: OpenWorkspaceOptions = {},
): Promise<Workspace> {
  const platform = options.platform ?? process.platform;
  const fs = options.fs ?? defaultFs;
  const rootPath = resolveUserPath(
    userPath,
    options.cwd ?? process.cwd(),
    platform,
  );
  let realPath: string;

  try {
    const resolvedRealPath = await fs.realpath(rootPath);
    realPath = normalizeFilesystemPath(resolvedRealPath, platform);
    const pathStat = await fs.stat(realPath);
    if (!pathStat.isDirectory()) {
      throw new CoreError("UnsafePath", "Workspace path must be a directory.");
    }
    await fs.access(realPath, constants.R_OK | constants.X_OK);
  } catch (error) {
    if (error instanceof CoreError) {
      throw error;
    }
    throw new CoreError("UnsafePath", "Workspace path is unavailable.");
  }

  let trustState: Workspace["trustState"];
  try {
    trustState = await resolveTrust(realPath, trustStore, platform);
  } catch (error) {
    if (error instanceof CoreError) {
      throw error;
    }
    throw new CoreError(
      "ProtocolViolation",
      "Workspace trust decision is unavailable.",
    );
  }

  return {
    id: workspaceIdForPath(realPath, platform),
    rootPath,
    realPath,
    trustState,
  };
}
