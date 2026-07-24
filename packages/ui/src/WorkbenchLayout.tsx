import type { ReactNode } from "react";

export interface WorkbenchLayoutProps {
  readonly titleBar: ReactNode;
  readonly activityBar: ReactNode;
  readonly sideBar: ReactNode;
  readonly editor: ReactNode;
  readonly auxiliaryBar: ReactNode;
  readonly bottomPanel: ReactNode;
  readonly statusBar: ReactNode;
}

export function WorkbenchLayout({
  titleBar,
  activityBar,
  sideBar,
  editor,
  auxiliaryBar,
  bottomPanel,
  statusBar,
}: WorkbenchLayoutProps): JSX.Element {
  return (
    <div className="halo-workbench">
      <header className="halo-workbench__titlebar" aria-label="标题栏">{titleBar}</header>
      <nav className="halo-workbench__activitybar" aria-label="主活动栏">{activityBar}</nav>
      <aside className="halo-workbench__sidebar" aria-label="侧边栏">{sideBar}</aside>
      <main className="halo-workbench__editor" aria-label="编辑器区域">{editor}</main>
      <aside className="halo-workbench__auxiliary" aria-label="Agent 面板">{auxiliaryBar}</aside>
      <section className="halo-workbench__bottom" aria-label="底部面板">{bottomPanel}</section>
      <footer className="halo-workbench__status">{statusBar}</footer>
    </div>
  );
}
