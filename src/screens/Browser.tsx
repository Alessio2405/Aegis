/**
 * Browser screen — embedded iframe with client-side history management.
 * Navigation controls (back/forward/refresh) are managed in React state
 * since we cannot access cross-origin iframe internals.
 *
 * Note: sites that set X-Frame-Options: DENY/SAMEORIGIN will refuse to embed;
 * they will show their own error page inside the iframe.
 */
import React, { useState, useRef } from 'react';

const TOOLBAR_H = 40;

function normalizeUrl(raw: string): string {
  const s = raw.trim();
  if (!s) return 'https://www.google.com';
  if (/^https?:\/\//i.test(s)) return s;
  if (/^[a-z0-9.-]+\.[a-z]{2,}/i.test(s)) return `https://${s}`;
  return `https://www.google.com/search?q=${encodeURIComponent(s)}`;
}

export default function Browser() {
  const [addressBar, setAddressBar] = useState('');
  const [iframeSrc, setIframeSrc]   = useState('');
  const [history, setHistory]       = useState<string[]>([]);
  const [histIdx, setHistIdx]       = useState(-1);
  const iframeRef                   = useRef<HTMLIFrameElement>(null);

  const canBack    = histIdx > 0;
  const canForward = histIdx < history.length - 1;

  const navigate = (raw: string) => {
    const url = normalizeUrl(raw);
    setAddressBar(url);
    setIframeSrc(url);
    // Truncate forward history, push new entry
    const newHist = [...history.slice(0, histIdx + 1), url];
    setHistory(newHist);
    setHistIdx(newHist.length - 1);
  };

  const goBack = () => {
    if (!canBack) return;
    const idx = histIdx - 1;
    setHistIdx(idx);
    setAddressBar(history[idx]);
    setIframeSrc(history[idx]);
  };

  const goForward = () => {
    if (!canForward) return;
    const idx = histIdx + 1;
    setHistIdx(idx);
    setAddressBar(history[idx]);
    setIframeSrc(history[idx]);
  };

  const refresh = () => {
    if (!iframeSrc) return;
    // Force iframe reload by briefly clearing src then restoring
    setIframeSrc('');
    requestAnimationFrame(() => setIframeSrc(iframeSrc));
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--bg-surface)' }}>

      {/* ── Toolbar ──────────────────────────────────────────────────── */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 4,
        padding: '0 8px', height: TOOLBAR_H,
        borderBottom: '1px solid var(--border)',
        background: 'var(--bg-raised)',
        flexShrink: 0,
      }}>
        <button
          className="btn btn-ghost"
          style={{ width: 30, height: 28, padding: 0, fontSize: 16, justifyContent: 'center', opacity: canBack ? 1 : 0.35 }}
          title="Back" onClick={goBack} disabled={!canBack}
        >←</button>
        <button
          className="btn btn-ghost"
          style={{ width: 30, height: 28, padding: 0, fontSize: 16, justifyContent: 'center', opacity: canForward ? 1 : 0.35 }}
          title="Forward" onClick={goForward} disabled={!canForward}
        >→</button>
        <button
          className="btn btn-ghost"
          style={{ width: 30, height: 28, padding: 0, fontSize: 18, justifyContent: 'center' }}
          title="Refresh" onClick={refresh}
        >↻</button>

        <div style={{
          flex: 1, display: 'flex', alignItems: 'center',
          background: 'var(--bg-input, rgba(255,255,255,0.06))',
          border: '1px solid var(--border)',
          borderRadius: 6, height: 28, padding: '0 10px', gap: 6,
        }}>
          <span style={{ fontSize: 11, color: 'var(--text-muted)', flexShrink: 0 }}>🌐</span>
          <input
            style={{
              flex: 1, background: 'transparent', border: 'none', outline: 'none',
              color: 'var(--text)', fontSize: 13, fontFamily: 'var(--font-mono)',
            }}
            value={addressBar}
            onChange={e => setAddressBar(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') navigate(addressBar); }}
            onFocus={e => e.currentTarget.select()}
            placeholder="Search or enter address…"
            spellCheck={false}
          />
        </div>

        <button
          className="btn btn-primary"
          style={{ height: 28, padding: '0 14px', fontSize: 13 }}
          onClick={() => navigate(addressBar)}
        >Go</button>

        <button
          className="btn btn-ghost"
          style={{ width: 30, height: 28, padding: 0, fontSize: 14, justifyContent: 'center' }}
          title="Home" onClick={() => navigate('https://www.google.com')}
        >⌂</button>
      </div>

      {/* ── Content area ─────────────────────────────────────────────── */}
      {iframeSrc ? (
        <iframe
          ref={iframeRef}
          src={iframeSrc}
          style={{ flex: 1, border: 'none', background: '#fff' }}
          sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox allow-top-navigation-by-user-activation"
          title="Browser"
        />
      ) : (
        <div style={{
          flex: 1, display: 'flex', flexDirection: 'column',
          alignItems: 'center', justifyContent: 'center',
          color: 'var(--text-muted)', gap: 16, userSelect: 'none',
        }}>
          <div style={{ fontSize: 56, lineHeight: 1 }}>🌐</div>
          <div style={{ fontSize: 14, fontWeight: 500 }}>Enter a URL or search above</div>
          <div style={{ fontSize: 12, opacity: 0.6, maxWidth: 280, textAlign: 'center' }}>
            Embedded via iframe. Some sites (Google, GitHub) block embedding — they will show their own error page.
          </div>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', justifyContent: 'center', marginTop: 8 }}>
            {['wikipedia.org', 'news.ycombinator.com', 'reddit.com', 'example.com'].map(site => (
              <button
                key={site} className="btn btn-ghost" style={{ fontSize: 12, height: 26 }}
                onClick={() => navigate(site)}
              >{site}</button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
