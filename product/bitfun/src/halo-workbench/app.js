const files = [
  { name: 'src', kind: 'folder', depth: 0 },
  { name: 'main.rs', kind: 'file', depth: 1 },
  { name: 'workspace.rs', kind: 'file', depth: 1 },
  { name: 'Cargo.toml', kind: 'file', depth: 0 },
  { name: 'README.md', kind: 'file', depth: 0 },
];

const fileContents = {
  'main.rs': 'fn main() {\n    println!("Halo Studio ready");\n}\n',
  'workspace.rs': 'pub struct Workspace {\n    pub root: String,\n}\n',
  'Cargo.toml': '[package]\nname = "halo-workbench"\nversion = "0.1.0"\n',
  'README.md': '# Halo Studio\n\n本地编码工作台。',
};

const navItems = [
  { id: 'workspace', label: '工作区', icon: '□' },
  { id: 'code', label: '编码会话', icon: '</>' },
  { id: 'git', label: '版本控制', icon: '⌘' },
  { id: 'terminal', label: '终端', icon: '>_' },
];

const state = {
  activeNav: 'workspace',
  activeFile: 'main.rs',
  running: false,
  workspaceOpen: false,
  commandText: '',
  lastCommand: '',
};

function fileRow(file) {
  const indent = file.depth * 16;
  const icon = file.kind === 'folder' ? '▾' : '•';
  const selected = file.name === state.activeFile ? ' is-selected' : '';
  return `<button class="file-row${selected}" type="button" data-file="${file.name}" style="--indent:${indent}px">
    <span class="file-row__icon" aria-hidden="true">${icon}</span><span>${file.name}</span>
  </button>`;
}

function render() {
  const app = document.querySelector('#app');
  app.innerHTML = `
    <div class="shell">
      <header class="topbar">
        <div class="brand">
          <img src="/halo-icon.svg" alt="" class="brand__icon" />
          <div><strong>Halo Studio</strong><span>本地编码工作台</span></div>
        </div>
        <div class="topbar__workspace"><span class="status-dot"></span>${state.workspaceOpen ? 'halo-workspace' : '未打开工作区'}</div>
        <div class="topbar__actions">
          <button class="icon-button" type="button" title="打开工作区" data-action="open-workspace">＋</button>
          <button class="avatar" type="button" title="本地开发者">H</button>
        </div>
      </header>
      <div class="workbench">
        <aside class="sidebar sidebar--left">
          <div class="sidebar__section-label">导航</div>
          <nav class="nav-list" aria-label="主导航">
            ${navItems.map(item => `<button class="nav-item${item.id === state.activeNav ? ' is-active' : ''}" type="button" data-nav="${item.id}"><span class="nav-item__icon">${item.icon}</span><span>${item.label}</span></button>`).join('')}
          </nav>
          <div class="sidebar__divider"></div>
          <div class="sidebar__section-label sidebar__section-label--row"><span>工作区文件</span><button class="mini-button" type="button" title="新建文件" data-action="new-file">＋</button></div>
          <div class="file-tree">${files.map(fileRow).join('')}</div>
          <div class="sidebar__footer"><span class="status-dot status-dot--teal"></span><span>本地模式</span><span class="footer__version">v0.1</span></div>
        </aside>
        <main class="main-panel">
          <section class="hero-strip">
            <div><p class="eyebrow">HALO WORKBENCH</p><h1>把注意力留在代码上</h1><p class="hero-strip__copy">从本地工作区开始，编辑、检查并运行你的项目。</p></div>
            <button class="primary-button" type="button" data-action="open-workspace">${state.workspaceOpen ? '切换工作区' : '打开工作区'} <span aria-hidden="true">→</span></button>
          </section>
          <div class="editor-tabs"><button class="editor-tab is-active" type="button"><span class="tab-dot"></span>${state.activeFile}<span class="tab-close">×</span></button><span class="editor-tabs__hint">本地编辑器</span></div>
          <section class="editor" aria-label="代码编辑区">
            <div class="editor__gutter">${Array.from({length: 9}, (_, i) => `<span>${i + 1}</span>`).join('')}</div>
            <pre class="code"><code>${escapeHtml(fileContents[state.activeFile] || '// 选择一个文件开始编辑')}</code></pre>
          </section>
          <section class="command-bar">
            <div class="command-bar__label"><span class="command-bar__mark">⌁</span><span>工作区命令</span></div>
            <input id="command-input" type="text" value="${escapeHtml(state.commandText)}" placeholder="输入要执行的本地命令…" aria-label="工作区命令" />
            <button class="run-button" type="button" data-action="run-command"><span aria-hidden="true">▶</span>${state.running ? '再次运行' : '运行'}</button>
          </section>
          <section class="terminal-panel"><div class="terminal-panel__top"><span>终端</span><span class="terminal-panel__path">${state.workspaceOpen ? '~/halo-workspace' : '~/workspace'}</span><span class="terminal-panel__state">${state.running ? '已记录' : '就绪'}</span></div><pre id="terminal-output">${state.running ? `$ ${escapeHtml(state.lastCommand)}\n本地命令已加入工作台预览。` : '$ 等待本地命令\n选择文件或运行一条命令开始工作。'}</pre></section>
        </main>
        <aside class="sidebar sidebar--right">
          <div class="panel-heading"><span>工作区状态</span><button class="mini-button" type="button" title="刷新状态" data-action="refresh">↻</button></div>
          <div class="status-card"><div class="status-card__title"><span class="status-dot status-dot--teal"></span>本地工作区</div><strong>${state.workspaceOpen ? '已打开' : '等待打开'}</strong><p>${state.workspaceOpen ? '本地文件、终端和版本控制面板已准备。' : '打开一个项目目录以开始编码。'}</p></div>
          <div class="panel-heading panel-heading--spaced"><span>最近活动</span><span class="muted">${state.running ? '刚刚' : '今天'}</span></div>
          <div class="activity-list"><div class="activity-item"><span class="activity-item__icon">✓</span><div><strong>工作台已启动</strong><span>Halo 本地工作台已载入</span></div><time>09:41</time></div><div class="activity-item"><span class="activity-item__icon">⌁</span><div><strong>编码会话</strong><span>保持本地上下文</span></div><time>09:40</time></div></div>
          <div class="tips"><span class="tips__mark">i</span><div><strong>快速开始</strong><p>打开工作区后，从左侧文件树选择文件。</p></div></div>
        </aside>
      </div>
    </div>`;
  bindEvents();
}

function escapeHtml(value) {
  return value.replace(/[&<>"']/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[char]));
}

function bindEvents() {
  document.querySelectorAll('[data-nav]').forEach(button => button.addEventListener('click', () => {
    state.activeNav = button.dataset.nav;
    render();
  }));
  document.querySelectorAll('[data-file]').forEach(button => button.addEventListener('click', () => {
    if (button.dataset.file === 'src') return;
    state.activeFile = button.dataset.file;
    render();
  }));
  document.querySelectorAll('[data-action="open-workspace"]').forEach(button => button.addEventListener('click', () => {
    state.workspaceOpen = true;
    render();
  }));
  const commandInput = document.querySelector('#command-input');
  const recordCommand = () => {
    const command = commandInput?.value.trim() || state.lastCommand;
    if (!command) return;
    state.lastCommand = command;
    state.commandText = '';
    state.running = true;
    render();
  };
  commandInput?.addEventListener('input', event => {
    state.commandText = event.currentTarget.value;
  });
  document.querySelectorAll('[data-action="run-command"]').forEach(button => button.addEventListener('click', recordCommand));
  commandInput?.addEventListener('keydown', event => {
    if (event.key === 'Enter') {
      event.preventDefault();
      recordCommand();
    }
  });
  document.querySelectorAll('[data-action="new-file"]').forEach(button => button.addEventListener('click', () => {
    state.activeFile = 'README.md';
    render();
  }));
  document.querySelectorAll('[data-action="refresh"]').forEach(button => button.addEventListener('click', () => render()));
}

render();
