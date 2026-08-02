import { useEffect, useRef } from 'react';
import { systemAPI } from '@/infrastructure/api/service-api/SystemAPI';
import { configManager } from '@/infrastructure/config/services/ConfigManager';
import { useI18n } from '@/infrastructure/i18n';
import { isHaloLocalCodingScope } from '@/infrastructure/runtime';
import { createLogger } from '@/shared/utils/logger';
import {
  buildDialogCompletionNotificationCopy,
  shouldSendDialogCompletionNotification,
} from './dialogCompletionNotifyPolicy';

const log = createLogger('useDialogCompletionNotify');

/**
 * Listens for dialog turn completion events and sends an OS-level desktop
 * notification (Windows toast / macOS notification center) when the window
 * is not focused and the feature is enabled in config.
 *
 * Notification title = session title (or short session id fallback).
 * Notification body  = fixed "task completed" message.
 *
 * "Not focused" means: the page is hidden (minimized / tab switched) OR
 * the window has lost focus to another OS-level application.
 */
export const useDialogCompletionNotify = () => {
  const { t } = useI18n('common');
  // Track whether the window currently has OS-level focus
  const windowFocusedRef = useRef(true);

  useEffect(() => {
    if (isHaloLocalCodingScope()) return;

    let disposed = false;
    let unlisten: (() => void) | null = null;
    const handleFocus = () => { windowFocusedRef.current = true; };
    const handleBlur = () => { windowFocusedRef.current = false; };

    window.addEventListener('focus', handleFocus);
    window.addEventListener('blur', handleBlur);

    void Promise.all([
      import('@/infrastructure/api/service-api/AgentAPI'),
      import('@/flow_chat/store/FlowChatStore'),
    ]).then(([{ agentAPI }, { flowChatStore }]) => {
      if (disposed) return;
      unlisten = agentAPI.onDialogTurnCompleted(async (event) => {
        const isBackground = document.hidden || !windowFocusedRef.current;

        let enabled = true;
        try {
          enabled = await configManager.getConfig<boolean>(
            'app.notifications.dialog_completion_notify'
          );
        } catch (error) {
          log.warn('Failed to read dialog_completion_notify config', error);
        }

        const sessionId: string = event?.sessionId ?? '';
        const session = sessionId
          ? flowChatStore.getState().sessions.get(sessionId)
          : undefined;
        if (
          !shouldSendDialogCompletionNotification({
            event,
            session,
            isBackground,
            notificationsEnabled: enabled,
          })
        ) {
          return;
        }

        const notificationCopy = buildDialogCompletionNotificationCopy({
          sessionTitle: session?.title,
          success: event?.success,
          finishReason: event?.finishReason ?? event?.finish_reason,
          t,
        });

        await systemAPI.sendSystemNotification(
          notificationCopy.title,
          notificationCopy.body,
        );
      });
    }).catch(error => {
      if (!disposed) log.warn('Failed to initialize dialog completion notifications', error);
    });

    return () => {
      disposed = true;
      window.removeEventListener('focus', handleFocus);
      window.removeEventListener('blur', handleBlur);
      unlisten?.();
    };
  }, [t]);
};
