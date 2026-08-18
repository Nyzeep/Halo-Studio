import React, { forwardRef } from 'react';
import './Switch.scss';

export interface SwitchProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'size'> {
  label?: string;
  description?: string;
  size?: 'small' | 'medium' | 'large';
  loading?: boolean;
  checkedText?: string;
  uncheckedText?: string;
  className?: string;
}

export const Switch = forwardRef<HTMLInputElement, SwitchProps>(
  (
    {
      label,
      description,
      size = 'medium',
      loading = false,
      checkedText,
      uncheckedText,
      disabled = false,
      className = '',
      checked,
      children,
      ...props
    },
    ref
  ) => {
    const isDisabled = disabled || loading;

    const containerClass = [
      'halo-switch',
      `halo-switch--${size}`,
      isDisabled && 'halo-switch--disabled',
      loading && 'halo-switch--loading',
      className
    ].filter(Boolean).join(' ');

    const switchClass = [
      'halo-switch__track',
      checked && 'halo-switch__track--checked'
    ].filter(Boolean).join(' ');

    return (
      <label className={containerClass}>
        <div className="halo-switch__wrapper">
          <input
            ref={ref}
            type="checkbox"
            className="halo-switch__input"
            disabled={isDisabled}
            checked={checked}
            {...props}
          />
          <span className={switchClass}>
            {(checkedText || uncheckedText) && (
              <span className="halo-switch__text">
                {checked ? checkedText : uncheckedText}
              </span>
            )}
            <span className="halo-switch__thumb">
              {loading && (
                <svg className="halo-switch__loading" viewBox="0 0 16 16">
                  <circle
                    cx="8"
                    cy="8"
                    r="6"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeDasharray="31.4"
                    strokeDashoffset="10"
                  />
                </svg>
              )}
            </span>
          </span>
        </div>
        {(label || description || children) && (
          <div className="halo-switch__content">
            {label && <span className="halo-switch__label">{label}</span>}
            {description && <span className="halo-switch__description">{description}</span>}
            {children}
          </div>
        )}
      </label>
    );
  }
);

Switch.displayName = 'Switch';