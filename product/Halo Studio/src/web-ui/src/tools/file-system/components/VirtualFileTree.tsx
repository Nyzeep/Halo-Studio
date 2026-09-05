import React, { useCallback, useMemo, useRef, forwardRef } from 'react';
import { useVirtualizer, type Virtualizer } from '@tanstack/react-virtual';
import { VirtualFileTreeProps, FlatFileNode, FileSystemNode } from '../types';
import { useI18n } from '@/infrastructure/i18n';
import { expandedFoldersContains } from '@/shared/utils/pathUtils';
import { FileTreeItem } from './FileTreeItem';

/** Imperative handle exposed to parents (tanstack virtualizer instance). */
export type VirtualFileTreeHandle = Virtualizer<HTMLDivElement, Element>;

interface VirtualFileRowProps {
  node: FlatFileNode;
  isSelected: boolean;
  isExpanded: boolean;
  onSelect: (node: FlatFileNode) => void;
  onToggleExpand: (path: string) => void;
  renamingPath?: string | null;
  onRename?: (oldPath: string, newName: string) => void;
  onCancelRename?: () => void;
  renderContent?: (node: FileSystemNode, level: number) => React.ReactNode;
  renderActions?: (node: FileSystemNode) => React.ReactNode;
}

const FILE_TREE_ROW_ESTIMATED_HEIGHT_PX = 32;

const VirtualFileRow = React.memo<VirtualFileRowProps>(({
  node,
  isSelected,
  isExpanded,
  onSelect,
  onToggleExpand,
  renamingPath,
  onRename,
  onCancelRename,
  renderContent,
  renderActions,
}) => {
  const indentPx = node.depth * 20 + 16;

  const nodeForIcon: FileSystemNode = useMemo(() => ({
    path: node.path,
    name: node.name,
    isDirectory: node.isDirectory,
    extension: node.extension,
    size: node.size,
    lastModified: node.lastModified,
    isCompressed: node.isCompressed,
  }), [node]);

  return (
    <div className="halo-file-explorer__node">
      <FileTreeItem
        node={nodeForIcon}
        level={node.depth}
        indentPx={indentPx}
        isSelected={isSelected}
        isExpanded={isExpanded}
        isLoading={node.isLoading}
        renamingPath={renamingPath}
        onRename={onRename}
        onCancelRename={onCancelRename}
        onSelect={() => onSelect(node)}
        onToggleExpand={() => onToggleExpand(node.path)}
        renderContent={renderContent}
        renderActions={renderActions}
      />
    </div>
  );
});

VirtualFileRow.displayName = 'VirtualFileRow';

export const VirtualFileTree = forwardRef<VirtualFileTreeHandle, VirtualFileTreeProps>(({
  flatNodes,
  selectedFile,
  expandedFolders,
  onNodeSelect,
  onToggleExpand,
  height = '100%',
  className = '',
  renamingPath,
  onRename,
  onCancelRename,
  renderNodeContent,
  renderNodeActions,
}, ref) => {
  const { t } = useI18n('tools');
  const scrollElementRef = useRef<HTMLDivElement | null>(null);

  // Long-list virtualization is standardized on @tanstack/react-virtual
  // (ADR-0077 虚拟化收敛; react-virtuoso was removed in issue #53).
  const virtualizer = useVirtualizer({
    count: flatNodes.length,
    getScrollElement: () => scrollElementRef.current,
    estimateSize: () => FILE_TREE_ROW_ESTIMATED_HEIGHT_PX,
    getItemKey: (index) => flatNodes[index]?.path ?? index,
    overscan: 20,
  });

  React.useImperativeHandle(ref, () => virtualizer, [virtualizer]);

  const handleNodeSelect = useCallback((node: FlatFileNode) => {
    onNodeSelect?.(node);
  }, [onNodeSelect]);

  const handleToggleExpand = useCallback((path: string) => {
    onToggleExpand?.(path);
  }, [onToggleExpand]);

  const renderRow = useCallback((node: FlatFileNode) => {
    const isSelected = selectedFile === node.path;
    const isExpanded = expandedFoldersContains(expandedFolders, node.path);

    return (
      <VirtualFileRow
        node={node}
        isSelected={isSelected}
        isExpanded={isExpanded}
        onSelect={handleNodeSelect}
        onToggleExpand={handleToggleExpand}
        renamingPath={renamingPath}
        onRename={onRename}
        onCancelRename={onCancelRename}
        renderContent={renderNodeContent}
        renderActions={renderNodeActions}
      />
    );
  }, [selectedFile, expandedFolders, handleNodeSelect, handleToggleExpand, renamingPath, onRename, onCancelRename, renderNodeContent, renderNodeActions]);

  if (flatNodes.length === 0) {
    return (
      <div className={`halo-file-explorer__tree halo-file-explorer__tree--empty ${className}`}>
        <div className="halo-file-explorer__empty-message">
          <p>{t('fileTree.empty')}</p>
        </div>
      </div>
    );
  }

  return (
    <div
      className={`halo-file-explorer__tree halo-file-explorer__tree--virtual ${className}`}
      style={{ height }}
      tabIndex={0}
    >
      <div
        ref={scrollElementRef}
        className="halo-file-explorer__tree-scroller"
        style={{ height: '100%', overflowY: 'auto', overflowX: 'hidden' }}
      >
        <div
          style={{
            height: virtualizer.getTotalSize(),
            position: 'relative',
            width: '100%',
          }}
        >
          {virtualizer.getVirtualItems().map((virtualItem) => {
            const node = flatNodes[virtualItem.index];
            if (!node) {
              return null;
            }
            return (
              <div
                key={virtualItem.key}
                ref={virtualizer.measureElement}
                data-index={virtualItem.index}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  transform: `translateY(${virtualItem.start}px)`,
                }}
              >
                {renderRow(node)}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
});

VirtualFileTree.displayName = 'VirtualFileTree';

export default VirtualFileTree;
