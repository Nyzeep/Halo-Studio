import React, { lazy, Suspense, useCallback, useState } from 'react';
import {
  ChevronUp,
  Info,
  MoreVertical,
  Settings,
  SquareTerminal,
  Terminal,
} from 'lucide-react';
import { Tooltip } from '@/component-library';
import { useI18n } from '@/infrastructure/i18n/hooks/useI18n';
import { useSceneManager } from '../../../hooks/useSceneManager';
import { useNavSceneStore } from '../../../stores/navSceneStore';
import NotificationButton from '../../TitleBar/NotificationButton';

const AboutDialog = lazy(() =>
  import('../../AboutDialog').then(module => ({ default: module.AboutDialog }))
);

const PersistentFooterActions: React.FC = () => {
  const { t } = useI18n('common');
  const { openScene } = useSceneManager();
  const showSceneNav = useNavSceneStore((s) => s.showSceneNav);
  const navSceneId = useNavSceneStore((s) => s.navSceneId);
  const openNavScene = useNavSceneStore((s) => s.openNavScene);
  const closeNavScene = useNavSceneStore((s) => s.closeNavScene);

  const [menuOpen, setMenuOpen] = useState(false);
  const [menuClosing, setMenuClosing] = useState(false);
  const [showAbout, setShowAbout] = useState(false);

  const closeMenu = useCallback(() => {
    setMenuClosing(true);
    setTimeout(() => {
      setMenuOpen(false);
      setMenuClosing(false);
    }, 150);
  }, []);

  const toggleMenu = () => {
    if (menuOpen) {
      closeMenu();
    } else {
      setMenuOpen(true);
    }
  };

  const handleOpenSettings = () => {
    closeMenu();
    openScene('settings');
  };

  const handleOpenShell = useCallback(() => {
    if (showSceneNav && navSceneId === 'shell') {
      closeNavScene();
      return;
    }
    openNavScene('shell');
  }, [closeNavScene, navSceneId, openNavScene, showSceneNav]);

  const handleShowAbout = () => {
    closeMenu();
    setShowAbout(true);
  };

  return (
    <>
      <div className="bitfun-nav-panel__footer">
        <div className="bitfun-nav-panel__footer-left">
          <div className="bitfun-nav-panel__footer-more-wrap">
            <Tooltip content={t('nav.moreOptions')} placement="right" followCursor disabled={menuOpen}>
              <button
                type="button"
                className={`bitfun-nav-panel__footer-btn bitfun-nav-panel__footer-btn--icon${menuOpen ? ' is-active' : ''}`}
                aria-label={t('nav.moreOptions')}
                aria-expanded={menuOpen}
                onClick={toggleMenu}
                data-testid="nav-footer-more-btn"
              >
                {menuOpen ? (
                  <MoreVertical size={15} aria-hidden="true" />
                ) : (
                  <span className="bitfun-nav-panel__footer-btn-icon-swap" aria-hidden="true">
                    <MoreVertical size={15} className="bitfun-nav-panel__footer-btn-icon-swap-default" />
                    <ChevronUp size={15} className="bitfun-nav-panel__footer-btn-icon-swap-hover" />
                  </span>
                )}
              </button>
            </Tooltip>

            {menuOpen && (
              <>
                <div
                  className="bitfun-nav-panel__footer-backdrop"
                  onClick={closeMenu}
                />
                <div
                  className={`bitfun-nav-panel__footer-menu${menuClosing ? ' is-closing' : ''}`}
                  role="menu"
                  data-testid="nav-footer-menu"
                >
                  <button
                    type="button"
                    className="bitfun-nav-panel__footer-menu-item"
                    role="menuitem"
                    onClick={handleOpenSettings}
                    data-testid="nav-footer-settings-item"
                  >
                    <Settings size={14} />
                    <span>{t('shared:features.settings')}</span>
                  </button>
                  <button
                    type="button"
                    className="bitfun-nav-panel__footer-menu-item"
                    role="menuitem"
                    onClick={handleShowAbout}
                  >
                    <Info size={14} />
                    <span>{t('header.about')}</span>
                  </button>
                </div>
              </>
            )}
          </div>

          <Tooltip content={t('scenes.shell')} placement="right">
            <button
              type="button"
              className={`bitfun-nav-panel__footer-btn bitfun-nav-panel__footer-btn--icon${showSceneNav && navSceneId === 'shell' ? ' is-active' : ''}`}
              aria-label={t('scenes.shell')}
              aria-pressed={showSceneNav && navSceneId === 'shell'}
              onClick={handleOpenShell}
              data-testid="shell-panel-entry"
            >
              <span className="bitfun-nav-panel__footer-btn-icon-swap" aria-hidden="true">
                <SquareTerminal size={15} className="bitfun-nav-panel__footer-btn-icon-swap-default" />
                <Terminal size={15} className="bitfun-nav-panel__footer-btn-icon-swap-hover" />
              </span>
            </button>
          </Tooltip>
        </div>

        <div className="bitfun-nav-panel__footer-right">
          <NotificationButton className="bitfun-nav-panel__footer-btn" navFooterHoverIconSwap />
        </div>
      </div>
      {showAbout && (
        <Suspense fallback={null}>
          <AboutDialog isOpen={showAbout} onClose={() => setShowAbout(false)} />
        </Suspense>
      )}
    </>
  );
};

export default PersistentFooterActions;
