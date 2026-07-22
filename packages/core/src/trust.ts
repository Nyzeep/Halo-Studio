import type { AgentKind, TrustState } from "@halo-studio/contracts";

import { OPENCODE_PROJECT_CONFIG_ENV } from "./environment.js";
import {
  isPathWithin,
  normalizeFilesystemPath,
  normalizePathKey,
  type PathPlatform,
} from "./pathPolicy.js";

export interface TrustDecision {
  readonly realPath: string;
  readonly state: TrustState;
  readonly decidedAt: Date;
}

export interface TrustStore {
  listDecisions(): Promise<readonly TrustDecision[]>;
  setDecision(realPath: string, state: TrustState): Promise<void>;
}

export class MemoryTrustStore implements TrustStore {
  readonly #decisions = new Map<string, TrustDecision>();
  readonly #platform: PathPlatform;

  constructor(platform: PathPlatform = process.platform) {
    this.#platform = platform;
  }

  async listDecisions(): Promise<readonly TrustDecision[]> {
    return [...this.#decisions.values()].map((decision) => ({
      ...decision,
      decidedAt: new Date(decision.decidedAt),
    }));
  }

  async setDecision(realPath: string, state: TrustState): Promise<void> {
    const key = normalizePathKey(realPath, this.#platform);
    this.#decisions.set(key, {
      decidedAt: new Date(),
      realPath: normalizeFilesystemPath(realPath, this.#platform),
      state,
    });
  }
}

function pathDepth(realPath: string, platform: PathPlatform): number {
  const separator = platform === "win32" ? "\\" : "/";
  return normalizePathKey(realPath, platform)
    .split(separator)
    .filter((segment) => segment.length > 0).length;
}

export async function resolveTrust(
  realPath: string,
  store: TrustStore,
  platform: PathPlatform = process.platform,
): Promise<TrustState> {
  const decisions = await store.listDecisions();
  let selected: TrustDecision | undefined;
  let selectedDepth = -1;

  for (const decision of decisions) {
    if (!isPathWithin(realPath, decision.realPath, platform)) {
      continue;
    }

    const depth = pathDepth(decision.realPath, platform);
    if (depth > selectedDepth) {
      selected = decision;
      selectedDepth = depth;
    }
  }

  return selected?.state ?? "untrusted";
}

export interface RuntimeTrustPolicy {
  readonly args: readonly string[];
  readonly env: Readonly<Record<string, string>>;
  readonly loadProjectResources: boolean;
}

export function runtimeTrustPolicy(
  kind: AgentKind,
  state: TrustState,
): RuntimeTrustPolicy {
  if (kind === "pi") {
    return state === "trusted"
      ? { args: ["--approve"], env: {}, loadProjectResources: true }
      : {
          args: ["--no-approve", "--no-context-files"],
          env: {},
          loadProjectResources: false,
        };
  }

  return state === "trusted"
    ? { args: [], env: {}, loadProjectResources: true }
    : {
        args: [],
        env: { [OPENCODE_PROJECT_CONFIG_ENV]: "1" },
        loadProjectResources: false,
      };
}

export function mergeRuntimeEnvironment(
  runtimeEnvironment: Readonly<Record<string, string>>,
  policy: RuntimeTrustPolicy,
): Record<string, string> {
  return { ...runtimeEnvironment, ...policy.env };
}
