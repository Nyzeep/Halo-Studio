// @vitest-environment jsdom

import React, { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { PiConfigurationClient } from '@/infrastructure/pi-configuration/client';
import PiConfigurationPanel from './PiConfigurationPanel';

globalThis.IS_REACT_ACT_ENVIRONMENT = true;

vi.mock('@/infrastructure/i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

describe('PiConfigurationPanel', () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it('creates a missing Pi configuration, clears the one-shot secret, and refreshes the runtime', async () => {
    const client: PiConfigurationClient = {
      writeCredential: vi.fn(async () => ({
        credentialRef: 'halo-pi-credential-v1-openai-new',
      })),
      deleteCredential: vi.fn(async () => undefined),
      readSnapshot: vi.fn(async () => null),
      createConfiguration: vi.fn(async () => undefined),
      updateConfiguration: vi.fn(async () => undefined),
      deleteConfiguration: vi.fn(async () => undefined),
      rollbackConfiguration: vi.fn(async () => undefined),
      checkReadiness: vi.fn(async () => ({ available: true })),
    };
    const onConfigured = vi.fn(async () => undefined);

    await act(async () => {
      root.render(<PiConfigurationPanel client={client} onConfigured={onConfigured} />);
    });

    await vi.waitFor(() => {
      expect(container.querySelector('[data-testid="pi-configuration-provider"]')).not.toBeNull();
    });

    const setValue = (testId: string, value: string) => {
      const input = container.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`);
      expect(input).not.toBeNull();
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setter?.call(input, value);
      input?.dispatchEvent(new Event('input', { bubbles: true }));
      input?.dispatchEvent(new Event('change', { bubbles: true }));
    };

    await act(async () => {
      setValue('pi-configuration-provider', 'openai');
      setValue('pi-configuration-model', 'gpt-5');
      setValue('pi-configuration-secret', 'one-shot-secret');
    });

    const form = container.querySelector<HTMLFormElement>('[data-testid="pi-configuration-form"]');
    expect(form).not.toBeNull();
    await act(async () => {
      form?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    });

    await vi.waitFor(() => {
      expect(client.createConfiguration).toHaveBeenCalledWith({
        providerId: 'openai',
        baseUrl: null,
        modelId: 'gpt-5',
        thinkingLevel: 'medium',
        startupOptions: { noExtensions: true, noApprove: true },
        credentialRef: 'halo-pi-credential-v1-openai-new',
      });
      expect(onConfigured).toHaveBeenCalledOnce();
    });

    expect(client.writeCredential).toHaveBeenCalledWith('openai', 'one-shot-secret');
    expect(client.checkReadiness).toHaveBeenCalledOnce();
    expect(container.querySelector<HTMLInputElement>('[data-testid="pi-configuration-secret"]')?.value)
      .toBe('');
  });

  it('clears a one-shot secret after configuration persistence even when Pi is not ready', async () => {
    const client: PiConfigurationClient = {
      writeCredential: vi.fn(async () => ({
        credentialRef: 'halo-pi-credential-v1-openai-new',
      })),
      deleteCredential: vi.fn(async () => undefined),
      readSnapshot: vi.fn(async () => null),
      createConfiguration: vi.fn(async () => undefined),
      updateConfiguration: vi.fn(async () => undefined),
      deleteConfiguration: vi.fn(async () => undefined),
      rollbackConfiguration: vi.fn(async () => undefined),
      checkReadiness: vi.fn(async () => ({ available: false })),
    };
    const onConfigured = vi.fn(async () => undefined);

    await act(async () => {
      root.render(<PiConfigurationPanel client={client} onConfigured={onConfigured} />);
    });

    await vi.waitFor(() => {
      expect(container.querySelector('[data-testid="pi-configuration-provider"]')).not.toBeNull();
    });

    const setValue = (testId: string, value: string) => {
      const input = container.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`);
      expect(input).not.toBeNull();
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setter?.call(input, value);
      input?.dispatchEvent(new Event('input', { bubbles: true }));
      input?.dispatchEvent(new Event('change', { bubbles: true }));
    };

    await act(async () => {
      setValue('pi-configuration-provider', 'openai');
      setValue('pi-configuration-model', 'gpt-5');
      setValue('pi-configuration-secret', 'one-shot-secret');
    });

    const form = container.querySelector<HTMLFormElement>('[data-testid="pi-configuration-form"]');
    expect(form).not.toBeNull();
    await act(async () => {
      form?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    });

    await vi.waitFor(() => {
      expect(client.checkReadiness).toHaveBeenCalledOnce();
      expect(container.querySelector<HTMLInputElement>('[data-testid="pi-configuration-secret"]')?.value)
        .toBe('');
    });
    expect(onConfigured).not.toHaveBeenCalled();
  });

  it('preserves a write-only endpoint while rotating the provider credential', async () => {
    const previousConfiguration = {
      providerId: 'openai',
      modelId: 'gpt-4.1',
      thinkingLevel: 'medium' as const,
      startupOptions: { noExtensions: true, noApprove: true },
      credentialRef: 'halo-pi-credential-v1-openai-previous',
      baseUrlHint: '<configured>',
    };
    const replacementCredentialRef = 'halo-pi-credential-v1-openai-replacement';
    const client: PiConfigurationClient = {
      writeCredential: vi.fn(async () => ({ credentialRef: replacementCredentialRef })),
      deleteCredential: vi.fn(async () => undefined),
      readSnapshot: vi.fn()
        .mockResolvedValueOnce(previousConfiguration)
        .mockResolvedValueOnce({
          ...previousConfiguration,
          modelId: 'gpt-5',
          credentialRef: replacementCredentialRef,
        }),
      createConfiguration: vi.fn(async () => undefined),
      updateConfiguration: vi.fn(async () => undefined),
      deleteConfiguration: vi.fn(async () => undefined),
      rollbackConfiguration: vi.fn(async () => undefined),
      checkReadiness: vi.fn(async () => ({ available: true })),
    };

    await act(async () => {
      root.render(<PiConfigurationPanel client={client} />);
    });
    await vi.waitFor(() => {
      expect(container.querySelector<HTMLInputElement>('[data-testid="pi-configuration-model"]')?.value)
        .toBe('gpt-4.1');
    });

    const setValue = (testId: string, value: string) => {
      const input = container.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`);
      expect(input).not.toBeNull();
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setter?.call(input, value);
      input?.dispatchEvent(new Event('input', { bubbles: true }));
      input?.dispatchEvent(new Event('change', { bubbles: true }));
    };
    await act(async () => {
      setValue('pi-configuration-model', 'gpt-5');
      setValue('pi-configuration-secret', 'synthetic-replacement');
    });
    const form = container.querySelector<HTMLFormElement>('[data-testid="pi-configuration-form"]');
    await act(async () => {
      form?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    });

    await vi.waitFor(() => {
      expect(client.updateConfiguration).toHaveBeenCalledWith({
        providerId: 'openai',
        baseUrl: null,
        modelId: 'gpt-5',
        thinkingLevel: 'medium',
        startupOptions: { noExtensions: true, noApprove: true },
        credentialRef: replacementCredentialRef,
      });
      expect(client.deleteCredential).not.toHaveBeenCalled();
    });
  });

  it('requires a new credential when the selected provider changes', async () => {
    const client: PiConfigurationClient = {
      writeCredential: vi.fn(async () => ({ credentialRef: 'halo-pi-credential-v1-new' })),
      deleteCredential: vi.fn(async () => undefined),
      readSnapshot: vi.fn(async () => ({
        providerId: 'openai',
        modelId: 'gpt-4.1',
        thinkingLevel: 'medium',
        startupOptions: { noExtensions: true, noApprove: true },
        credentialRef: 'halo-pi-credential-v1-openai-current',
        baseUrlHint: null,
      })),
      createConfiguration: vi.fn(async () => undefined),
      updateConfiguration: vi.fn(async () => undefined),
      deleteConfiguration: vi.fn(async () => undefined),
      rollbackConfiguration: vi.fn(async () => undefined),
      checkReadiness: vi.fn(async () => ({ available: true })),
    };
    await act(async () => {
      root.render(<PiConfigurationPanel client={client} />);
    });
    await vi.waitFor(() => {
      expect(container.querySelector('[data-testid="pi-configuration-provider"]')).not.toBeNull();
    });

    const setValue = (testId: string, value: string) => {
      const input = container.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`);
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setter?.call(input, value);
      input?.dispatchEvent(new Event('input', { bubbles: true }));
      input?.dispatchEvent(new Event('change', { bubbles: true }));
    };
    await act(async () => {
      setValue('pi-configuration-provider', 'anthropic');
      setValue('pi-configuration-model', 'claude');
    });
    const form = container.querySelector<HTMLFormElement>('[data-testid="pi-configuration-form"]');
    await act(async () => {
      form?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    });

    await vi.waitFor(() => {
      expect(container.textContent).toContain(
        'nav.sessions.workbenchRuntime.piConfiguration.providerCredentialRequired',
      );
    });
    expect(client.writeCredential).not.toHaveBeenCalled();
    expect(client.updateConfiguration).not.toHaveBeenCalled();
  });

  it('clears the one-shot credential when persistence fails', async () => {
    const client: PiConfigurationClient = {
      writeCredential: vi.fn(async () => ({ credentialRef: 'halo-pi-credential-v1-new' })),
      deleteCredential: vi.fn(async () => undefined),
      readSnapshot: vi.fn(async () => null),
      createConfiguration: vi.fn(async () => { throw new Error('synthetic persistence failure'); }),
      updateConfiguration: vi.fn(async () => undefined),
      deleteConfiguration: vi.fn(async () => undefined),
      rollbackConfiguration: vi.fn(async () => undefined),
      checkReadiness: vi.fn(async () => ({ available: true })),
    };
    await act(async () => {
      root.render(<PiConfigurationPanel client={client} />);
    });
    await vi.waitFor(() => {
      expect(container.querySelector('[data-testid="pi-configuration-provider"]')).not.toBeNull();
    });

    const setValue = (testId: string, value: string) => {
      const input = container.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`);
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setter?.call(input, value);
      input?.dispatchEvent(new Event('input', { bubbles: true }));
      input?.dispatchEvent(new Event('change', { bubbles: true }));
    };
    await act(async () => {
      setValue('pi-configuration-provider', 'openai');
      setValue('pi-configuration-model', 'gpt-5');
      setValue('pi-configuration-secret', 'synthetic-one-shot');
    });
    const form = container.querySelector<HTMLFormElement>('[data-testid="pi-configuration-form"]');
    await act(async () => {
      form?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    });

    await vi.waitFor(() => {
      expect(container.querySelector<HTMLInputElement>('[data-testid="pi-configuration-secret"]')?.value)
        .toBe('');
    });
  });
});
