import React, { type FormEvent, useEffect, useRef, useState } from 'react';
import { CheckCircle2, KeyRound, Loader2, Settings2 } from 'lucide-react';

import { useI18n } from '@/infrastructure/i18n';
import {
  createPiConfigurationClient,
  createTauriPiConfigurationTransport,
  type PiConfigurationClient,
  type PiConfigurationSnapshot,
  type PiThinkingLevel,
} from '@/infrastructure/pi-configuration';

import './PiConfigurationPanel.scss';

const DEFAULT_THINKING_LEVEL: PiThinkingLevel = 'medium';
const MANAGED_STARTUP_OPTIONS = { noExtensions: true, noApprove: true } as const;

const defaultClient = createPiConfigurationClient(createTauriPiConfigurationTransport());

interface PiConfigurationPanelProps {
  client?: PiConfigurationClient;
  onConfigured?: () => Promise<void> | void;
  version?: string | null;
  profile?: string | null;
  verifiedCapabilities?: number;
  requiredCapabilities?: number;
  runtimeReady?: boolean;
}

const PiConfigurationPanel: React.FC<PiConfigurationPanelProps> = ({
  client = defaultClient,
  onConfigured,
  version = null,
  profile = null,
  verifiedCapabilities = 0,
  requiredCapabilities = 0,
  runtimeReady = false,
}) => {
  const { t } = useI18n('common');
  const [configuration, setConfiguration] = useState<PiConfigurationSnapshot | null>(null);
  const [providerId, setProviderId] = useState('');
  const [modelId, setModelId] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [thinkingLevel, setThinkingLevel] = useState<PiThinkingLevel>(DEFAULT_THINKING_LEVEL);
  const secretInputRef = useRef<HTMLInputElement>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [statusKey, setStatusKey] = useState<string | null>(null);
  const [errorKey, setErrorKey] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void client.readSnapshot()
      .then(snapshot => {
        if (!active) return;
        setConfiguration(snapshot);
        setProviderId(snapshot?.providerId ?? '');
        setModelId(snapshot?.modelId ?? '');
        setThinkingLevel(snapshot?.thinkingLevel ?? DEFAULT_THINKING_LEVEL);
        setStatusKey(snapshot
          ? 'nav.sessions.workbenchRuntime.piConfiguration.configurationSaved'
          : 'nav.sessions.workbenchRuntime.piConfiguration.configurationRequired');
      })
      .catch(() => {
        if (active) setErrorKey('nav.sessions.workbenchRuntime.piConfiguration.loadFailed');
      })
      .finally(() => {
        if (active) setIsLoading(false);
      });
    return () => { active = false; };
  }, [client]);

  const saveConfiguration = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (isSaving) return;

    const normalizedProviderId = providerId.trim();
    const normalizedModelId = modelId.trim();
    const normalizedBaseUrl = baseUrl.trim();
    const secretInput = secretInputRef.current;
    const normalizedSecret = secretInput?.value.trim() ?? '';
    if (secretInput) {
      // One-shot credential entry: the value is consumed from the DOM and
      // never retained in React state or across failed submissions.
      secretInput.value = '';
    }
    if (!normalizedProviderId || !normalizedModelId || (!configuration && !normalizedSecret)) {
      setErrorKey('nav.sessions.workbenchRuntime.piConfiguration.requiredFields');
      return;
    }
    if (configuration && configuration.providerId !== normalizedProviderId && !normalizedSecret) {
      setErrorKey('nav.sessions.workbenchRuntime.piConfiguration.providerCredentialRequired');
      return;
    }

    setIsSaving(true);
    setErrorKey(null);
    let createdCredentialRef: string | null = null;
    try {
      const credentialRef = normalizedSecret
        ? (await client.writeCredential(normalizedProviderId, normalizedSecret)).credentialRef
        : configuration?.credentialRef;
      if (!credentialRef) {
        setErrorKey('nav.sessions.workbenchRuntime.piConfiguration.requiredFields');
        return;
      }
      createdCredentialRef = normalizedSecret ? credentialRef : null;
      const nextConfiguration = {
        providerId: normalizedProviderId,
        baseUrl: normalizedBaseUrl || null,
        modelId: normalizedModelId,
        thinkingLevel,
        startupOptions: MANAGED_STARTUP_OPTIONS,
        credentialRef,
      };
      if (configuration) {
        await client.updateConfiguration(nextConfiguration);
      } else {
        await client.createConfiguration(nextConfiguration);
      }
      createdCredentialRef = null;

      const readiness = await client.checkReadiness();
      if (!readiness.available) {
        setErrorKey('nav.sessions.workbenchRuntime.piConfiguration.notReady');
        return;
      }

      const refreshed = await client.readSnapshot();
      setConfiguration(refreshed ?? {
        providerId: normalizedProviderId,
        modelId: normalizedModelId,
        thinkingLevel,
        startupOptions: MANAGED_STARTUP_OPTIONS,
        credentialRef,
        baseUrlHint: normalizedBaseUrl ? '<configured>' : null,
      });
      setBaseUrl('');
      setStatusKey('nav.sessions.workbenchRuntime.piConfiguration.configurationReady');
      await onConfigured?.();
    } catch {
      if (createdCredentialRef) {
        await client.deleteCredential(normalizedProviderId, createdCredentialRef).catch(() => undefined);
      }
      setErrorKey('nav.sessions.workbenchRuntime.piConfiguration.saveFailed');
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <section className="halo-pi-configuration" data-testid="pi-configuration-panel">
      <header className="halo-pi-configuration__header">
        <div>
          <h2>{t('nav.sessions.workbenchRuntime.piConfiguration.title')}</h2>
          <p>{t('nav.sessions.workbenchRuntime.piConfiguration.description')}</p>
        </div>
        <Settings2 size={18} aria-hidden="true" />
      </header>

      <div className="halo-pi-configuration__status" data-testid="pi-configuration-status">
        <span>{t(runtimeReady
          ? 'nav.sessions.workbenchRuntime.piConfiguration.runtimeReady'
          : 'nav.sessions.workbenchRuntime.piConfiguration.runtimeUnavailable')}</span>
        {version ? <span>{t('nav.sessions.workbenchRuntime.piConfiguration.version', { version })}</span> : null}
        {profile ? <span>{t('nav.sessions.workbenchRuntime.piConfiguration.profile', { profile })}</span> : null}
        {requiredCapabilities > 0 ? (
          <span>{t('nav.sessions.workbenchRuntime.piConfiguration.capabilities', {
            verified: verifiedCapabilities,
            required: requiredCapabilities,
          })}</span>
        ) : null}
      </div>

      <form className="halo-pi-configuration__form" data-testid="pi-configuration-form" onSubmit={saveConfiguration}>
        <label>
          <span>{t('nav.sessions.workbenchRuntime.piConfiguration.provider')}</span>
          <input
            value={providerId}
            onChange={event => setProviderId(event.target.value)}
            autoComplete="off"
            maxLength={256}
            disabled={isLoading || isSaving}
            data-testid="pi-configuration-provider"
          />
        </label>
        <label>
          <span>{t('nav.sessions.workbenchRuntime.piConfiguration.model')}</span>
          <input
            value={modelId}
            onChange={event => setModelId(event.target.value)}
            autoComplete="off"
            maxLength={256}
            disabled={isLoading || isSaving}
            data-testid="pi-configuration-model"
          />
        </label>
        <label>
          <span>{t('nav.sessions.workbenchRuntime.piConfiguration.baseUrl')}</span>
          <input
            value={baseUrl}
            onChange={event => setBaseUrl(event.target.value)}
            autoComplete="off"
            inputMode="url"
            maxLength={2048}
            disabled={isLoading || isSaving}
            data-testid="pi-configuration-base-url"
          />
          {configuration?.baseUrlHint ? (
            <small>{t('nav.sessions.workbenchRuntime.piConfiguration.baseUrlConfigured')}</small>
          ) : null}
        </label>
        <label>
          <span>{t('nav.sessions.workbenchRuntime.piConfiguration.thinking')}</span>
          <select
            value={thinkingLevel}
            onChange={event => setThinkingLevel(event.target.value as PiThinkingLevel)}
            disabled={isLoading || isSaving}
            data-testid="pi-configuration-thinking"
          >
            {(['off', 'minimal', 'low', 'medium', 'high'] as const).map(level => (
              <option key={level} value={level}>{t(`nav.sessions.workbenchRuntime.piConfiguration.thinkingLevels.${level}`)}</option>
            ))}
          </select>
        </label>
        <label>
          <span>{t('nav.sessions.workbenchRuntime.piConfiguration.credential')}</span>
          <input
            ref={secretInputRef}
            type="password"
            autoComplete="new-password"
            maxLength={512 * 1024}
            disabled={isLoading || isSaving}
            data-testid="pi-configuration-secret"
          />
          <small>{t(configuration
            ? 'nav.sessions.workbenchRuntime.piConfiguration.credentialOptional'
            : 'nav.sessions.workbenchRuntime.piConfiguration.credentialRequired')}</small>
        </label>
        <p className="halo-pi-configuration__policy">
          <KeyRound size={13} aria-hidden="true" />
          <span>{t('nav.sessions.workbenchRuntime.piConfiguration.managedPolicy')}</span>
        </p>
        <button type="submit" disabled={isLoading || isSaving} data-testid="pi-configuration-save">
          {isSaving || isLoading ? <Loader2 size={14} className="is-spinning" aria-hidden="true" /> : <CheckCircle2 size={14} aria-hidden="true" />}
          <span>{t('nav.sessions.workbenchRuntime.piConfiguration.save')}</span>
        </button>
      </form>

      {statusKey ? <p className="halo-pi-configuration__notice" role="status">{t(statusKey)}</p> : null}
      {errorKey ? <p className="halo-pi-configuration__error" role="alert">{t(errorKey)}</p> : null}
    </section>
  );
};

export default PiConfigurationPanel;
