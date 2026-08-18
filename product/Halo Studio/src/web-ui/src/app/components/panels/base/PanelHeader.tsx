/**
 * Generic panel header with centered title and optional action buttons.
 */

import React from 'react';
import './PanelHeader.scss';

export interface PanelHeaderProps {
  title: string;
  actions?: React.ReactNode;
  className?: string;
}

export const PanelHeader: React.FC<PanelHeaderProps> = ({
  title,
  actions,
  className = '',
}) => {
  return (
    <div className={`halo-panel-header ${className}`}>
      <h3 className="halo-panel-header__title">{title}</h3>
      {actions && (
        <div className="halo-panel-header__actions">
          {actions}
        </div>
      )}
    </div>
  );
};

PanelHeader.displayName = 'PanelHeader';

export default PanelHeader;
