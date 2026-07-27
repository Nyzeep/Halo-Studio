import {
  Bot,
  CircleAlert,
  Command,
  Cpu,
  LoaderCircle,
  Plus,
  Send,
  Square,
} from "lucide-react";
import type {
  AgentKind,
  CommandDescriptor,
  SessionMessage,
  SessionSummary,
} from "@halo-studio/contracts";
import type { FormEvent, KeyboardEvent } from "react";

export interface SessionSelection {
  readonly agentKind: AgentKind;
  readonly sessionId: string;
}

export interface SessionWorkbenchProps {
  readonly sessions: readonly SessionSummary[];
  readonly selectedSession: SessionSelection | undefined;
  readonly messages: readonly SessionMessage[];
  readonly commands: readonly CommandDescriptor[];
  readonly draft: string;
  readonly commandFilter: string;
  readonly loading: boolean;
  readonly messagesLoading?: boolean;
  readonly commandsLoading?: boolean;
  readonly sending: boolean;
  readonly aborting: boolean;
  readonly error: string | undefined;
  onSelectSession(session: SessionSummary): void;
  onCreateSession?(agentKind: AgentKind): void;
  canCreateSession?(agentKind: AgentKind): boolean;
  onDraftChange(value: string): void;
  onCommandFilterChange(value: string): void;
  onSend(session: SessionSummary, message: string): void;
  onAbort(session: SessionSummary): void;
}

const agentLabels: Readonly<Record<AgentKind, string>> = {
  pi: "Pi",
  opencode: "OpenCode",
};

const commandSourceLabels: Readonly<Record<CommandDescriptor["source"], string>> = {
  native: "原生",
  extension: "扩展",
  prompt: "提示词",
  skill: "技能",
  tui: "TUI",
};

const roleLabels: Readonly<Record<SessionMessage["role"], string>> = {
  user: "你",
  assistant: "受管应用",
  system: "系统",
  unknown: "原生事件",
};

function sessionTitle(session: SessionSummary): string {
  return session.title?.trim() || `未命名 ${agentLabels[session.agentKind]} 会话`;
}

function matchesCommand(command: CommandDescriptor, query: string): boolean {
  if (query.length === 0) return true;
  const searchable = [command.name, command.description, command.argumentHint, commandSourceLabels[command.source]]
    .filter((value): value is string => value !== undefined)
    .join(" ")
    .toLocaleLowerCase();
  return searchable.includes(query.toLocaleLowerCase());
}

interface SessionListProps {
  readonly sessions: readonly SessionSummary[];
  readonly selectedSession: SessionSelection | undefined;
  readonly loading: boolean;
  onSelectSession(session: SessionSummary): void;
  readonly onCreateSession: ((agentKind: AgentKind) => void) | undefined;
  readonly canCreateSession: ((agentKind: AgentKind) => boolean) | undefined;
}

function SessionList({
  sessions,
  selectedSession,
  loading,
  onSelectSession,
  onCreateSession,
  canCreateSession,
}: SessionListProps): JSX.Element {
  const piSessions = sessions.filter((session) => session.agentKind === "pi");
  const openCodeSessions = sessions.filter((session) => session.agentKind === "opencode");

  const renderSessions = (agentKind: AgentKind, items: readonly SessionSummary[]): JSX.Element => (
    <section className="session-workbench__session-group" aria-label={`${agentLabels[agentKind]} 会话`}>
      <div className="session-workbench__session-group-heading">
        {agentKind === "pi" ? <Bot size={14} aria-hidden="true" /> : <Cpu size={14} aria-hidden="true" />}
        <span>{agentLabels[agentKind]}</span>
        <span className="session-workbench__count">{items.length}</span>
        {onCreateSession === undefined ? null : (
          <button
            className="session-workbench__icon-button"
            type="button"
            aria-label={`新建 ${agentLabels[agentKind]} 会话`}
            title={`新建 ${agentLabels[agentKind]} 会话`}
            disabled={loading || !(canCreateSession?.(agentKind) ?? true)}
            onClick={() => onCreateSession(agentKind)}
          >
            <Plus size={14} aria-hidden="true" />
          </button>
        )}
      </div>
      <ul className="session-workbench__session-list">
        {items.map((session) => {
          const isSelected = selectedSession?.agentKind === session.agentKind
            && selectedSession.sessionId === session.sessionId;
          return (
            <li key={`${session.agentKind}:${session.sessionId}`}>
            <button
              className={`session-workbench__session${isSelected ? " session-workbench__session--selected" : ""}`}
              type="button"
              aria-current={isSelected ? "page" : undefined}
              onClick={() => onSelectSession(session)}
            >
              <span className="session-workbench__session-title">{sessionTitle(session)}</span>
              <span className="session-workbench__session-meta">
                {session.active ? "当前会话" : "历史会话"}
              </span>
            </button>
            </li>
          );
        })}
      </ul>
    </section>
  );

  return (
    <aside className="session-workbench__sessions" aria-label="受管会话列表">
      <div className="session-workbench__pane-heading">
        <span>会话</span>
        {loading ? <LoaderCircle className="spin" size={14} aria-label="正在加载会话" /> : null}
      </div>
      {loading && sessions.length === 0 ? (
        <div className="session-workbench__notice">正在加载受管会话...</div>
      ) : null}
      {!loading && sessions.length === 0 ? (
        <div className="session-workbench__notice">当前工作区尚无受管会话。</div>
      ) : null}
      {renderSessions("pi", piSessions)}
      {renderSessions("opencode", openCodeSessions)}
    </aside>
  );
}

function MessageList({
  selectedSession,
  messages,
  loading,
}: {
  readonly selectedSession: SessionSummary | undefined;
  readonly messages: readonly SessionMessage[];
  readonly loading: boolean;
}): JSX.Element {
  if (selectedSession === undefined) {
    return <div className="session-workbench__empty">选择一个受管会话以查看消息。</div>;
  }

  if (loading) {
    return (
      <div className="session-workbench__empty" role="status">
        <LoaderCircle className="spin" size={18} aria-hidden="true" />
        <span>正在加载结构化消息...</span>
      </div>
    );
  }

  const scopedMessages = messages.filter((message) => (
    message.sessionId === selectedSession.sessionId
    && message.agentKind === selectedSession.agentKind
  ));

  if (scopedMessages.length === 0) {
    return <div className="session-workbench__empty">该会话尚无可显示的结构化消息。</div>;
  }

  return (
    <ol className="session-workbench__messages" aria-label="结构化会话消息">
      {scopedMessages.map((message) => (
        <li
          key={`${message.agentKind}:${message.sessionId}:${message.ordinal}`}
          className={`session-workbench__message session-workbench__message--${message.role}`}
        >
          <span className="session-workbench__message-role">{roleLabels[message.role]}</span>
          <p>{message.text}</p>
        </li>
      ))}
    </ol>
  );
}

function CommandDirectory({
  selectedSession,
  commands,
  commandFilter,
  loading,
  draft,
  onCommandFilterChange,
  onDraftChange,
}: {
  readonly selectedSession: SessionSummary | undefined;
  readonly commands: readonly CommandDescriptor[];
  readonly commandFilter: string;
  readonly loading: boolean;
  readonly draft: string;
  onCommandFilterChange(value: string): void;
  onDraftChange(value: string): void;
}): JSX.Element {
  const draftCommandFilter = draft.trimStart().startsWith("/") ? draft.trimStart() : "";
  const activeFilter = draftCommandFilter || commandFilter;
  const sessionCommands = selectedSession === undefined
    ? []
    : commands.filter((command) => command.agentKind === selectedSession.agentKind);
  const filteredCommands = sessionCommands.filter((command) => matchesCommand(command, activeFilter));

  return (
    <aside className="session-workbench__commands" aria-label="原生命令目录">
      <div className="session-workbench__pane-heading">
        <span>命令目录</span>
        {loading ? <LoaderCircle className="spin" size={14} aria-label="正在加载命令目录" /> : null}
      </div>
      <label className="session-workbench__command-filter">
        <span className="sr-only">筛选原生命令</span>
        <Command size={14} aria-hidden="true" />
        <input
          type="search"
          value={commandFilter}
          placeholder="筛选命令"
          aria-label="筛选原生命令"
          onChange={(event) => onCommandFilterChange(event.target.value)}
        />
      </label>
      {selectedSession === undefined ? (
        <div className="session-workbench__notice">选择会话后显示其原生命令。</div>
      ) : null}
      {selectedSession !== undefined && loading ? (
        <div className="session-workbench__notice">正在加载 {agentLabels[selectedSession.agentKind]} 命令...</div>
      ) : null}
      {selectedSession !== undefined && !loading && filteredCommands.length === 0 ? (
        <div className="session-workbench__notice">没有匹配的原生命令。</div>
      ) : null}
      {selectedSession !== undefined && !loading ? (
        <ul className="session-workbench__command-list">
          {filteredCommands.map((command) => (
            <li key={`${command.agentKind}:${command.source}:${command.name}`}>
              <button
                className="session-workbench__command"
                type="button"
                aria-label={`插入命令 ${command.name}`}
                title={`插入 ${command.name} 到消息输入框`}
                onClick={() => onDraftChange(`${command.name}${command.argumentHint === undefined ? "" : " "}`)}
              >
                <span className="session-workbench__command-name">{command.name}</span>
                <span className="session-workbench__command-source">{commandSourceLabels[command.source]}</span>
                {command.tuiOnly ? <span className="session-workbench__command-tui">TUI</span> : null}
                {command.description === undefined ? null : <span className="session-workbench__command-description">{command.description}</span>}
                {command.argumentHint === undefined ? null : <span className="session-workbench__command-hint">{command.argumentHint}</span>}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </aside>
  );
}

export function SessionWorkbench({
  sessions,
  selectedSession,
  messages,
  commands,
  draft,
  commandFilter,
  loading,
  messagesLoading = false,
  commandsLoading = false,
  sending,
  aborting,
  error,
  onSelectSession,
  onCreateSession,
  canCreateSession,
  onDraftChange,
  onCommandFilterChange,
  onSend,
  onAbort,
}: SessionWorkbenchProps): JSX.Element {
  const activeSession = selectedSession === undefined
    ? undefined
    : sessions.find((session) => (
      session.agentKind === selectedSession.agentKind
      && session.sessionId === selectedSession.sessionId
    ));
  const canSend = activeSession !== undefined && draft.trim().length > 0 && !loading && !sending;
  const canAbort = activeSession !== undefined && activeSession.active && !loading && !aborting;

  const sendDraft = (): void => {
    if (!canSend || activeSession === undefined) return;
    onSend(activeSession, draft);
  };
  const handleSubmit = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    sendDraft();
  };
  const handleComposerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>): void => {
    if (event.key !== "Enter" || (!event.metaKey && !event.ctrlKey)) return;
    event.preventDefault();
    sendDraft();
  };

  return (
    <section className="session-workbench" aria-label="受管会话工作台" aria-busy={loading}>
      <SessionList
        sessions={sessions}
        selectedSession={selectedSession}
        loading={loading}
        onSelectSession={onSelectSession}
        onCreateSession={onCreateSession}
        canCreateSession={canCreateSession}
      />
      <div className="session-workbench__conversation">
        <div className="session-workbench__conversation-heading">
          <div>
            <span className="session-workbench__eyebrow">结构化会话</span>
            <strong>{activeSession === undefined ? "未选择会话" : sessionTitle(activeSession)}</strong>
          </div>
          {activeSession === undefined ? null : (
            <span className="session-workbench__agent-badge">
              {activeSession.agentKind === "pi" ? <Bot size={13} aria-hidden="true" /> : <Cpu size={13} aria-hidden="true" />}
              {agentLabels[activeSession.agentKind]}
            </span>
          )}
        </div>
        {error === undefined ? null : (
          <div className="session-workbench__error" role="alert">
            <CircleAlert size={15} aria-hidden="true" />
            <span>{error}</span>
          </div>
        )}
        <div className="session-workbench__message-scroll">
          <MessageList selectedSession={activeSession} messages={messages} loading={messagesLoading} />
        </div>
        <form className="session-workbench__composer" onSubmit={handleSubmit}>
          <label className="sr-only" htmlFor="session-workbench-message">会话消息</label>
          <textarea
            id="session-workbench-message"
            value={draft}
            disabled={activeSession === undefined || loading || sending}
            placeholder={activeSession === undefined ? "先选择一个受管会话" : "发送消息或输入 / 查看可用原生命令"}
            aria-label="会话消息"
            onChange={(event) => onDraftChange(event.target.value)}
            onKeyDown={handleComposerKeyDown}
          />
          <div className="session-workbench__composer-actions">
            <button
              className="session-workbench__abort"
              type="button"
              aria-label="中止当前会话"
              title="中止当前会话"
              disabled={!canAbort}
              onClick={() => {
                if (activeSession !== undefined) onAbort(activeSession);
              }}
            >
              {aborting ? <LoaderCircle className="spin" size={15} aria-hidden="true" /> : <Square size={14} aria-hidden="true" />}
            </button>
            <button
              className="session-workbench__send"
              type="submit"
              aria-label="发送消息"
              title="发送消息"
              disabled={!canSend}
            >
              {sending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <Send size={16} aria-hidden="true" />}
            </button>
          </div>
        </form>
      </div>
      <CommandDirectory
        selectedSession={activeSession}
        commands={commands}
        commandFilter={commandFilter}
        loading={commandsLoading}
        draft={draft}
        onCommandFilterChange={onCommandFilterChange}
        onDraftChange={onDraftChange}
      />
    </section>
  );
}
