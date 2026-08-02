import { describe, expect, it, vi } from 'vitest';

import {
  createPiConfigurationClient,
  HALO_PI_CONFIGURATION_CREATE_COMMAND,
  HALO_PI_CONFIGURATION_DELETE_COMMAND,
  HALO_PI_CONFIGURATION_READINESS_COMMAND,
  HALO_PI_CONFIGURATION_ROLLBACK_COMMAND,
  HALO_PI_CONFIGURATION_SNAPSHOT_COMMAND,
  HALO_PI_CONFIGURATION_UPDATE_COMMAND,
  HALO_PI_CREDENTIAL_DELETE_COMMAND,
  HALO_PI_CREDENTIAL_WRITE_COMMAND,
  type PiConfigurationTransport,
} from './client';
import type {
  PiConfigurationSnapshot,
  PiCredentialWriteResponse,
  PiRuntimeConfigurationWriteInput,
  PiProviderReadinessResponse,
} from './types';

const CONFIGURATION: PiRuntimeConfigurationWriteInput = {
  providerId: 'openai',
  baseUrl: 'https://api.example.test/v1',
  modelId: 'gpt-5',
  thinkingLevel: 'medium',
  startupOptions: { noExtensions: true, noApprove: true },
  credentialRef: 'halo-pi-credential-v1-openai-opaque',
};

const SNAPSHOT: PiConfigurationSnapshot = {
  providerId: 'openai',
  modelId: 'gpt-5',
  thinkingLevel: 'medium',
  startupOptions: { noExtensions: true, noApprove: true },
  credentialRef: 'halo-pi-credential-v1-openai-opaque',
  baseUrlHint: '<configured>',
};

describe('PiConfigurationClient', () => {
  it('maps the isolated Tauri commands and keeps snapshot DTOs Renderer-safe', async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === HALO_PI_CONFIGURATION_SNAPSHOT_COMMAND) {
        return SNAPSHOT;
      }
      if (command === HALO_PI_CREDENTIAL_WRITE_COMMAND) {
        return { credentialRef: 'halo-pi-credential-v1-openai-new' } satisfies PiCredentialWriteResponse;
      }
      if (command === HALO_PI_CONFIGURATION_READINESS_COMMAND) {
        return { available: true } satisfies PiProviderReadinessResponse;
      }
      return undefined;
    });
    const transport: PiConfigurationTransport = { invoke };
    const client = createPiConfigurationClient(transport);

    await expect(client.readSnapshot()).resolves.toEqual(SNAPSHOT);
    await expect(client.writeCredential('openai', 'secret-value')).resolves.toEqual({
      credentialRef: 'halo-pi-credential-v1-openai-new',
    });
    await client.deleteCredential('openai', 'halo-pi-credential-v1-openai-old');
    await client.createConfiguration(CONFIGURATION);
    await client.updateConfiguration(CONFIGURATION);
    await client.deleteConfiguration();
    await client.rollbackConfiguration();
    await expect(client.checkReadiness()).resolves.toEqual({ available: true });

    expect(invoke.mock.calls).toEqual([
      [HALO_PI_CONFIGURATION_SNAPSHOT_COMMAND, { request: {} }],
      [HALO_PI_CREDENTIAL_WRITE_COMMAND, {
        request: { providerId: 'openai', secret: 'secret-value' },
      }],
      [HALO_PI_CREDENTIAL_DELETE_COMMAND, {
        request: {
          providerId: 'openai',
          credentialRef: 'halo-pi-credential-v1-openai-old',
        },
      }],
      [HALO_PI_CONFIGURATION_CREATE_COMMAND, { request: { configuration: CONFIGURATION } }],
      [HALO_PI_CONFIGURATION_UPDATE_COMMAND, { request: { configuration: CONFIGURATION } }],
      [HALO_PI_CONFIGURATION_DELETE_COMMAND, { request: {} }],
      [HALO_PI_CONFIGURATION_ROLLBACK_COMMAND, { request: {} }],
      [HALO_PI_CONFIGURATION_READINESS_COMMAND, { request: {} }],
    ]);
    expect(SNAPSHOT).not.toHaveProperty('baseUrl');
    expect(JSON.stringify(SNAPSHOT)).not.toContain('api.example.test');
  });

  it('normalizes command failures to stable, non-sensitive errors', async () => {
    const sensitiveError = {
      code: 'pi_configuration_denied',
      summary: 'secret-value Authorization https://api.example.test/v1',
      recoveryAction: 'internal details',
      raw: 'auth.json SECRET=value',
    };
    const transport: PiConfigurationTransport = {
      invoke: vi.fn(async () => Promise.reject(sensitiveError)),
    };
    const client = createPiConfigurationClient(transport);

    await expect(client.readSnapshot()).rejects.toMatchObject({
      name: 'PiConfigurationError',
      code: 'pi_configuration_denied',
      summary: 'The Pi credential does not belong to the selected provider',
      recoveryAction: 'configure_provider',
    });
    await expect(client.readSnapshot()).rejects.toSatisfy((error: Error) => (
      !error.message.includes('secret-value')
      && !error.message.includes('Authorization')
      && !error.message.includes('api.example.test')
      && !error.message.includes('auth.json')
      && !error.message.includes('SECRET=value')
    ));
  });

  it('falls back to the stable unavailable code for unknown failures', async () => {
    const transport: PiConfigurationTransport = {
      invoke: vi.fn(async () => Promise.reject('raw backend failure with secret-value')),
    };
    const client = createPiConfigurationClient(transport);

    await expect(client.checkReadiness()).rejects.toMatchObject({
      code: 'pi_configuration_unavailable',
      summary: 'The Pi configuration service is unavailable',
      recoveryAction: 'retry',
    });
  });
});
