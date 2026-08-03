/**
 * The thinking levels accepted by Halo's persisted Pi configuration.
 * Keep this union narrower than Pi's internal capability list.
 */
export type PiThinkingLevel = 'off' | 'minimal' | 'low' | 'medium' | 'high';

export interface PiStartupOptions {
  noExtensions: boolean;
  noApprove: boolean;
}

/**
 * Input sent by the settings surface. `baseUrl` is write-only from the
 * Renderer boundary: it is never present in a snapshot returned by Tauri.
 */
export interface PiRuntimeConfigurationWriteInput {
  providerId: string;
  baseUrl: string | null;
  modelId: string;
  thinkingLevel: PiThinkingLevel;
  startupOptions: PiStartupOptions;
  credentialRef: string;
}

/**
 * Renderer-safe persisted configuration projection. The server-side full
 * base URL is intentionally represented only by an opaque hint.
 */
export interface PiConfigurationSnapshot {
  providerId: string;
  modelId: string;
  thinkingLevel: PiThinkingLevel;
  startupOptions: PiStartupOptions;
  credentialRef: string;
  baseUrlHint: string | null;
}

export interface PiCredentialWriteResponse {
  credentialRef: string;
}

export interface PiProviderReadinessResponse {
  available: boolean;
}

export const PI_CONFIGURATION_ERROR_CODES = [
  'pi_configuration_invalid',
  'pi_configuration_missing',
  'pi_configuration_denied',
  'pi_configuration_store_unavailable',
  'pi_configuration_unavailable',
] as const;

export type PiConfigurationErrorCode = (typeof PI_CONFIGURATION_ERROR_CODES)[number];

export interface PiConfigurationCommandErrorShape {
  code: PiConfigurationErrorCode;
  summary: string;
  recoveryAction: string;
}

export const PI_CONFIGURATION_ERROR_DETAILS: Record<
  PiConfigurationErrorCode,
  Omit<PiConfigurationCommandErrorShape, 'code'>
> = {
  pi_configuration_invalid: {
    summary: 'The Pi configuration is invalid',
    recoveryAction: 'correct_configuration',
  },
  pi_configuration_missing: {
    summary: 'The requested Pi configuration or credential was not found',
    recoveryAction: 'configure_provider',
  },
  pi_configuration_denied: {
    summary: 'The Pi credential does not belong to the selected provider',
    recoveryAction: 'configure_provider',
  },
  pi_configuration_store_unavailable: {
    summary: 'The system credential or configuration store is unavailable',
    recoveryAction: 'retry',
  },
  pi_configuration_unavailable: {
    summary: 'The Pi configuration service is unavailable',
    recoveryAction: 'retry',
  },
};

