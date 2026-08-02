/**
 * useCurrentSessionTitle — returns the active FlowChat session title.
 * Subscribes to flowChatStore so the value updates reactively.
 */

import { useState, useEffect } from 'react';
import { isHaloLocalCodingScope } from '@/infrastructure/runtime';

export function useCurrentSessionTitle(): string {
  const [title, setTitle] = useState('');

  useEffect(() => {
    if (isHaloLocalCodingScope()) {
      return;
    }

    let disposed = false;
    let unsubscribe = () => {};

    void import('../../flow_chat/store/FlowChatStore').then(({ flowChatStore }) => {
      if (disposed) return;

      const readTitle = (state: ReturnType<typeof flowChatStore.getState>): string => {
        const session = state.activeSessionId ? state.sessions.get(state.activeSessionId) : undefined;
        return session?.title ?? '';
      };

      setTitle(readTitle(flowChatStore.getState()));
      unsubscribe = flowChatStore.subscribe(state => {
        if (!disposed) {
          setTitle(readTitle(state));
        }
      });
    });

    return () => {
      disposed = true;
      unsubscribe();
    };
  }, []);

  return title;
}
