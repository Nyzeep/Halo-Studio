import { CoreError } from "./error.js";

export type EnvironmentInput = Readonly<Record<string, string | undefined>>;
export type ProviderEnvironment = Readonly<Record<string, string>>;

export const OPENCODE_PROJECT_CONFIG_ENV =
  "OPENCODE_DISABLE_PROJECT_CONFIG" as const;

const HOST_KEYS = [
  "HOME",
  "USERPROFILE",
  "TEMP",
  "TMP",
  "TMPDIR",
  "LANG",
  "LANGUAGE",
  "LC_ALL",
  "LC_CTYPE",
  "HTTP_PROXY",
  "HTTPS_PROXY",
  "ALL_PROXY",
  "NO_PROXY",
  "http_proxy",
  "https_proxy",
  "all_proxy",
  "no_proxy",
] as const;

const BASE_KEYS = new Set<string>(["PATH", ...HOST_KEYS]);
const BLOCKED_PROVIDER_KEYS = new Set([
  "BASH_ENV",
  "ELECTRON_RUN_AS_NODE",
  "NODE_OPTIONS",
  "NODE_PATH",
  OPENCODE_PROJECT_CONFIG_ENV,
  "PERL5OPT",
  "PYTHONHOME",
  "PYTHONPATH",
  "RUBYOPT",
]);
const PROVIDER_KEY_PATTERN = /^[A-Z][A-Z0-9_]*$/u;

function copyDefined(
  target: Record<string, string>,
  source: EnvironmentInput,
  sourceKey: string,
  targetKey = sourceKey,
): void {
  const value = source[sourceKey];
  if (typeof value === "string") {
    target[targetKey] = value;
  }
}

function isBlockedProviderKey(key: string): boolean {
  return (
    BASE_KEYS.has(key) ||
    BLOCKED_PROVIDER_KEYS.has(key) ||
    key.startsWith("DYLD_") ||
    key.startsWith("LD_")
  );
}

export function buildRuntimeEnvironment(
  hostEnvironment: EnvironmentInput,
  providerEnvironment: ProviderEnvironment = {},
): Record<string, string> {
  const result: Record<string, string> = {};

  if (typeof hostEnvironment.PATH === "string") {
    result.PATH = hostEnvironment.PATH;
  } else {
    copyDefined(result, hostEnvironment, "Path", "PATH");
  }

  for (const key of HOST_KEYS) {
    copyDefined(result, hostEnvironment, key);
  }

  for (const [key, value] of Object.entries(providerEnvironment)) {
    if (
      !PROVIDER_KEY_PATTERN.test(key) ||
      isBlockedProviderKey(key) ||
      typeof value !== "string" ||
      value.includes("\0")
    ) {
      throw new CoreError(
        "ProtocolViolation",
        "Provider environment variable is not permitted.",
      );
    }
    result[key] = value;
  }

  return result;
}
