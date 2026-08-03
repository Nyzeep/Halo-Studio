import {
  PI_CONFIGURATION_ERROR_CODES,
  PI_CONFIGURATION_ERROR_DETAILS,
  type PiConfigurationCommandErrorShape,
  type PiConfigurationErrorCode,
  type PiConfigurationSnapshot,
  type PiCredentialWriteResponse,
  type PiProviderReadinessResponse,
  type PiRuntimeConfigurationWriteInput,
} from './types';

export const HALO_PI_CREDENTIAL_WRITE_COMMAND = 'halo_pi_credential_write';
export const HALO_PI_CREDENTIAL_DELETE_COMMAND = 'halo_pi_credential_delete';
export const HALO_PI_CONFIGURATION_SNAPSHOT_COMMAND = 'halo_pi_configuration_snapshot';
export const HALO_PI_CONFIGURATION_CREATE_COMMAND = 'halo_pi_configuration_create';
export const HALO_PI_CONFIGURATION_UPDATE_COMMAND = 'halo_pi_configuration_update';
export const HALO_PI_CONFIGURATION_DELETE_COMMAND = 'halo_pi_configuration_delete';
export const HALO_PI_CONFIGURATION_ROLLBACK_COMMAND = 'halo_pi_configuration_rollback';
export const HALO_PI_CONFIGURATION_READINESS_COMMAND = 'halo_pi_configuration_readiness';

const PI_CREDENTIAL_REF_PREFIX = 'halo-pi-credential-v1-';
const PI_CREDENTIAL_REF_MAX_LENGTH = 128;

export interface PiConfigurationTransport {
  invoke<T>(command: string, args: Record<string, unknown>): Promise<T>;
}

export class PiConfigurationError extends Error implements PiConfigurationCommandErrorShape {
  readonly code: PiConfigurationErrorCode;
  readonly summary: string;
  readonly recoveryAction: string;

  constructor(code: PiConfigurationErrorCode) {
    const details = PI_CONFIGURATION_ERROR_DETAILS[code];
    super(details.summary);
    this.name = 'PiConfigurationError';
    this.code = code;
    this.summary = details.summary;
    this.recoveryAction = details.recoveryAction;
  }
}

const isErrorCode = (value: unknown): value is PiConfigurationErrorCode => (
  typeof value === 'string'
  && (PI_CONFIGURATION_ERROR_CODES as readonly string[]).includes(value)
);

const isSafeSelection = (value: unknown): value is string => (
  typeof value === 'string'
  && value.length > 0
  && value.length <= 256
  && !value.startsWith('-')
  && !value.includes('\\')
  && !Array.from(value).some(character => /\p{Cc}/u.test(character))
);

const isSafeBaseUrl = (value: unknown): value is string | null => {
  if (value === null) {
    return true;
  }
  if (typeof value !== 'string') {
    return false;
  }
  if (
    value.length === 0
    || value.length > 2048
    || value !== value.trim()
    || /[\s\u0000-\u001f\u007f?#@]/u.test(value)
  ) {
    return false;
  }
  try {
    const url = new URL(value);
    return (
      (url.protocol === 'http:' || url.protocol === 'https:')
      && url.hostname.length > 0
      && url.username.length === 0
      && url.password.length === 0
      && url.search.length === 0
      && url.hash.length === 0
    );
  } catch {
    return false;
  }
};

const isSafeCredentialRef = (value: unknown): value is string => {
  if (
    typeof value !== 'string'
    || !value.startsWith(PI_CREDENTIAL_REF_PREFIX)
    || value.length > PI_CREDENTIAL_REF_MAX_LENGTH
  ) {
    return false;
  }
  const suffix = value.slice(PI_CREDENTIAL_REF_PREFIX.length);
  return suffix.length > 0
    && Array.from(suffix).every(character => /[a-zA-Z0-9-]/u.test(character));
};

const rejectInvalidInput = (): never => {
  throw new PiConfigurationError('pi_configuration_invalid');
};

const validateCredentialInput = (providerId: unknown, secret: unknown): void => {
  if (!isSafeSelection(providerId)) {
    rejectInvalidInput();
  }
  const secretValue = typeof secret === 'string' ? secret : '';
  if (secretValue.length === 0 || secretValue.length > 512 * 1024) {
    rejectInvalidInput();
  }
  if (Array.from(secretValue).some(character => /\p{Cc}/u.test(character))) {
    rejectInvalidInput();
  }
};

const validateConfigurationInput = (configuration: PiRuntimeConfigurationWriteInput): void => {
  if (
    typeof configuration !== 'object'
    || configuration === null
    || !isSafeSelection(configuration.providerId)
    || !isSafeSelection(configuration.modelId)
    || !isSafeCredentialRef(configuration.credentialRef)
    || !isSafeBaseUrl(configuration.baseUrl)
    || typeof configuration.startupOptions !== 'object'
    || configuration.startupOptions === null
    || configuration.startupOptions.noExtensions !== true
    || configuration.startupOptions.noApprove !== true
  ) {
    rejectInvalidInput();
  }
};

/**
 * Converts Tauri rejections into a fixed public error vocabulary. The
 * transport's summary, recovery action, and raw payload are deliberately not
 * carried across the Renderer boundary.
 */
export const toPiConfigurationError = (error: unknown): PiConfigurationError => {
  if (typeof error === 'object' && error !== null && 'code' in error) {
    const code = (error as { code?: unknown }).code;
    if (isErrorCode(code)) {
      return new PiConfigurationError(code);
    }
  }
  return new PiConfigurationError('pi_configuration_unavailable');
};

const invoke = async <T>(
  transport: PiConfigurationTransport,
  command: string,
  args: Record<string, unknown>,
): Promise<T> => {
  try {
    return await transport.invoke<T>(command, args);
  } catch (error) {
    throw toPiConfigurationError(error);
  }
};

export interface PiConfigurationClient {
  writeCredential(providerId: string, secret: string): Promise<PiCredentialWriteResponse>;
  deleteCredential(providerId: string, credentialRef: string): Promise<void>;
  readSnapshot(): Promise<PiConfigurationSnapshot | null>;
  createConfiguration(configuration: PiRuntimeConfigurationWriteInput): Promise<void>;
  updateConfiguration(configuration: PiRuntimeConfigurationWriteInput): Promise<void>;
  deleteConfiguration(): Promise<void>;
  rollbackConfiguration(): Promise<void>;
  checkReadiness(): Promise<PiProviderReadinessResponse>;
}

export const createPiConfigurationClient = (
  transport: PiConfigurationTransport,
): PiConfigurationClient => ({
  writeCredential: async (providerId, secret) => {
    validateCredentialInput(providerId, secret);
    return invoke<PiCredentialWriteResponse>(
      transport,
      HALO_PI_CREDENTIAL_WRITE_COMMAND,
      { request: { providerId, secret } },
    );
  },
  deleteCredential: (providerId, credentialRef) => invoke<void>(
    transport,
    HALO_PI_CREDENTIAL_DELETE_COMMAND,
    { request: { providerId, credentialRef } },
  ),
  readSnapshot: () => invoke<PiConfigurationSnapshot | null>(
    transport,
    HALO_PI_CONFIGURATION_SNAPSHOT_COMMAND,
    { request: {} },
  ),
  createConfiguration: async configuration => {
    validateConfigurationInput(configuration);
    return invoke<void>(
      transport,
      HALO_PI_CONFIGURATION_CREATE_COMMAND,
      { request: { configuration } },
    );
  },
  updateConfiguration: async configuration => {
    validateConfigurationInput(configuration);
    return invoke<void>(
      transport,
      HALO_PI_CONFIGURATION_UPDATE_COMMAND,
      { request: { configuration } },
    );
  },
  deleteConfiguration: () => invoke<void>(
    transport,
    HALO_PI_CONFIGURATION_DELETE_COMMAND,
    { request: {} },
  ),
  rollbackConfiguration: () => invoke<void>(
    transport,
    HALO_PI_CONFIGURATION_ROLLBACK_COMMAND,
    { request: {} },
  ),
  checkReadiness: () => invoke<PiProviderReadinessResponse>(
    transport,
    HALO_PI_CONFIGURATION_READINESS_COMMAND,
    { request: {} },
  ),
});

export const createTauriPiConfigurationTransport = (): PiConfigurationTransport => ({
  invoke: async <T>(command: string, args: Record<string, unknown>) => {
    const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
    return tauriInvoke<T>(command, args);
  },
});
