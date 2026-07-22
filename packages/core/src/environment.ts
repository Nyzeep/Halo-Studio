import { types as utilTypes } from "node:util";

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
const INVALID_ENVIRONMENT_MESSAGE =
  "Runtime environment input is not permitted.";
const NO_PROVIDER_KEYS = new Set<string>();

function invalidEnvironment(): never {
  throw new CoreError("ProtocolViolation", INVALID_ENVIRONMENT_MESSAGE);
}

function assertInspectableObject(value: unknown): asserts value is object {
  if (
    value === null ||
    typeof value !== "object" ||
    utilTypes.isProxy(value)
  ) {
    invalidEnvironment();
  }
}

function readEnvironmentValue(
  source: object,
  key: string,
): string | undefined {
  const descriptor = Object.getOwnPropertyDescriptor(source, key);
  if (descriptor === undefined) {
    return undefined;
  }
  if (!("value" in descriptor)) {
    return invalidEnvironment();
  }

  const value: unknown = descriptor.value;
  if (value === undefined) {
    return undefined;
  }
  if (typeof value !== "string" || value.includes("\0")) {
    return invalidEnvironment();
  }
  return value;
}

function copyDefined(
  target: Record<string, string>,
  source: object,
  sourceKey: string,
  targetKey = sourceKey,
): void {
  const value = readEnvironmentValue(source, sourceKey);
  if (value !== undefined) {
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

function assertNativeProviderKeySet(
  value: unknown,
): asserts value is ReadonlySet<string> {
  if (
    value === null ||
    typeof value !== "object" ||
    utilTypes.isProxy(value) ||
    Object.getPrototypeOf(value) !== Set.prototype
  ) {
    invalidEnvironment();
  }

  Set.prototype.forEach.call(value, (key: unknown) => {
    if (
      typeof key !== "string" ||
      !PROVIDER_KEY_PATTERN.test(key) ||
      isBlockedProviderKey(key)
    ) {
      invalidEnvironment();
    }
  });
}

export function buildRuntimeEnvironment(
  hostEnvironment: EnvironmentInput,
  providerEnvironment: ProviderEnvironment = {},
  allowedProviderKeys: ReadonlySet<string> = NO_PROVIDER_KEYS,
): Record<string, string> {
  try {
    assertInspectableObject(hostEnvironment);
    assertInspectableObject(providerEnvironment);
    assertNativeProviderKeySet(allowedProviderKeys);
    const result: Record<string, string> = {};

    const pathValue = readEnvironmentValue(hostEnvironment, "PATH");
    if (pathValue !== undefined) {
      result.PATH = pathValue;
    } else {
      copyDefined(result, hostEnvironment, "Path", "PATH");
    }

    for (const key of HOST_KEYS) {
      copyDefined(result, hostEnvironment, key);
    }

    for (const key of Reflect.ownKeys(providerEnvironment)) {
      if (typeof key !== "string") {
        invalidEnvironment();
      }

      const descriptor = Object.getOwnPropertyDescriptor(
        providerEnvironment,
        key,
      );
      if (descriptor === undefined || !descriptor.enumerable) {
        continue;
      }
      if (!("value" in descriptor)) {
        invalidEnvironment();
      }

      const value: unknown = descriptor.value;
      if (
        !PROVIDER_KEY_PATTERN.test(key) ||
        isBlockedProviderKey(key) ||
        !Set.prototype.has.call(allowedProviderKeys, key) ||
        typeof value !== "string" ||
        value.includes("\0")
      ) {
        invalidEnvironment();
      }
      result[key] = value;
    }

    return result;
  } catch (error) {
    if (
      error instanceof CoreError &&
      error.code === "ProtocolViolation" &&
      error.message === INVALID_ENVIRONMENT_MESSAGE
    ) {
      throw error;
    }
    return invalidEnvironment();
  }
}
