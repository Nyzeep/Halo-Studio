/**
 * GraphView — Wraps GitGraphView for the Git scene graph tab.
 */

import React from 'react';
import { GitGraphView } from '@/tools/git/components/GitGraphView';
import './GraphView.scss';

interface GraphViewProps {
  workspacePath?: string;
}

const GraphView: React.FC<GraphViewProps> = ({ workspacePath = '' }) => {
  if (!workspacePath) {
    return (
      <div className="halo-git-scene-graph halo-git-scene-graph--empty">
        <p>Open a workspace to see the commit graph.</p>
      </div>
    );
  }

  return (
    <div className="halo-git-scene-graph">
      <GitGraphView repositoryPath={workspacePath} />
    </div>
  );
};

export default GraphView;
