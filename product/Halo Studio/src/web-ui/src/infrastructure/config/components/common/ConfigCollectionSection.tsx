import React from 'react';
import { ConfigPageSection } from './ConfigPageLayout';
import './ConfigCollectionSection.scss';

export interface ConfigCollectionSectionProps {
  title: string;
  description?: string;
  toolbar?: React.ReactNode;
  filters?: React.ReactNode;
  editor?: React.ReactNode;
  className?: string;
  children: React.ReactNode;
}

export const ConfigCollectionSection: React.FC<ConfigCollectionSectionProps> = ({
  title,
  description,
  toolbar,
  filters,
  editor,
  className = '',
  children,
}) => {
  const hasEditor = Boolean(editor);

  return (
    <ConfigPageSection
      title={title}
      description={description}
      className={`halo-config-collection-section ${hasEditor ? 'halo-config-collection-section--with-editor' : ''} ${className}`}
    >
      <div className="halo-config-collection-section__content">
        {toolbar && (
          <div className="halo-config-collection-section__toolbar">
            {toolbar}
          </div>
        )}
        {editor && (
          <div className="halo-config-collection-section__editor">
            {editor}
          </div>
        )}
        {filters && (
          <div className="halo-config-collection-section__filters">
            {filters}
          </div>
        )}
        <div className="halo-config-collection-section__list">
          {children}
        </div>
      </div>
    </ConfigPageSection>
  );
};

export default ConfigCollectionSection;
