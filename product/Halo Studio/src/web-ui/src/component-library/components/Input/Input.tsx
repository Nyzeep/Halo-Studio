/**
 * Input component
 */

import React, { forwardRef } from 'react';
import './Input.scss';

export interface InputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'size' | 'prefix'> {
  variant?: 'default' | 'filled' | 'outlined';
  inputSize?: 'small' | 'medium' | 'large';
  size?: 'small' | 'medium' | 'large';
  error?: boolean;
  errorMessage?: string;
  prefix?: React.ReactNode;
  suffix?: React.ReactNode;
  label?: string;
  hint?: React.ReactNode;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(({
  variant = 'default',
  inputSize = 'medium',
  size,
  error = false,
  errorMessage,
  prefix,
  suffix,
  label,
  hint,
  className = '',
  disabled,
  ...props
}, ref) => {
  const resolvedInputSize = size ?? inputSize;
  const classNames = [
    'halo-input-wrapper',
    `halo-input-wrapper--${variant}`,
    `halo-input-wrapper--${resolvedInputSize}`,
    error && 'halo-input-wrapper--error',
    disabled && 'halo-input-wrapper--disabled',
    className
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <div className={classNames}>
      {label && <label className="halo-input-label">{label}</label>}
      <div className="halo-input-container">
        {prefix && <span className="halo-input-prefix">{prefix}</span>}
        <input
          ref={ref}
          className="halo-input"
          disabled={disabled}
          {...props}
        />
        {suffix && <span className="halo-input-suffix">{suffix}</span>}
      </div>
      {!error && hint && (
        <span className="halo-input-error-message">{hint}</span>
      )}
      {error && errorMessage && (
        <span className="halo-input-error-message">{errorMessage}</span>
      )}
    </div>
  );
});

Input.displayName = 'Input';
