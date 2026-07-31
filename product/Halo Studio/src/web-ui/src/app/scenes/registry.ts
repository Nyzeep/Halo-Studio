/**
 * SCENE_TAB_REGISTRY - Halo local-coding scene definitions.
 *
 * The full BitFun source tree remains present for upstream sync and audit, but
 * the Halo product entry only assembles the first-release local coding path.
 */

import {
  MessageSquare,
  Terminal,
  GitBranch,
  Settings,
  FileCode2,
} from 'lucide-react';
import type { SceneTabDef, SceneTabId } from '../components/SceneBar/types';

/** Upper bound for concurrent open scene tabs (top bar); oldest closable tab is evicted when exceeded. */
export const MAX_OPEN_SCENES = 3;

export const SCENE_TAB_REGISTRY: SceneTabDef[] = [
  {
    id: 'welcome' as SceneTabId,
    label: 'Welcome',
    labelKey: 'welcomeScene.tabLabel',
    pinned: false,
    singleton: true,
    defaultOpen: true,
  },
  {
    id: 'session' as SceneTabId,
    label: 'Session',
    labelKey: 'scenes.aiAgent',
    Icon: MessageSquare,
    pinned: true,
    fixed: true,
    closable: false,
    singleton: true,
    defaultOpen: false,
  },
  {
    id: 'file-viewer' as SceneTabId,
    label: 'File Viewer',
    Icon: FileCode2,
    pinned: false,
    singleton: true,
    defaultOpen: false,
  },
  {
    id: 'git' as SceneTabId,
    label: 'Git',
    Icon: GitBranch,
    pinned: false,
    singleton: true,
    defaultOpen: false,
  },
  {
    id: 'terminal' as SceneTabId,
    label: 'Terminal',
    Icon: Terminal,
    pinned: false,
    singleton: true,
    defaultOpen: false,
  },
  {
    id: 'shell' as SceneTabId,
    label: 'Shell',
    labelKey: 'scenes.shell',
    Icon: Terminal,
    pinned: false,
    singleton: true,
    defaultOpen: false,
  },
  {
    id: 'settings' as SceneTabId,
    label: 'Settings',
    labelKey: 'shared:features.settings',
    Icon: Settings,
    pinned: false,
    singleton: true,
    defaultOpen: false,
  },
];

export function getSceneDef(id: SceneTabId): SceneTabDef | undefined {
  return SCENE_TAB_REGISTRY.find(d => d.id === id);
}
