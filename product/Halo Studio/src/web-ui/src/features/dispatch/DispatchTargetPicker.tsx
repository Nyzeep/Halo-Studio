import React, { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, ChevronDown, Laptop } from 'lucide-react';
import { Tooltip } from '@/component-library';
import { useI18n } from '@/infrastructure/i18n';
import { computeFixedPopoverPosition } from '@/shared/utils/fixedPopoverViewport';
import type {
  DispatchSelection,
  DispatchTarget,
} from './types';
import './DispatchTargetPicker.scss';

interface DispatchTargetPickerProps {
  target: DispatchTarget;
  locked: boolean;
  disabled?: boolean;
  onSelectLocal?: () => void;
  onSelectSsh: (selection: DispatchSelection) => void;
}

export const DispatchTargetPicker: React.FC<DispatchTargetPickerProps> = ({
  locked,
  disabled = false,
  onSelectLocal,
}) => {
  const { t } = useI18n('flow-chat');
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [menuPosition, setMenuPosition] = useState({ top: 0, left: 0 });

  const displayLabel = t('chatInput.dispatch.local');
  const tooltip = locked
    ? t('chatInput.dispatch.locked', { target: displayLabel })
    : t('chatInput.dispatch.current', { target: displayLabel });

  const updatePosition = useCallback(() => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) return;
    const width = menuRef.current?.offsetWidth ?? 300;
    const height = menuRef.current?.offsetHeight ?? 340;
    setMenuPosition(computeFixedPopoverPosition(rect, width, height, 7, 8));
  }, []);

  useEffect(() => {
    if (!open) return;
    updatePosition();
    const frame = requestAnimationFrame(updatePosition);
    window.addEventListener('resize', updatePosition);
    window.addEventListener('scroll', updatePosition, true);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('resize', updatePosition);
      window.removeEventListener('scroll', updatePosition, true);
    };
  }, [open, updatePosition]);

  useEffect(() => {
    if (!open) return;
    const handlePointerDown = (event: PointerEvent) => {
      const node = event.target as Node;
      if (!rootRef.current?.contains(node) && !menuRef.current?.contains(node)) {
        setOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('pointerdown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

  const menu = open ? createPortal(
    <div
      ref={menuRef}
      className="dispatch-target-picker__menu"
      role="menu"
      aria-label={t('chatInput.dispatch.menuLabel')}
      style={{ top: menuPosition.top, left: menuPosition.left }}
      data-testid="dispatch-target-menu"
    >
      <div className="dispatch-target-picker__header">
        <span>{t('chatInput.dispatch.menuLabel')}</span>
        <small>{t('chatInput.dispatch.sessionScope')}</small>
      </div>
      <div className="dispatch-target-picker__section">
        <div className="dispatch-target-picker__section-title">
          {t('chatInput.dispatch.localSection')}
        </div>
        <button
          type="button"
          role="menuitemradio"
          aria-checked={true}
          className="dispatch-target-picker__option"
          onClick={() => {
            setOpen(false);
            onSelectLocal?.();
          }}
        >
          <Laptop size={15} aria-hidden />
          <span>
            <strong>{t('chatInput.dispatch.local')}</strong>
            <small>{t('chatInput.dispatch.localDescription')}</small>
          </span>
          <Check size={14} aria-hidden />
        </button>
      </div>
    </div>,
    document.body,
  ) : null;

  return (
    <>
      <div ref={rootRef} className="dispatch-target-picker">
        <Tooltip content={tooltip} placement="top">
          <button
            ref={triggerRef}
            type="button"
            className="dispatch-target-picker__trigger"
            aria-haspopup="menu"
            aria-expanded={open}
            aria-label={tooltip}
            disabled={disabled || locked}
            data-testid="chat-input-dispatch-trigger"
            data-dispatch-kind="local"
            onClick={event => {
              event.stopPropagation();
              setOpen(current => !current);
            }}
          >
            <Laptop size={12} />
            <span>{displayLabel}</span>
            {!locked ? (
              <ChevronDown
                className="dispatch-target-picker__chevron"
                size={11}
                aria-hidden
              />
            ) : null}
          </button>
        </Tooltip>
        {menu}
      </div>
    </>
  );
};
