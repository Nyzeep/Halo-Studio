/**
 * Page object for header (title bar and window controls).
 */
import { BasePage } from '../BasePage';
import { $ } from '@wdio/globals';

export class Header extends BasePage {
  private selectors = {
    container: '.halo-nav-panel, .halo-scene-bar, .halo-nav-bar, [data-testid="header-container"], .halo-header, header',
    homeBtn: '[data-testid="header-home-btn"], .halo-nav-bar__logo-button, .halo-header__home',
    minimizeBtn: '[data-testid="header-minimize-btn"], .halo-title-bar__minimize, .window-controls__btn--minimize, button[aria-label*="Minimize"], button[aria-label*="最小化"]',
    maximizeBtn: '[data-testid="header-maximize-btn"], .halo-title-bar__maximize, .window-controls__btn--maximize, button[aria-label*="Maximize"], button[aria-label*="最大化"], button[aria-label*="Restore"], button[aria-label*="还原"]',
    closeBtn: '[data-testid="header-close-btn"], .halo-title-bar__close, .window-controls__btn--close, button[aria-label*="Close"], button[aria-label*="关闭"]',
    leftPanelToggle: '[data-testid="header-left-panel-toggle"], .halo-nav-bar__panel-toggle',
    rightPanelToggle: '[data-testid="header-right-panel-toggle"]',
    newSessionBtn: '[data-testid="header-new-session-btn"]',
    title: '[data-testid="header-title"], .halo-nav-bar__menu-item-main, .halo-header__title',
    configBtn: '[data-testid="header-config-btn"], .halo-header-right button',
  };

  private async findExistingElement(selectors: string[]): Promise<WebdriverIO.Element | null> {
    for (const selector of selectors) {
      try {
        const element = await $(selector);
        if (await element.isExisting()) {
          return element;
        }
      } catch (error) {
        // Try the next selector.
      }
    }

    return null;
  }

  async isVisible(): Promise<boolean> {
    const selectors = ['.halo-nav-panel', '.halo-scene-bar', '.halo-nav-bar', '[data-testid="header-container"]', '.halo-header', 'header'];
    for (const selector of selectors) {
      try {
        const element = await $(selector);
        const exists = await element.isExisting();
        if (exists) {
          return true;
        }
      } catch (e) {
        // Continue
      }
    }
    return false;
  }

  async waitForLoad(): Promise<void> {
    const selectors = ['.halo-nav-panel', '.halo-scene-bar', '.halo-nav-bar', '[data-testid="header-container"]', '.halo-header', 'header'];
    for (const selector of selectors) {
      try {
        const element = await $(selector);
        const exists = await element.isExisting();
        if (exists) {
          return;
        }
      } catch (e) {
        // Continue
      }
    }
    // Fallback wait
    await this.wait(2000);
  }

  async clickHome(): Promise<void> {
    await this.safeClick(this.selectors.homeBtn);
  }

  async isHomeButtonVisible(): Promise<boolean> {
    return this.isElementVisible(this.selectors.homeBtn);
  }

  async clickMinimize(): Promise<void> {
    const element = await this.findExistingElement([
      '[data-testid="header-minimize-btn"]',
      '.halo-title-bar__minimize',
      '.window-controls__btn--minimize',
      'button[aria-label*="Minimize"]',
      'button[aria-label*="最小化"]',
      '.window-controls button:first-child',
    ]);

    if (!element) {
      throw new Error('Minimize button not found');
    }

    await element.scrollIntoView();
    await element.waitForClickable({ timeout: 10000 });
    await element.click();
  }

  async isMinimizeButtonVisible(): Promise<boolean> {
    return (await this.findExistingElement([
      '[data-testid="header-minimize-btn"]',
      '.halo-title-bar__minimize',
      '.window-controls__btn--minimize',
      'button[aria-label*="Minimize"]',
      'button[aria-label*="最小化"]',
      '.window-controls button:first-child',
    ])) !== null;
  }

  async clickMaximize(): Promise<void> {
    const element = await this.findExistingElement([
      '[data-testid="header-maximize-btn"]',
      '.halo-title-bar__maximize',
      '.window-controls__btn--maximize',
      'button[aria-label*="Maximize"]',
      'button[aria-label*="最大化"]',
      'button[aria-label*="Restore"]',
      'button[aria-label*="还原"]',
      '.window-controls button:nth-child(2)',
    ]);

    if (!element) {
      throw new Error('Maximize button not found');
    }

    await element.scrollIntoView();
    await element.waitForClickable({ timeout: 10000 });
    await element.click();
  }

  async isMaximizeButtonVisible(): Promise<boolean> {
    return (await this.findExistingElement([
      '[data-testid="header-maximize-btn"]',
      '.halo-title-bar__maximize',
      '.window-controls__btn--maximize',
      'button[aria-label*="Maximize"]',
      'button[aria-label*="最大化"]',
      'button[aria-label*="Restore"]',
      'button[aria-label*="还原"]',
      '.window-controls button:nth-child(2)',
    ])) !== null;
  }

  async clickClose(): Promise<void> {
    const element = await this.findExistingElement([
      '[data-testid="header-close-btn"]',
      '.halo-title-bar__close',
      '.window-controls__btn--close',
      'button[aria-label*="Close"]',
      'button[aria-label*="关闭"]',
      '.window-controls button:last-child',
    ]);

    if (!element) {
      throw new Error('Close button not found');
    }

    await element.scrollIntoView();
    await element.waitForClickable({ timeout: 10000 });
    await element.click();
  }

  async isCloseButtonVisible(): Promise<boolean> {
    return (await this.findExistingElement([
      '[data-testid="header-close-btn"]',
      '.halo-title-bar__close',
      '.window-controls__btn--close',
      'button[aria-label*="Close"]',
      'button[aria-label*="关闭"]',
      '.window-controls button:last-child',
    ])) !== null;
  }

  async toggleLeftPanel(): Promise<void> {
    await this.safeClick(this.selectors.leftPanelToggle);
  }

  async toggleRightPanel(): Promise<void> {
    await this.safeClick(this.selectors.rightPanelToggle);
  }

  async clickNewSession(): Promise<void> {
    await this.safeClick(this.selectors.newSessionBtn);
  }

  async isNewSessionButtonVisible(): Promise<boolean> {
    return this.isElementVisible(this.selectors.newSessionBtn);
  }

  async getTitle(): Promise<string> {
    try {
      const element = await $(this.selectors.title);
      const exists = await element.isExisting();
      if (exists) {
        return await element.getText();
      }
    } catch (e) {
      // Return empty string
    }
    return '';
  }

  async areWindowControlsVisible(): Promise<boolean> {
    const controlsContainer = await this.findExistingElement([
      '.window-controls',
      '.halo-header-right .window-controls',
    ]);

    if (controlsContainer) {
      return true;
    }

    const minimizeVisible = await this.isMinimizeButtonVisible();
    const maximizeVisible = await this.isMaximizeButtonVisible();
    const closeVisible = await this.isCloseButtonVisible();

    return minimizeVisible || maximizeVisible || closeVisible;
  }
}

export default Header;
