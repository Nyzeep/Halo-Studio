import type { Workspace } from "@halo-studio/contracts";
import { buildRuntimeEnvironment, CoreError } from "@halo-studio/core";
import type { CredentialVault } from "@halo-studio/storage";
import { types as utilTypes } from "node:util";

/**
 * The only shape allowed to cross from a Main-owned configuration source into
 * a Pi child process.  It is deliberately not part of the IPC contract.
 */
export interface PiLaunchConfiguration {
  readonly model: string;
  readonly thinking: string;
  readonly providerEnvironment: Readonly<Record<string, string>>;
  readonly allowedProviderKeys: ReadonlySet<string>;
}

export interface PiLaunchContext {
  readonly workspace: Pick<Workspace, "id" | "realPath" | "trustState">;
}

export type PiLaunchResolver = (
  context: PiLaunchContext,
) => Promise<PiLaunchConfiguration>;

export interface EnvironmentPiLaunchResolverOptions {
  /** Main-process environment only; it is never exposed through preload. */
  readonly environment: Readonly<Record<string, string | undefined>>;
  /** The Provider value stays encrypted until the resolver is about to spawn Pi. */
  readonly vault: CredentialVault;
}

const MAX_LAUNCH_VALUE_BYTES = 1024;
const CONFIGURATION_ERROR = "Runtime is unavailable.";

function unavailable(): CoreError {
  return new CoreError("RuntimeUnavailable", CONFIGURATION_ERROR);
}

function validText(value: unknown): value is string {
  return (
    typeof value === "string"
    && value.length > 0
    && !value.includes("\0")
    && Buffer.byteLength(value, "utf8") <= MAX_LAUNCH_VALUE_BYTES
  );
}

function ownText(record: object, key: string): string {
  try {
    const descriptor = Object.getOwnPropertyDescriptor(record, key);
    if (descriptor === undefined || !("value" in descriptor) || !validText(descriptor.value)) {
      throw unavailable();
    }
    return descriptor.value;
  } catch (error) {
    if (error instanceof CoreError) throw error;
    throw unavailable();
  }
}

function ownEnvironmentText(
  environment: Readonly<Record<string, string | undefined>>,
  key: string,
): string {
  try {
    const descriptor = Object.getOwnPropertyDescriptor(environment, key);
    if (descriptor === undefined || !("value" in descriptor) || !validText(descriptor.value)) {
      throw unavailable();
    }
    return descriptor.value;
  } catch (error) {
    if (error instanceof CoreError) throw error;
    throw unavailable();
  }
}

/**
 * Normalizes resolver output before it reaches the agent runtime.  In
 * particular, this rejects accessor/proxy values before their properties can
 * be evaluated by process construction code.
 */
export function validatePiLaunchConfiguration(
  value: unknown,
): PiLaunchConfiguration {
  try {
    if (
      value === null
      || typeof value !== "object"
      || utilTypes.isProxy(value)
      || (Object.getPrototypeOf(value) !== Object.prototype
        && Object.getPrototypeOf(value) !== null)
    ) {
      throw unavailable();
    }
    const keys = Reflect.ownKeys(value);
    const allowedKeys = new Set([
      "model",
      "thinking",
      "providerEnvironment",
      "allowedProviderKeys",
    ]);
    if (keys.some((key) => typeof key !== "string" || !allowedKeys.has(key))) {
      throw unavailable();
    }

    const record = value as Record<string, unknown>;
    const model = ownText(record, "model");
    const thinking = ownText(record, "thinking");
    const providerDescriptor = Object.getOwnPropertyDescriptor(record, "providerEnvironment");
    const keyDescriptor = Object.getOwnPropertyDescriptor(record, "allowedProviderKeys");
    if (
      providerDescriptor === undefined
      || !("value" in providerDescriptor)
      || keyDescriptor === undefined
      || !("value" in keyDescriptor)
    ) {
      throw unavailable();
    }

    // core owns the provider allowlist rules. It also clones only own data
    // properties, so no mutable resolver-owned object reaches PiRuntime.
    const providerEnvironment = buildRuntimeEnvironment(
      {},
      providerDescriptor.value as Readonly<Record<string, string>>,
      keyDescriptor.value as ReadonlySet<string>,
    );
    const sourceKeys = keyDescriptor.value as ReadonlySet<string>;
    const copiedKeys = new Set<string>();
    Set.prototype.forEach.call(sourceKeys, (key: unknown) => {
      if (typeof key !== "string") throw unavailable();
      copiedKeys.add(key);
    });

    return {
      model,
      thinking,
      providerEnvironment,
      allowedProviderKeys: copiedKeys,
    };
  } catch {
    throw unavailable();
  }
}

/**
 * Initial Main-only source for the first lifecycle phase. The environment
 * stores only non-secret selectors; the actual Provider value is fetched from
 * Electron-protected credential storage immediately before Pi is created.
 */
export function createEnvironmentPiLaunchResolver(
  options: EnvironmentPiLaunchResolverOptions,
): PiLaunchResolver {
  return async (_context) => {
    const model = ownEnvironmentText(options.environment, "HALO_PI_MODEL");
    const thinking = ownEnvironmentText(options.environment, "HALO_PI_THINKING");
    const providerKey = ownEnvironmentText(options.environment, "HALO_PI_PROVIDER_ENV_KEY");
    const credentialReference = ownEnvironmentText(
      options.environment,
      "HALO_PI_CREDENTIAL_REFERENCE",
    );
    let credential: string | null;
    try {
      if (!options.vault.isAvailable()) throw unavailable();
      credential = await options.vault.get(credentialReference);
    } catch {
      throw unavailable();
    }
    if (!validText(credential)) throw unavailable();

    return validatePiLaunchConfiguration({
      model,
      thinking,
      providerEnvironment: { [providerKey]: credential },
      allowedProviderKeys: new Set([providerKey]),
    });
  };
}
