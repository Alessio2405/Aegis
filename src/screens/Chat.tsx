import React, { useState, useRef, useEffect } from 'react';
import { useAppStore } from '../store';
import { ChatTurn, UserAgent } from '../types';
import * as api from '../api/tauri';

interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  created_at?: string;
  loading?: boolean;
  error?: string;
  /** Set when the message is the result of a /agent command */
  agentName?: string;
  agentSteps?: number;
}

const QUICK_PROMPTS = [
  { label: 'What\'s in my workspace?', text: 'Give me an overview of what\'s in my workspace.' },
  { label: 'Summarize recent notes',   text: 'Summarize the most important notes and decisions from my workspace.' },
  { label: 'Write a plan',             text: 'Help me write a structured plan for a project. Ask me what it\'s about.' },
  { label: 'List my agents',           text: '/agent' },
];

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  } catch {
    return '';
  }
}

export default function Chat() {
  const { settings, addToast } = useAppStore();
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [historyLoaded, setHistoryLoaded] = useState(false);
  const [agents, setAgents] = useState<UserAgent[]>([]);
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    api.listAgents().then(setAgents).catch(() => {});
  }, []);

  // Load persisted chat history on mount
  useEffect(() => {
    api.listChatHistory(50).then((rows) => {
      setMessages(rows.map((r) => ({
        id: r.id,
        role: r.role,
        content: r.content,
        created_at: r.created_at,
      })));
      setHistoryLoaded(true);
    }).catch(() => setHistoryLoaded(true));
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Parse "/agent Name: task" or "/agent Name\ntask"
  const parseAgentCommand = (text: string): { agentRef: string; task: string } | null => {
    if (!text.startsWith('/agent')) return null;
    const rest = text.slice('/agent'.length).trim();
    if (!rest) return null;
    // Format 1: "Name: task description"
    const colonIdx = rest.indexOf(':');
    if (colonIdx > 0) {
      return { agentRef: rest.slice(0, colonIdx).trim(), task: rest.slice(colonIdx + 1).trim() };
    }
    // Format 2: "Name\ntask description"
    const newlineIdx = rest.indexOf('\n');
    if (newlineIdx > 0) {
      return { agentRef: rest.slice(0, newlineIdx).trim(), task: rest.slice(newlineIdx + 1).trim() };
    }
    return null;
  };

  const handleSend = async (e: React.FormEvent) => {
    e.preventDefault();
    const text = input.trim();
    if (!text || isLoading) return;

    // Special: /agent listing (just "/agent" with no args)
    if (text === '/agent' || text === '/agent ') {
      const list = agents.length
        ? agents.map((a) => `• **${a.name}** — ${a.description || 'no description'}`).join('\n')
        : 'No agents created yet. Open the Agents window to create one.';
      setMessages((prev) => [
        ...prev,
        { id: Math.random().toString(36).slice(2), role: 'user', content: text },
        { id: Math.random().toString(36).slice(2), role: 'assistant', content: `**Available agents:**\n\n${list}\n\nUse \`/agent Name: task\` to run one.` },
      ]);
      setInput('');
      inputRef.current?.focus();
      return;
    }

    const agentCmd = parseAgentCommand(text);
    const assistantMsgId = Math.random().toString(36).slice(2);

    setMessages((prev) => [
      ...prev,
      { id: Math.random().toString(36).slice(2), role: 'user', content: text },
      { id: assistantMsgId, role: 'assistant', content: '', loading: true },
    ]);
    setInput('');
    setIsLoading(true);

    try {
      if (agentCmd) {
        // Run agent
        const { agentRef, task } = agentCmd;
        const found = agents.find(
          (a) => a.name.toLowerCase().includes(agentRef.toLowerCase()) || a.id === agentRef
        );
        if (!found) {
          const names = agents.map((a) => a.name).join(', ') || 'none';
          throw new Error(`No agent matching "${agentRef}". Available: ${names}`);
        }
        const run = await api.runAgent(found.id, task || '(run)');
        const content = run.output ?? '(no output)';
        setMessages((prev) =>
          prev.map((m) =>
            m.id === assistantMsgId
              ? { ...m, content, loading: false, agentName: found.name, agentSteps: run.steps.length }
              : m
          )
        );
        // Refresh agents list in case a new one was created
        api.listAgents().then(setAgents).catch(() => {});
      } else {
        // Streaming chat — tokens arrive via event listener
        const history: ChatTurn[] = messages
          .filter((m) => !m.error && !m.loading)
          .map((m) => ({ role: m.role, content: m.content }));

        const streamId = Math.random().toString(36).slice(2);
        const unlisten = await api.listenChatToken((evt) => {
          if (evt.id !== streamId) return;
          setMessages((prev) =>
            prev.map((m) =>
              m.id === assistantMsgId
                ? { ...m, content: m.content + evt.token, loading: false }
                : m
            )
          );
        });
        try {
          await api.chatMessageStream(streamId, text, history);
        } finally {
          unlisten();
        }
        // Ensure loading cleared even if zero tokens (empty response)
        setMessages((prev) =>
          prev.map((m) => m.id === assistantMsgId && m.loading ? { ...m, loading: false } : m)
        );
      }
    } catch (err) {
      const errMsg = String(err);
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantMsgId
            ? { ...m, content: '', loading: false, error: errMsg }
            : m
        )
      );
      if (errMsg.includes('disabled') || errMsg.includes('model.use')) {
        addToast('Enable a model provider in Settings to use the chat', 'error');
      }
    } finally {
      setIsLoading(false);
      inputRef.current?.focus();
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend(e as unknown as React.FormEvent);
    }
  };

  const handleClearHistory = async () => {
    try {
      await api.clearChatHistory();
      setMessages([]);
    } catch {
      addToast('Failed to clear history', 'error');
    }
    inputRef.current?.focus();
  };

  const modelLabel = settings
    ? settings.model_provider === 'openai'
      ? `${settings.openai_model} (OpenAI)`
      : `${settings.ollama_model} via ${settings.ollama_url}`
    : 'LLM';

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{
        padding: '12px 20px',
        borderBottom: '1px solid var(--border)',
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        background: 'var(--bg-surface)',
      }}>
        <h1 style={{ fontSize: 15, fontWeight: 700, flex: 1 }}>Chat <span style={{ fontSize: 11, fontWeight: 400, color: 'var(--text-faint)', marginLeft: 4 }}>· /agent Name: task to run an agent</span></h1>
        {settings && !settings.model_enabled && (
          <span className="tag tag-yellow" style={{ fontSize: 11 }}>
            Model disabled — enable in Settings
          </span>
        )}
        {settings?.model_enabled && (
          <span className="tag" style={{ fontSize: 11, color: 'var(--text-muted)' }}>
            {modelLabel}
          </span>
        )}
        {messages.length > 0 && (
          <button
            onClick={handleClearHistory}
            className="btn btn-ghost"
            style={{ fontSize: 12 }}
          >
            Clear History
          </button>
        )}
      </div>

      {/* Messages */}
      <div style={{
        flex: 1,
        overflow: 'auto',
        padding: '20px',
        display: 'flex',
        flexDirection: 'column',
        gap: 16,
      }}>
        {historyLoaded && messages.length === 0 ? (
          <div style={{ margin: 'auto', textAlign: 'center', maxWidth: 420, padding: '0 20px' }}>
            <div style={{ fontSize: 40, marginBottom: 12 }}>🛡</div>
            <div style={{ fontSize: 17, fontWeight: 700, color: 'var(--text)', marginBottom: 6 }}>Aegis</div>
            <div style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 28, lineHeight: 1.6 }}>
              Chat with your AI, search your knowledge base, or run an agent.<br />
              Type <code style={{ fontSize: 12, background: 'var(--bg-elevated)', padding: '1px 5px', borderRadius: 4 }}>/agent Name: task</code> to invoke any agent.
            </div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, justifyContent: 'center' }}>
              {QUICK_PROMPTS.map((p) => (
                <button
                  key={p.label}
                  className="btn btn-ghost"
                  style={{ fontSize: 12, padding: '6px 12px', borderRadius: 20, border: '1px solid var(--border)' }}
                  onClick={() => { setInput(p.text); inputRef.current?.focus(); }}
                >
                  {p.label}
                </button>
              ))}
            </div>
          </div>
        ) : (
          messages.map((msg) => <ChatBubble key={msg.id} msg={msg} />)
        )}
        <div ref={bottomRef} />
      </div>

      {/* Input */}
      <div style={{
        padding: '12px 20px',
        borderTop: '1px solid var(--border)',
        background: 'var(--bg-surface)',
      }}>
        <form onSubmit={handleSend} style={{ display: 'flex', gap: 10, alignItems: 'flex-end' }}>
          <textarea
            ref={inputRef}
            className="form-input"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Message… or /agent Name: task (Enter to send, Shift+Enter for newline)"
            rows={2}
            disabled={isLoading}
            style={{ flex: 1, fontSize: 13, resize: 'none', lineHeight: 1.5 }}
            autoFocus
          />
          <button
            type="submit"
            className="btn btn-primary"
            disabled={!input.trim() || isLoading}
            style={{ alignSelf: 'flex-end' }}
          >
            {isLoading ? '…' : 'Send ↵'}
          </button>
        </form>
        <div style={{ fontSize: 11, color: 'var(--text-faint)', marginTop: 6 }}>
          History stored locally · <code style={{ fontSize: 10 }}>/agent Name: task</code> to run an agent inline
        </div>
      </div>
    </div>
  );
}

function ChatBubble({ msg }: { msg: Message }) {
  const isUser = msg.role === 'user';
  const ts = msg.created_at ? formatTime(msg.created_at) : '';

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      alignItems: isUser ? 'flex-end' : 'flex-start',
    }}>
      <div style={{
        maxWidth: '80%',
        borderRadius: isUser ? '12px 12px 4px 12px' : '12px 12px 12px 4px',
        padding: '10px 14px',
        fontSize: 13,
        lineHeight: 1.7,
        whiteSpace: 'pre-wrap',
        wordBreak: 'break-word',
        ...(isUser
          ? { background: 'var(--accent)', color: '#fff' }
          : msg.error
          ? { background: 'var(--bg-elevated)', border: '1px solid var(--danger)', color: 'var(--danger)' }
          : { background: 'var(--bg-elevated)', color: 'var(--text)', border: '1px solid var(--border)' }
        ),
      }}>
        {msg.loading ? (
          <span style={{ display: 'flex', alignItems: 'center', gap: 8, color: 'var(--text-muted)' }}>
            <span className="spinner" /> Thinking…
          </span>
        ) : msg.error ? (
          <>Error: {msg.error}</>
        ) : (
          msg.content
        )}
      </div>
      <div style={{ fontSize: 10, color: 'var(--text-faint)', marginTop: 3, paddingInline: 4, display: 'flex', gap: 8, alignItems: 'center' }}>
        <span>{isUser ? 'You' : msg.agentName ? `🤖 ${msg.agentName}` : 'Assistant'}{ts ? ` · ${ts}` : ''}</span>
        {msg.agentSteps != null && (
          <span className="tag" style={{ fontSize: 9, padding: '1px 5px' }}>{msg.agentSteps} steps</span>
        )}
      </div>
    </div>
  );
}
