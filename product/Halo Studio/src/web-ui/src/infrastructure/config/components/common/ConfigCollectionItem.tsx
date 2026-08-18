import React, { useId, useState } from 'react';
import { ChevronDown } from 'lucide-react';
import './ConfigCollectionItem.scss';

export interface ConfigCollectionItemProps extends React.HTMLAttributes<HTMLDivElement> {
  label: React.ReactNode;
  badge?: React.ReactNode;
  badgePlacement?: 'inline' | 'below';
  control: React.ReactNode;
  details?: React.ReactNode;
  disabled?: boolean;
  expanded?: boolean;
  onToggle?: () => void;
  className?: string;
}

export const ConfigCollectionItem: React.FC<ConfigCollectionItemProps> = ({
  label,
  badge,
  badgePlacement = 'inline',
  control,
  details,
  disabled = false,
  expanded: expandedProp,
  onToggle,
  className = '',
  ...rootProps
}) => {
  const [internalExpanded, setInternalExpanded] = useState(false);
  const labelId = useId();
  const detailsId = useId();
  const isControlled = expandedProp !== undefined;
  const isExpanded = isControlled ? expandedProp : internalExpanded;
  const hasDetails = Boolean(details);

  const toggleDetails = () => {
    if (!hasDetails || disabled) return;
    if (isControlled) {
      onToggle?.();
    } else {
      setInternalExpanded((prev) => !prev);
    }
  };

  return (
    <div
      {...rootProps}
      className={`halo-collection-item ${isExpanded ? 'is-expanded' : ''} ${disabled ? 'is-disabled' : ''} ${className}`}
    >
      <div className="halo-config-page-row halo-config-page-row--center halo-collection-item__row">
        <div className="halo-config-page-row__meta">
          <div
            className={`halo-config-page-row__label halo-collection-item__label ${
              badgePlacement === 'below' ? 'halo-collection-item__label--stacked' : ''
            }`}
          >
            <span id={labelId} className="halo-collection-item__name">{label}</span>
            {badge && (
              <span
                className={`halo-collection-item__badges ${
                  badgePlacement === 'below'
                    ? 'halo-collection-item__badges--stacked'
                    : 'halo-collection-item__badges--inline'
                }`}
              >
                {badge}
              </span>
            )}
          </div>
        </div>
        <div className="halo-config-page-row__control">
          <div className="halo-collection-item__control">
            {control}
            {hasDetails ? (
              <button
                type="button"
                className="halo-collection-btn halo-collection-item__details-toggle"
                onClick={toggleDetails}
                disabled={disabled}
                aria-labelledby={labelId}
                aria-expanded={isExpanded}
                aria-controls={detailsId}
              >
                <ChevronDown size={14} aria-hidden="true" />
              </button>
            ) : null}
          </div>
        </div>
      </div>

      {isExpanded && details && (
        <div id={detailsId} className="halo-collection-item__details">{details}</div>
      )}
    </div>
  );
};

export default ConfigCollectionItem;
