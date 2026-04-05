import React, { useEffect, useState } from 'react';
import { useAppStore } from '../store';
import { AppSettings, McpServerConfig } from '../types';
import { startTelegramBot, stopTelegramBot, getTelegramStatus,
         startDiscordBot, stopDiscordBot, getDiscordStatus,
         startWhatsAppBot, stopWhatsAppBot, getWhatsAppStatus,
         listMcpServers, addMcpServer, removeMcpServer, refreshMcpTools,
         openFolderDialog } from '../api/tauri';

export default function Settings() {
  const { settings, refreshSettings, saveSettings, addToast } = useAppStore();
  const [form, setForm] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(false);
  const [saved, setSaved] = useState(false);
  const [botRunning, setBotRunning] = useState(false);
  const [botLoading, setBotLoading] = useState(false);
  const [discordRunning, setDiscordRunning] = useState(false);
  const [discordLoading, setDiscordLoading] = useState(false);
  const [whatsappRunning, setWhatsappRunning] = useState(false);
  const [whatsappLoading, setWhatsappLoading] = useState(false);

  // MCP state
  const [mcpServers, setMcpServers] = useState<McpServerConfig[]>([]);
  const [mcpLabel, setMcpLabel] = useState('');
  const [mcpCommand, setMcpCommand] = useState('');
  const [mcpArgs, setMcpArgs] = useState('');
  const [mcpAdding, setMcpAdding] = useState(false);
  const [mcpError, setMcpError] = useState('');

  useEffect(() => {
    refreshSettings();
    getTelegramStatus().then(setBotRunning).catch(() => {});
    getDiscordStatus().then(setDiscordRunning).catch(() => {});
    getWhatsAppStatus().then(setWhatsappRunning).catch(() => {});
    listMcpServers().then(setMcpServers).catch(() => {});
  }, []);

  useEffect(() => {
    if (settings && !form) setForm({ ...settings });
  }, [settings]);

  if (!form) return <div style={{ padding: 24 }}><div className="spinner" /></div>;

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    try {
      await saveSettings(form);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      addToast(String(e), 'error');
    } finally {
      setLoading(false);
    }
  };

  const handleBotToggle = async () => {
    setBotLoading(true);
    try {
      if (botRunning) {
        await stopTelegramBot();
        setBotRunning(false);
        addToast('Telegram bot stopped.', 'info');
      } else {
        await startTelegramBot();
        setBotRunning(true);
        addToast('Telegram bot started! Send /help to your bot.', 'success');
      }
    } catch (e) {
      addToast(String(e), 'error');
    } finally {
      setBotLoading(false);
    }
  };

  const handleDiscordToggle = async () => {
    setDiscordLoading(true);
    try {
      if (discordRunning) {
        await stopDiscordBot();
        setDiscordRunning(false);
        addToast('Discord bot stopped.', 'info');
      } else {
        await startDiscordBot();
        setDiscordRunning(true);
        addToast('Discord bot started! Send /help in a DM to your bot.', 'success');
      }
    } catch (e) {
      addToast(String(e), 'error');
    } finally {
      setDiscordLoading(false);
    }
  };

  const handleWhatsappToggle = async () => {
    setWhatsappLoading(true);
    try {
      if (whatsappRunning) {
        await stopWhatsAppBot();
        setWhatsappRunning(false);
        addToast('WhatsApp webhook server stopped.', 'info');
      } else {
        await startWhatsAppBot();
        setWhatsappRunning(true);
        addToast('WhatsApp webhook server started! Send /help to your allowed number.', 'success');
      }
    } catch (e) {
      addToast(String(e), 'error');
    } finally {
      setWhatsappLoading(false);
    }
  };

  const Toggle = ({ value, onChange, disabled }: {
    value: boolean; onChange: (v: boolean) => void; disabled?: boolean;
  }) => (
    <label style={{ position: 'relative', display: 'inline-block', width: 44, height: 24, flexShrink: 0 }}>
      <input
        type="checkbox"
        checked={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
        style={{ opacity: 0, width: 0, height: 0 }}
      />
      <span style={{
        position: 'absolute', cursor: disabled ? 'not-allowed' : 'pointer', inset: 0,
        background: value && !disabled ? 'var(--accent)' : 'var(--bg-elevated)',
        borderRadius: 12, border: '1px solid var(--border)',
        opacity: disabled ? 0.4 : 1, transition: 'background 0.2s',
      }}>
        <span style={{
          position: 'absolute', top: 3, left: (value && !disabled) ? 22 : 3,
          width: 16, height: 16, background: '#fff', borderRadius: '50%',
          transition: 'left 0.2s', boxShadow: '0 1px 3px rgba(0,0,0,0.3)',
        }} />
      </span>
    </label>
  );

  return (
    <div style={{ height: '100%', overflow: 'auto', padding: 24 }}>
      <div style={{ maxWidth: 600 }}>
        <h1 style={{ fontSize: 18, fontWeight: 700, marginBottom: 4 }}>⚙ Settings</h1>
        <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 24 }}>
          Configure AI model, Telegram bot, and desktop automation.
        </p>

        <form onSubmit={handleSave} style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>

          {/* ── AI Model ── */}
          <div className="card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10 }}>
              <div>
                <div style={{ fontWeight: 600, fontSize: 14 }}>🤖 AI Model</div>
                <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
                  Enable to use an AI model for Q&A synthesis and agent runs.
                </div>
              </div>
              <Toggle value={form.model_enabled} onChange={(v) => setForm({ ...form, model_enabled: v })} />
            </div>

            {form.model_enabled && (
              <>
                {/* Provider selector */}
                <div style={{ display: 'flex', gap: 6, marginBottom: 10 }}>
                  {(['ollama', 'openai', 'anthropic'] as const).map((p) => (
                    <button
                      key={p}
                      type="button"
                      className={`btn ${form.model_provider === p ? 'btn-primary' : 'btn-ghost'}`}
                      style={{ fontSize: 12, flex: 1 }}
                      onClick={() => setForm({ ...form, model_provider: p })}
                    >
                      {p === 'ollama' ? '🦙 Ollama' : p === 'openai' ? '✦ OpenAI' : '◆ Anthropic'}
                    </button>
                  ))}
                </div>

                {form.model_provider === 'ollama' && (
                  <div style={{
                    padding: '10px 12px', background: 'rgba(251,191,36,0.08)',
                    border: '1px solid rgba(251,191,36,0.3)', borderRadius: 'var(--radius-sm)',
                    fontSize: 12, color: 'var(--warning)',
                  }}>
                    ⚠ Ollama mode sends document text to the local Ollama endpoint. Ensure Ollama is running.
                  </div>
                )}
                {form.model_provider !== 'ollama' && (
                  <div style={{
                    padding: '10px 12px', background: 'rgba(251,191,36,0.08)',
                    border: '1px solid rgba(251,191,36,0.3)', borderRadius: 'var(--radius-sm)',
                    fontSize: 12, color: 'var(--warning)',
                  }}>
                    ⚠ Cloud mode sends document excerpts to {form.model_provider === 'openai' ? 'OpenAI' : 'Anthropic'}. Ensure you're comfortable with your data leaving the device.
                  </div>
                )}
              </>
            )}
          </div>

          {/* ── Ollama config ── */}
          {form.model_enabled && form.model_provider === 'ollama' && (
            <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 4 }}>🦙 Ollama Configuration</div>
              <div className="form-group">
                <label className="form-label">Ollama URL</label>
                <input
                  className="form-input"
                  value={form.ollama_url}
                  onChange={(e) => setForm({ ...form, ollama_url: e.target.value })}
                  placeholder="http://localhost:11434"
                />
              </div>
              <div className="form-group">
                <label className="form-label">Chat Model</label>
                <input
                  className="form-input"
                  value={form.ollama_model}
                  onChange={(e) => setForm({ ...form, ollama_model: e.target.value })}
                  placeholder="llama3"
                />
                <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                  Run: <code>ollama pull llama3</code> to download.
                </div>
              </div>
            </div>
          )}

          {/* ── OpenAI config ── */}
          {form.model_enabled && form.model_provider === 'openai' && (
            <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 4 }}>✦ OpenAI Configuration</div>
              <div className="form-group">
                <label className="form-label">API Key</label>
                <input
                  className="form-input"
                  type="password"
                  value={form.openai_api_key}
                  onChange={(e) => setForm({ ...form, openai_api_key: e.target.value })}
                  placeholder="sk-…"
                />
                <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                  Stored locally in the vault DB. Get yours at platform.openai.com.
                </div>
              </div>
              <div className="form-group">
                <label className="form-label">Model</label>
                <select
                  className="form-input"
                  value={form.openai_model}
                  onChange={(e) => setForm({ ...form, openai_model: e.target.value })}
                >
                  <optgroup label="GPT-5 (Latest)">
                    <option value="gpt-5.4">gpt-5.4</option>
                    <option value="gpt-5.3">gpt-5.3</option>
                  </optgroup>
                  <optgroup label="GPT-4.1">
                    <option value="gpt-4.1">gpt-4.1 (best quality)</option>
                    <option value="gpt-4.1-mini">gpt-4.1-mini (fast, cheap)</option>
                    <option value="gpt-4.1-nano">gpt-4.1-nano (fastest)</option>
                  </optgroup>
                  <optgroup label="GPT-4.5">
                    <option value="gpt-4.5-preview">gpt-4.5-preview</option>
                  </optgroup>
                  <optgroup label="Reasoning (o-series)">
                    <option value="o4-mini">o4-mini (fast reasoning)</option>
                    <option value="o3">o3 (best reasoning)</option>
                    <option value="o1">o1</option>
                  </optgroup>
                  <optgroup label="GPT-4o">
                    <option value="gpt-4o">gpt-4o</option>
                    <option value="gpt-4o-mini">gpt-4o-mini</option>
                  </optgroup>
                  <optgroup label="Legacy">
                    <option value="gpt-4-turbo">gpt-4-turbo</option>
                    <option value="gpt-3.5-turbo">gpt-3.5-turbo</option>
                  </optgroup>
                </select>
              </div>
            </div>
          )}

          {/* ── Anthropic config ── */}
          {form.model_enabled && form.model_provider === 'anthropic' && (
            <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 4 }}>◆ Anthropic Configuration</div>
              <div className="form-group">
                <label className="form-label">API Key</label>
                <input
                  className="form-input"
                  type="password"
                  value={form.anthropic_api_key}
                  onChange={(e) => setForm({ ...form, anthropic_api_key: e.target.value })}
                  placeholder="sk-ant-…"
                />
                <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                  Stored locally in the vault DB. Get yours at console.anthropic.com.
                </div>
              </div>
              <div className="form-group">
                <label className="form-label">Model</label>
                <select
                  className="form-input"
                  value={form.anthropic_model}
                  onChange={(e) => setForm({ ...form, anthropic_model: e.target.value })}
                >
                  <optgroup label="Claude 4 (Latest)">
                    <option value="claude-opus-4-6">claude-opus-4-6 (most capable)</option>
                    <option value="claude-sonnet-4-6">claude-sonnet-4-6 (best balance)</option>
                  </optgroup>
                  <optgroup label="Claude 3.7">
                    <option value="claude-3-7-sonnet-20250219">claude-3-7-sonnet (extended thinking)</option>
                  </optgroup>
                  <optgroup label="Claude 3.5">
                    <option value="claude-3-5-sonnet-20241022">claude-3-5-sonnet</option>
                    <option value="claude-haiku-4-5-20251001">claude-haiku-4-5 (fast, cheap)</option>
                    <option value="claude-3-5-haiku-20241022">claude-3-5-haiku</option>
                  </optgroup>
                  <optgroup label="Claude 3 (Legacy)">
                    <option value="claude-3-opus-20240229">claude-3-opus</option>
                    <option value="claude-3-haiku-20240307">claude-3-haiku</option>
                  </optgroup>
                </select>
                <div style={{ fontSize: 11, color: 'var(--text-faint)', marginTop: 4 }}>
                  Note: Anthropic does not provide embedding models. Embeddings will use Ollama if enabled.
                </div>
              </div>
            </div>
          )}

          {/* ── Embeddings ── */}
          <div className="card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div>
                <div style={{ fontWeight: 600, fontSize: 14 }}>🧮 Semantic Embeddings</div>
                <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
                  Vector similarity search. Requires AI model enabled.
                </div>
              </div>
              <Toggle
                value={form.embeddings_enabled}
                onChange={(v) => setForm({ ...form, embeddings_enabled: v })}
                disabled={!form.model_enabled}
              />
            </div>
            {form.embeddings_enabled && form.model_enabled && (
              <div className="form-group" style={{ marginTop: 12 }}>
                <label className="form-label">Embed Model</label>
                <input
                  className="form-input"
                  value={form.ollama_embed_model}
                  onChange={(e) => setForm({ ...form, ollama_embed_model: e.target.value })}
                  placeholder="nomic-embed-text"
                />
                <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                  Run: <code>ollama pull nomic-embed-text</code>
                </div>
              </div>
            )}
          </div>

          {/* ── Telegram Bot ── */}
          <div className="card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10 }}>
              <div>
                <div style={{ fontWeight: 600, fontSize: 14 }}>📱 Telegram Bot</div>
                <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
                  Control agents, check status, and approve memories from your phone.
                </div>
              </div>
              <Toggle value={form.telegram_enabled} onChange={(v) => setForm({ ...form, telegram_enabled: v })} />
            </div>

            {form.telegram_enabled && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 10, marginTop: 4 }}>
                <div className="form-group">
                  <label className="form-label">Bot Token</label>
                  <input
                    className="form-input"
                    type="password"
                    value={form.telegram_token}
                    onChange={(e) => setForm({ ...form, telegram_token: e.target.value })}
                    placeholder="123456789:ABCDef..."
                  />
                  <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                    Create a bot via <a href="https://t.me/BotFather" style={{ color: 'var(--accent)' }}>@BotFather</a> on Telegram.
                  </div>
                </div>
                <div className="form-group">
                  <label className="form-label">Allowed Chat ID</label>
                  <input
                    className="form-input"
                    value={form.telegram_chat_id}
                    onChange={(e) => setForm({ ...form, telegram_chat_id: e.target.value })}
                    placeholder="123456789"
                  />
                  <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                    Your personal chat ID. Get it from <a href="https://t.me/userinfobot" style={{ color: 'var(--accent)' }}>@userinfobot</a>.
                    Only this ID can send commands to the bot.
                  </div>
                </div>

                {/* Bot start/stop control */}
                <div style={{
                  display: 'flex', alignItems: 'center', gap: 10,
                  padding: '10px 12px',
                  background: botRunning ? 'rgba(74,222,128,0.08)' : 'rgba(148,163,184,0.08)',
                  border: `1px solid ${botRunning ? 'rgba(74,222,128,0.3)' : 'var(--border)'}`,
                  borderRadius: 'var(--radius-sm)', fontSize: 13,
                }}>
                  <span style={{ flex: 1, color: botRunning ? 'var(--success)' : 'var(--text-muted)' }}>
                    {botRunning ? '● Bot is running' : '○ Bot is stopped'}
                  </span>
                  <button
                    type="button"
                    className={`btn ${botRunning ? 'btn-danger' : 'btn-primary'}`}
                    disabled={botLoading}
                    onClick={handleBotToggle}
                    style={{ fontSize: 12, padding: '4px 12px' }}
                  >
                    {botLoading ? '…' : botRunning ? 'Stop Bot' : 'Start Bot'}
                  </button>
                </div>

                <div style={{
                  padding: '8px 10px', background: 'rgba(251,191,36,0.08)',
                  border: '1px solid rgba(251,191,36,0.3)', borderRadius: 'var(--radius-sm)',
                  fontSize: 11, color: 'var(--warning)',
                }}>
                  ⚠ Save settings before starting the bot. Token is stored in plaintext in the local DB.
                </div>
              </div>
            )}
          </div>

          {/* ── Discord Bot ── */}
          <div className="card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10 }}>
              <div>
                <div style={{ fontWeight: 600, fontSize: 14 }}>Discord Bot</div>
                <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
                  Control agents from Discord DMs. Requires Message Content Intent enabled.
                </div>
              </div>
              <Toggle value={form.discord_enabled} onChange={(v) => setForm({ ...form, discord_enabled: v })} />
            </div>

            {form.discord_enabled && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 10, marginTop: 4 }}>
                <div className="form-group">
                  <label className="form-label">Bot Token</label>
                  <input
                    className="form-input"
                    type="password"
                    value={form.discord_token}
                    onChange={(e) => setForm({ ...form, discord_token: e.target.value })}
                    placeholder="MTI3…"
                  />
                  <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                    Create a bot at discord.com/developers/applications. Enable <b>Message Content Intent</b> under Bot → Privileged Gateway Intents.
                  </div>
                </div>
                <div className="form-group">
                  <label className="form-label">Your Discord User ID</label>
                  <input
                    className="form-input"
                    value={form.discord_allowed_user_id}
                    onChange={(e) => setForm({ ...form, discord_allowed_user_id: e.target.value })}
                    placeholder="123456789012345678"
                  />
                  <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                    Enable Developer Mode in Discord (Settings → Advanced), then right-click your username → Copy User ID.
                    Only this user can send commands to the bot.
                  </div>
                </div>

                <div style={{
                  display: 'flex', alignItems: 'center', gap: 10,
                  padding: '10px 12px',
                  background: discordRunning ? 'rgba(74,222,128,0.08)' : 'rgba(148,163,184,0.08)',
                  border: `1px solid ${discordRunning ? 'rgba(74,222,128,0.3)' : 'var(--border)'}`,
                  borderRadius: 'var(--radius-sm)', fontSize: 13,
                }}>
                  <span style={{ flex: 1, color: discordRunning ? 'var(--success)' : 'var(--text-muted)' }}>
                    {discordRunning ? '● Bot is running' : '○ Bot is stopped'}
                  </span>
                  <button
                    type="button"
                    className={`btn ${discordRunning ? 'btn-danger' : 'btn-primary'}`}
                    disabled={discordLoading}
                    onClick={handleDiscordToggle}
                    style={{ fontSize: 12, padding: '4px 12px' }}
                  >
                    {discordLoading ? '…' : discordRunning ? 'Stop Bot' : 'Start Bot'}
                  </button>
                </div>
              </div>
            )}
          </div>

          {/* ── WhatsApp Bot ── */}
          <div className="card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10 }}>
              <div>
                <div style={{ fontWeight: 600, fontSize: 14 }}>WhatsApp Bot</div>
                <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
                  Control agents via WhatsApp. Requires a Meta Business account and a public webhook URL.
                </div>
              </div>
              <Toggle value={form.whatsapp_enabled} onChange={(v) => setForm({ ...form, whatsapp_enabled: v })} />
            </div>

            {form.whatsapp_enabled && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 10, marginTop: 4 }}>
                <div className="form-group">
                  <label className="form-label">Access Token</label>
                  <input
                    className="form-input"
                    type="password"
                    value={form.whatsapp_access_token}
                    onChange={(e) => setForm({ ...form, whatsapp_access_token: e.target.value })}
                    placeholder="EAABs…"
                  />
                  <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                    From Meta Developers → WhatsApp → API Setup. Use a permanent token for production.
                  </div>
                </div>
                <div className="form-group">
                  <label className="form-label">Phone Number ID</label>
                  <input
                    className="form-input"
                    value={form.whatsapp_phone_number_id}
                    onChange={(e) => setForm({ ...form, whatsapp_phone_number_id: e.target.value })}
                    placeholder="1234567890"
                  />
                  <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                    Found in Meta Developers → WhatsApp → API Setup → Phone Number ID.
                  </div>
                </div>
                <div className="form-group">
                  <label className="form-label">Your WhatsApp Number</label>
                  <input
                    className="form-input"
                    value={form.whatsapp_allowed_number}
                    onChange={(e) => setForm({ ...form, whatsapp_allowed_number: e.target.value })}
                    placeholder="15551234567"
                  />
                  <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                    Digits only, with country code (no + or spaces). Only this number can send commands.
                  </div>
                </div>
                <div className="form-group">
                  <label className="form-label">Webhook Verify Token</label>
                  <input
                    className="form-input"
                    value={form.whatsapp_verify_token}
                    onChange={(e) => setForm({ ...form, whatsapp_verify_token: e.target.value })}
                    placeholder="my-secret-token"
                  />
                  <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                    A secret string you choose. Enter the same value in Meta Developers → Webhooks → Verify Token.
                  </div>
                </div>
                <div className="form-group">
                  <label className="form-label">Local Webhook Port</label>
                  <input
                    className="form-input"
                    value={form.whatsapp_webhook_port}
                    onChange={(e) => setForm({ ...form, whatsapp_webhook_port: e.target.value })}
                    placeholder="8765"
                  />
                </div>

                <div style={{
                  padding: '8px 10px', background: 'rgba(96,165,250,0.08)',
                  border: '1px solid rgba(96,165,250,0.3)', borderRadius: 'var(--radius-sm)',
                  fontSize: 11, color: 'var(--info, #60a5fa)',
                }}>
                  To receive messages, expose port {form.whatsapp_webhook_port || '8765'} publicly, e.g.:<br />
                  <code>ngrok http {form.whatsapp_webhook_port || '8765'}</code><br />
                  Then set the webhook URL in Meta Developers to <code>https://&lt;ngrok-url&gt;/webhook</code>.
                </div>

                <div style={{
                  display: 'flex', alignItems: 'center', gap: 10,
                  padding: '10px 12px',
                  background: whatsappRunning ? 'rgba(74,222,128,0.08)' : 'rgba(148,163,184,0.08)',
                  border: `1px solid ${whatsappRunning ? 'rgba(74,222,128,0.3)' : 'var(--border)'}`,
                  borderRadius: 'var(--radius-sm)', fontSize: 13,
                }}>
                  <span style={{ flex: 1, color: whatsappRunning ? 'var(--success)' : 'var(--text-muted)' }}>
                    {whatsappRunning ? '● Webhook server running' : '○ Webhook server stopped'}
                  </span>
                  <button
                    type="button"
                    className={`btn ${whatsappRunning ? 'btn-danger' : 'btn-primary'}`}
                    disabled={whatsappLoading}
                    onClick={handleWhatsappToggle}
                    style={{ fontSize: 12, padding: '4px 12px' }}
                  >
                    {whatsappLoading ? '…' : whatsappRunning ? 'Stop Server' : 'Start Server'}
                  </button>
                </div>
              </div>
            )}
          </div>

          {/* ── Desktop Automation ── */}
          <div className="card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: form.desktop_control_enabled ? 10 : 0 }}>
              <div>
                <div style={{ fontWeight: 600, fontSize: 14 }}>🖱 Desktop Control</div>
                <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
                  Allow the app to type text, press key combos, and open files/URLs.
                </div>
              </div>
              <Toggle
                value={form.desktop_control_enabled}
                onChange={(v) => setForm({ ...form, desktop_control_enabled: v })}
              />
            </div>
            {form.desktop_control_enabled && (
              <div style={{
                padding: '10px 12px', background: 'rgba(239,68,68,0.08)',
                border: '1px solid rgba(239,68,68,0.3)', borderRadius: 'var(--radius-sm)',
                fontSize: 12, color: 'var(--danger, #ef4444)',
              }}>
                ⚠ Desktop control allows the app to send keystrokes and open applications on your behalf.
                Only enable when you trust the agents running.
              </div>
            )}
          </div>

          {/* ── Web Search ── */}
          <div className="card">
            <div style={{ fontWeight: 600, fontSize: 14, marginBottom: 10 }}>🔍 Web Search</div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              <label style={{ fontSize: 12 }}>
                <div style={{ marginBottom: 4, fontWeight: 600 }}>Provider</div>
                <select
                  className="form-input"
                  value={form.search_provider}
                  onChange={(e) => setForm({ ...form, search_provider: e.target.value })}
                  style={{ width: '100%' }}
                >
                  <option value="brave">Brave Search (cloud, API key required)</option>
                  <option value="searxng">SearXNG (self-hosted, free)</option>
                </select>
              </label>
              {form.search_provider === 'brave' && (
                <label style={{ fontSize: 12 }}>
                  <div style={{ marginBottom: 4, fontWeight: 600 }}>Brave API Key</div>
                  <input
                    className="form-input"
                    type="password"
                    value={form.brave_api_key}
                    onChange={(e) => setForm({ ...form, brave_api_key: e.target.value })}
                    placeholder="BSA…"
                    style={{ width: '100%' }}
                  />
                  <div style={{ fontSize: 11, color: 'var(--text-faint)', marginTop: 3 }}>
                    Get a free key at search.brave.com/search/api
                  </div>
                </label>
              )}
              {form.search_provider === 'searxng' && (
                <label style={{ fontSize: 12 }}>
                  <div style={{ marginBottom: 4, fontWeight: 600 }}>SearXNG URL</div>
                  <input
                    className="form-input"
                    type="text"
                    value={form.searxng_url}
                    onChange={(e) => setForm({ ...form, searxng_url: e.target.value })}
                    placeholder="http://localhost:8888"
                    style={{ width: '100%' }}
                  />
                  <div style={{ fontSize: 11, color: 'var(--text-faint)', marginTop: 3 }}>
                    Your SearXNG instance must have JSON format enabled.
                  </div>
                </label>
              )}
            </div>
          </div>

          {/* ── Agent Environment ── */}
          <div className="card">
            <div style={{ fontWeight: 600, fontSize: 14, marginBottom: 6 }}>📁 Agent Working Directory</div>
            <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 12 }}>
              The directory where agents are allowed to read and write files.
              Leave empty to use workspace directories instead.
            </div>
            <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <input
                className="form-input"
                value={form.agent_work_dir}
                onChange={(e) => setForm({ ...form, agent_work_dir: e.target.value })}
                placeholder="e.g. C:\Users\you\AgentFiles"
                style={{ flex: 1, fontFamily: 'monospace', fontSize: 12 }}
              />
              <button
                type="button"
                className="btn btn-ghost"
                style={{ fontSize: 12, whiteSpace: 'nowrap' }}
                onClick={async () => {
                  try {
                    const dir = await openFolderDialog();
                    if (dir) setForm({ ...form, agent_work_dir: dir });
                  } catch (e) {
                    addToast(String(e), 'error');
                  }
                }}
              >
                Browse…
              </button>
            </div>
            {form.agent_work_dir && (
              <div style={{
                marginTop: 8, padding: '6px 10px',
                background: 'rgba(74,222,128,0.08)',
                border: '1px solid rgba(74,222,128,0.25)',
                borderRadius: 'var(--radius-sm)',
                fontSize: 11, color: 'var(--success)',
              }}>
                ✓ Agents will write files inside: <code style={{ wordBreak: 'break-all' }}>{form.agent_work_dir}</code>
              </div>
            )}
          </div>

          {/* ── MCP Servers ── */}
          <div className="card">
            <div style={{ fontWeight: 600, fontSize: 14, marginBottom: 6 }}>🔌 MCP Servers</div>
            <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 14 }}>
              Connect Model Context Protocol tool servers. Their tools become available to agents.
            </div>

            {/* Server list */}
            {mcpServers.length > 0 && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 14 }}>
                {mcpServers.map((srv) => (
                  <div key={srv.id} style={{
                    display: 'flex', alignItems: 'center', gap: 8,
                    padding: '8px 10px', background: 'var(--bg-elevated)',
                    border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)', fontSize: 12,
                  }}>
                    <span style={{ flex: 1, fontWeight: 600 }}>{srv.label}</span>
                    <code style={{ color: 'var(--text-muted)', fontSize: 11 }}>{srv.command}</code>
                    <button
                      type="button"
                      className="btn btn-ghost"
                      style={{ fontSize: 11, padding: '2px 8px' }}
                      onClick={async () => {
                        try {
                          const updated = await refreshMcpTools(srv.id);
                          setMcpServers((prev) => prev.map((s) => s.id === srv.id ? updated : s));
                        } catch (e) {
                          addToast(String(e), 'error');
                        }
                      }}
                    >
                      Refresh tools
                    </button>
                    <button
                      type="button"
                      className="btn btn-ghost"
                      style={{ fontSize: 11, padding: '2px 8px', color: 'var(--danger, #ef4444)' }}
                      onClick={async () => {
                        try {
                          await removeMcpServer(srv.id);
                          setMcpServers((prev) => prev.filter((s) => s.id !== srv.id));
                        } catch (e) {
                          addToast(String(e), 'error');
                        }
                      }}
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
            )}

            {/* Add server form */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              <div style={{ display: 'flex', gap: 8 }}>
                <input
                  className="form-input"
                  placeholder="Label (e.g. Filesystem)"
                  value={mcpLabel}
                  onChange={(e) => setMcpLabel(e.target.value)}
                  style={{ flex: 1 }}
                />
                <input
                  className="form-input"
                  placeholder="Command (e.g. npx)"
                  value={mcpCommand}
                  onChange={(e) => setMcpCommand(e.target.value)}
                  style={{ flex: 1 }}
                />
              </div>
              <input
                className="form-input"
                placeholder="Args space-separated (e.g. -y @modelcontextprotocol/server-filesystem /tmp)"
                value={mcpArgs}
                onChange={(e) => setMcpArgs(e.target.value)}
              />
              {mcpError && (
                <div style={{ fontSize: 11, color: 'var(--danger, #ef4444)' }}>{mcpError}</div>
              )}
              <button
                type="button"
                className="btn btn-ghost"
                disabled={mcpAdding || !mcpLabel.trim() || !mcpCommand.trim()}
                style={{ alignSelf: 'flex-start', fontSize: 12 }}
                onClick={async () => {
                  setMcpAdding(true);
                  setMcpError('');
                  try {
                    const args = mcpArgs.trim() ? mcpArgs.trim().split(/\s+/) : [];
                    const srv = await addMcpServer(mcpLabel.trim(), mcpCommand.trim(), args);
                    setMcpServers((prev) => [...prev, srv]);
                    setMcpLabel('');
                    setMcpCommand('');
                    setMcpArgs('');
                    addToast(`MCP server "${srv.label}" added`, 'success');
                  } catch (e) {
                    setMcpError(String(e));
                  } finally {
                    setMcpAdding(false);
                  }
                }}
              >
                {mcpAdding ? '…' : '+ Add Server'}
              </button>
            </div>
          </div>

          {/* ── Save button ── */}
          <button
            type="submit"
            className={`btn ${saved ? 'btn-success' : 'btn-primary'}`}
            disabled={loading}
            style={{ alignSelf: 'flex-start' }}
          >
            {loading ? 'Saving…' : saved ? '✓ Saved!' : 'Save Settings'}
          </button>
        </form>

        {/* ── About ── */}
        <div className="card" style={{ marginTop: 24, padding: '12px 16px' }}>
          <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 6 }}>About this app</div>
          <div style={{ fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.8 }}>
            <div>Vault DB: <code style={{ color: 'var(--text)' }}>~/.local/share/local-first-agent-os/vault.db</code></div>
            <div>Encryption: <code>XChaCha20-Poly1305</code> · Key derivation: <code>Argon2id</code></div>
            <div>No telemetry · No cloud · All data stays on your device</div>
          </div>
        </div>
      </div>
    </div>
  );
}
