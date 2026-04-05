import React, { useCallback, useEffect, useRef, useState } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { python } from '@codemirror/lang-python';
import { useAppStore } from '../store';
import * as api from '../api/tauri';
import { PythonInfo, PythonResult } from '../types';

// ── Default starter code ────────────────────────────────────────────────────

const STARTER = `# Python IDE — running inside your local-first OS
# Press Ctrl+Enter or click Run ▶ to execute

import sys

print(f"Python {sys.version}")
print("Hello from the agent OS!")
`;

// ── IDE screen ──────────────────────────────────────────────────────────────

export default function IDE() {
  const { addToast } = useAppStore();

  const [code, setCode]             = useState(STARTER);
  const [output, setOutput]         = useState<PythonResult | null>(null);
  const [running, setRunning]       = useState(false);
  const [pythonInfo, setPythonInfo] = useState<PythonInfo | null>(null);

  const outputRef = useRef<HTMLDivElement>(null);

  // Detect Python on mount
  useEffect(() => {
    api.getPythonInfo().then(setPythonInfo).catch(() => {
      setPythonInfo({ version: '', path: '', available: false });
    });
  }, []);

  // Scroll output into view when it updates
  useEffect(() => {
    outputRef.current?.scrollTo({ top: outputRef.current.scrollHeight, behavior: 'smooth' });
  }, [output]);

  const run = useCallback(async () => {
    if (running || !code.trim()) return;
    setRunning(true);
    try {
      const result = await api.runPython(code);
      setOutput(result);
    } catch (err) {
      addToast(String(err), 'error');
    } finally {
      setRunning(false);
    }
  }, [code, running, addToast]);

  // Ctrl+Enter to run
  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        run();
      }
    },
    [run],
  );

  const saveOutput = async () => {
    if (!output) return;
    const content = [
      output.stdout,
      output.stderr ? `\n--- stderr ---\n${output.stderr}` : '',
    ].join('').trim();
    try {
      const path = await api.generateFile(content, 'python_output', 'txt');
      await api.openFile(path);
    } catch (err) {
      addToast(String(err), 'error');
    }
  };

  // ── Python version badge ──────────────────────────────────────────────────

  const versionBadge = pythonInfo == null
    ? <span className="ide-badge ide-badge-dim">Detecting…</span>
    : pythonInfo.available
      ? <span className="ide-badge ide-badge-ok" title={pythonInfo.path}>{pythonInfo.version}</span>
      : <span className="ide-badge ide-badge-err" title="Python not found on PATH">Python not found</span>;

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="ide-root" onKeyDown={onKeyDown}>
      {/* ── Toolbar ────────────────────────────────────────────────────── */}
      <div className="ide-toolbar">
        <button
          className="btn btn-primary btn-sm ide-run-btn"
          onClick={run}
          disabled={running || !code.trim() || pythonInfo?.available === false}
          title="Run (Ctrl+Enter)"
        >
          {running ? <><span className="spinner" style={{ width: 11, height: 11, borderWidth: 2 }} /> Running…</> : '▶ Run'}
        </button>

        <button
          className="btn btn-ghost btn-sm"
          onClick={() => setOutput(null)}
          disabled={!output}
          title="Clear output"
        >
          Clear
        </button>

        <button
          className="btn btn-ghost btn-sm"
          onClick={saveOutput}
          disabled={!output}
          title="Save output as .txt"
        >
          Save output
        </button>

        <div style={{ flex: 1 }} />

        {versionBadge}
      </div>

      {/* ── Editor ─────────────────────────────────────────────────────── */}
      <div className="ide-editor-wrap">
        <CodeMirror
          value={code}
          height="100%"
          theme="dark"
          extensions={[python()]}
          onChange={setCode}
          basicSetup={{
            lineNumbers: true,
            highlightActiveLine: true,
            foldGutter: true,
            autocompletion: true,
            bracketMatching: true,
            closeBrackets: true,
          }}
          style={{ height: '100%', fontSize: 13 }}
        />
      </div>

      {/* ── Output panel ───────────────────────────────────────────────── */}
      <div className="ide-output-panel" ref={outputRef}>
        {!output && !running && (
          <span className="ide-output-placeholder">
            Run your code to see output here.
          </span>
        )}

        {running && (
          <span className="ide-output-placeholder">
            <span className="spinner" style={{ width: 12, height: 12, borderWidth: 2 }} />
            &nbsp;Running…
          </span>
        )}

        {output && !running && (
          <>
            {/* Meta row: exit code + duration + timeout warning */}
            <div className="ide-output-meta">
              <span
                className={output.exit_code === 0 ? 'ide-badge ide-badge-ok' : 'ide-badge ide-badge-err'}
              >
                exit {output.exit_code}
              </span>
              <span className="ide-badge ide-badge-dim">{output.duration_ms}ms</span>
              {output.timed_out && (
                <span className="ide-badge ide-badge-err">timed out</span>
              )}
            </div>

            {/* stdout */}
            {output.stdout && (
              <pre className="ide-output-stdout">{output.stdout}</pre>
            )}

            {/* stderr */}
            {output.stderr && (
              <pre className="ide-output-stderr">{output.stderr}</pre>
            )}

            {/* Nothing printed */}
            {!output.stdout && !output.stderr && (
              <span className="ide-output-placeholder" style={{ color: 'var(--text-faint)' }}>
                (no output)
              </span>
            )}
          </>
        )}
      </div>
    </div>
  );
}
