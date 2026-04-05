import React, { useEffect, useState } from 'react';
import { useAppStore } from '../store';
import { Permission } from '../types';
import * as api from '../api/tauri';

const CAPABILITY_INFO: Record<string, { icon: string; desc: string; risk: 'low' | 'medium' | 'high' }> = {
  'vault.read':  { icon: '👁',  desc: 'Read any encrypted vault record (chunks, memories)', risk: 'low' },
  'vault.write': { icon: '✏️',  desc: 'Write to the vault (documents, memories, settings)',  risk: 'medium' },
  'file.read':   { icon: '📂', desc: 'Read files from workspace folders on disk',           risk: 'medium' },
  'model.use':   { icon: '🤖', desc: 'Call an external AI model (Ollama). Off by default.',  risk: 'high' },
};

const RISK_TAG: Record<string, string> = {
  low: 'tag-green', medium: 'tag-yellow', high: 'tag-red',
};

export default function Permissions() {
  const { permissions, refreshPermissions, addToast } = useAppStore();
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setLoading(true);
    refreshPermissions().finally(() => setLoading(false));
  }, []);

  const handleRevoke = async (id: string, cap: string) => {
    const core = ['vault.read', 'vault.write', 'file.read'];
    if (core.includes(cap)) {
      if (!confirm(`Revoke "${cap}"? This will prevent core functionality. Are you sure?`)) return;
    }
    try {
      await api.revokePermission(id);
      addToast(`Permission "${cap}" revoked`, 'info');
      await refreshPermissions();
    } catch (e) { addToast(String(e), 'error'); }
  };

  // Group by capability
  const grouped: Record<string, Permission[]> = {};
  for (const p of permissions) {
    if (!grouped[p.capability]) grouped[p.capability] = [];
    grouped[p.capability].push(p);
  }

  const allCaps = ['vault.read', 'vault.write', 'file.read', 'model.use'];

  return (
    <div style={{ height: '100%', overflow: 'auto', padding: 24 }}>
      <div style={{ maxWidth: 720 }}>
        <h1 style={{ fontSize: 18, fontWeight: 700, marginBottom: 4 }}>🔐 Permissions Center</h1>
        <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 24 }}>
          Capability-based access control. Every tool call is checked against these grants.
          Revoking a capability will deny that operation and log a denied event in the audit log.
        </p>

        {loading ? (
          <div className="spinner" />
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
            {allCaps.map((cap) => {
              const info = CAPABILITY_INFO[cap];
              const grants = grouped[cap] ?? [];
              const isGranted = grants.length > 0;
              return (
                <div key={cap} className="card" style={{
                  borderColor: isGranted ? 'var(--border)' : 'var(--border)',
                  opacity: isGranted ? 1 : 0.7,
                }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                    <div style={{ flex: 1 }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                        <span style={{ fontSize: 18 }}>{info?.icon}</span>
                        <code style={{ fontSize: 13, color: 'var(--accent)', fontWeight: 600 }}>{cap}</code>
                        <span className={`tag ${RISK_TAG[info?.risk ?? 'low']}`} style={{ fontSize: 10 }}>
                          {info?.risk} risk
                        </span>
                        <span className={`tag ${isGranted ? 'tag-green' : 'tag-gray'}`} style={{ fontSize: 10 }}>
                          {isGranted ? 'GRANTED' : 'NOT GRANTED'}
                        </span>
                      </div>
                      <p style={{ fontSize: 12, color: 'var(--text-muted)' }}>{info?.desc}</p>
                    </div>
                  </div>

                  {isGranted && (
                    <div style={{ marginTop: 12, display: 'flex', flexDirection: 'column', gap: 6 }}>
                      {grants.map((g) => (
                        <div
                          key={g.id}
                          style={{
                            display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                            padding: '6px 10px',
                            background: 'var(--bg-elevated)',
                            borderRadius: 'var(--radius-sm)',
                          }}
                        >
                          <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                            <span className={`tag ${g.grant_type === 'always' ? 'tag-blue' : 'tag-yellow'}`} style={{ marginRight: 6, fontSize: 10 }}>
                              {g.grant_type}
                            </span>
                            {g.workspace_id ? `Workspace: ${g.workspace_id.slice(0, 8)}…` : 'Global'}
                            {' · '}
                            {new Date(g.created_at).toLocaleDateString()}
                          </div>
                          <button
                            className="btn btn-ghost btn-sm"
                            style={{ color: 'var(--danger)', fontSize: 11 }}
                            onClick={() => handleRevoke(g.id, g.capability)}
                          >
                            Revoke
                          </button>
                        </div>
                      ))}
                    </div>
                  )}

                  {!isGranted && cap === 'model.use' && (
                    <div style={{ marginTop: 8, fontSize: 12, color: 'var(--text-faint)' }}>
                      Enable AI model in Settings to grant this capability.
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}

        <div className="card" style={{ marginTop: 24, padding: '12px 16px' }}>
          <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 6 }}>How permissions work</div>
          <ul style={{ fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.8, paddingLeft: 16 }}>
            <li>Every internal action (retrieve, write, model call) checks capabilities before executing.</li>
            <li>A denied check is always logged in the Audit log.</li>
            <li><strong>once</strong> grants are for single-use (not implemented in MVP UI — granted always).</li>
            <li><strong>always</strong> grants persist until manually revoked here.</li>
            <li>Revoking vault.read or vault.write will break core functionality.</li>
          </ul>
        </div>
      </div>
    </div>
  );
}
