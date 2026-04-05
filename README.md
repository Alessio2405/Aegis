# Aegis

**Your AI agents. Your data. Your rules.**

A local-first, encrypted AI agent OS — run autonomous agents on your machine,
on your schedule, triggered by your files, with your data never leaving your device.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%20v1-24c8db)](https://tauri.app)
[![Rust](https://img.shields.io/badge/backend-Rust-orange)](https://www.rust-lang.org)

</div>

---

## Why Aegis?

Most AI agent tools require a cloud account, send your data to someone's server, and run in a terminal. Aegis is different:

|  | Aegis | Others |
|---|---|---|
| **Data location** | Your machine only | Cloud servers |
| **Encryption** | Argon2id + XChaCha20-Poly1305, per-record | None or at-rest only |
| **LLM provider** | Ollama (local) _or_ OpenAI | Usually OpenAI only |
| **UI** | Native desktop app | CLI or web |
| **Scheduled agents** | Built-in, no server needed | Requires external cron / cloud function |
| **File-triggered agents** | Built-in file watcher | Not available |
| **Permissions** | Per-capability, audited | Trust-all |

---

## Features

### Core
- **Encrypted vault** — Every record (agents, memories, API keys, chat history) is encrypted at rest with a password you control. The plaintext key lives only in memory while unlocked and is zeroed on lock.
- **Knowledge base** — Ingest Markdown, PDF, and plain-text files. Agents search them with full-text + semantic (vector) search.
- **Q&A with citations** — Every answer cites the exact document chunks it drew from.
- **Memory inbox** — Agents _propose_ memories. You review, edit, accept, or reject them. Nothing is stored without your approval.
- **Audit log** — Every agent action is logged with the capability used, outcome, and timestamp.
- **Permission system** — Capabilities (file access, shell, network, etc.) require explicit grants per agent.

### Agents
- **ReAct runtime** — Agents reason step-by-step: _think → act → observe → repeat_. No black-box magic.
- **Hands** — Pre-built agent templates (Researcher, Memory Curator, Daily Briefing, Lead Scout, Knowledge Distiller) you can deploy in one click.
- **Scheduled runs** — Set `every 5m`, `every 1h`, `every 24h` — no server or cron daemon required.
- **File watcher triggers** — Agents fire automatically when a file or folder changes on your filesystem.
- **Task queue** — Enqueue work for agents to pick up in priority order, even while you're away.
- **Pipelines** — Chain multiple agents into sequential workflows.
- **Artifacts feed** — Agents save structured findings (reports, summaries, research notes) to a persistent activity feed.

### Integrations
- **Ollama** (local) — run any open model on your machine, zero cloud dependency
- **OpenAI** — GPT-4o, GPT-4o-mini; API key stored encrypted in the vault
- **MCP servers** — connect any Model Context Protocol tool server
- **Telegram bot** — receive agent alerts and send commands remotely
- **Python IDE** — run Python scripts inside the app, in a sandboxed subprocess
- **Desktop automation** — agents can type text, press key combos, and open files on your desktop

---

## Screenshots

> _Demo GIF and screenshots coming soon. Contributions welcome._

---

## Installation

### Pre-built binaries (recommended)

Download the latest release for your platform from the [Releases](https://github.com/your-org/aegis/releases) page:

| Platform | File |
|---|---|
| Windows | `Aegis_0.1.0_x64.msi` |
| macOS (Intel) | `Aegis_0.1.0_x64.dmg` |
| macOS (Apple Silicon) | `Aegis_0.1.0_aarch64.dmg` |
| Linux | `Aegis_0.1.0_amd64.AppImage` |

**macOS note:** After installing, you may need to run `xattr -cr /Applications/Aegis.app` if Gatekeeper blocks the app.

---

### Build from source

#### Prerequisites

| Tool | Version | Notes |
|---|---|---|
| [Node.js](https://nodejs.org) | ≥ 18 | Frontend build |
| [Rust](https://rustup.rs) | stable | Backend |
| Tauri CLI v1 | `npm i -g @tauri-apps/cli@^1` | Desktop packaging |
| [Ollama](https://ollama.ai) | any | Optional, for local LLMs |

**Linux only** — install system dependencies:

```bash
# Ubuntu / Debian
sudo apt update && sudo apt install -y \
  libwebkit2gtk-4.0-dev libappindicator3-dev librsvg2-dev patchelf
```

**Windows** — install [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/).

#### Clone and run

```bash
git clone https://github.com/your-org/aegis.git
cd aegis
npm install
npm run tauri:dev
```

> First run compiles the Rust backend — allow 3–5 minutes. Subsequent runs are fast.

#### Build a release binary

```bash
npm run tauri:build
# Output: src-tauri/target/release/bundle/
```

---

## Quick start

**1. Launch Aegis.** On first run you'll see the onboarding screen.

**2. Create your vault.** Choose a strong password. There is no recovery — write it down and keep it safe.

**3. Add a workspace.** Workspaces → Add Workspace → select a folder of documents (Markdown, PDF, plain text). Click **Ingest** to index it. Try the `fixtures/` folder included in this repo.

**4. Ask a question.** Go to Q&A, type a question, and Aegis will search your documents and answer with citations.

**5. Create an agent.** Agents → New Agent → give it a name, system prompt, and tools. Or pick a pre-built **Hand** template and click **Use template**. Click **Run** to send it a task.

**6. Schedule it.** Add a schedule like `every 1h` and the agent runs automatically in the background.

---

## Configuration

### Ollama (local, default)

1. Install [Ollama](https://ollama.ai) and pull models:
   ```bash
   ollama pull llama3
   ollama pull nomic-embed-text   # for semantic search
   ```
2. Settings → Model Provider → **Ollama**
3. Set URL (default: `http://localhost:11434`) and model name
4. Enable **Embeddings** and set embed model to `nomic-embed-text`

> All Ollama calls go to `localhost`. Nothing leaves your machine.

### OpenAI

1. Settings → Model Provider → **OpenAI**
2. Paste your API key (stored encrypted in the vault, never logged)
3. Choose model: `gpt-4o` (best quality) or `gpt-4o-mini` (fast/cheap)

---

## Hands — pre-built agent templates

| Hand | Schedule | Description |
|---|---|---|
| 🔬 **Researcher** | every 24h | Deep-dives into your knowledge base, synthesises answers with citations |
| 🧠 **Memory Curator** | every 6h | Finds insights worth remembering, proposes structured memory entries |
| 📰 **Daily Briefing** | every 24h | Generates a daily summary: key topics, notable changes, open questions |
| 🎯 **Lead Scout** | every 12h | Scans for opportunities, patterns, and actionable signals |
| ⚗️ **Knowledge Distiller** | every 1h | Extracts core concepts into concise, standalone memory entries |

---

## Schedule format

```
every 5m    →  every 5 minutes
every 30m   →  every 30 minutes
every 1h    →  every hour
every 6h    →  every 6 hours
every 24h   →  daily
```

The scheduler checks every 60 seconds and only runs when the vault is unlocked.

---

## File watcher triggers

In **Agents → Triggers**, map any filesystem path to an agent. When any file in that path changes, the agent receives `"File changed: /path/to/file"` as its input and runs immediately.

Use cases:
- Process incoming files dropped into a folder
- Auto-index new documents
- Monitor log files for anomalies
- Trigger a briefing whenever a project file changes

A 5-second debounce prevents rapid re-firing from the same trigger.

---

## Agent tools reference

| Tool | What it does |
|---|---|
| `search_documents` | Semantic + full-text search across ingested workspaces |
| `chat_llm` | Call the configured LLM with a prompt |
| `propose_memory` | Submit a memory entry for your review |
| `read_file` | Read a file from the workspace or agent working directory |
| `write_file` | Write or overwrite a file |
| `list_files` | List files in a directory |
| `save_artifact` | Save a structured finding (report, summary, etc.) to the Activity feed |
| `run_python` | Execute Python code in a sandboxed subprocess (60s timeout) |
| `shell` | Run a shell command in the agent working directory |
| `git` | Run git operations: `status`, `diff`, `log`, `add`, `commit`, `pull`, `push`, `clone` |
| `desktop_type` | Type text on the desktop using OS-level automation |
| `desktop_key` | Press a key combination (e.g. `ctrl+c`, `cmd+space`) |
| `desktop_open` | Open a file or URL with the system default app |
| `delegate_task` | Queue a sub-task for another agent |
| `read_scratchpad` | Read a persistent key-value entry for this agent |
| `write_scratchpad` | Write a persistent key-value entry for this agent |

---

## Architecture

```
aegis/
├── src/                        # React 18 + TypeScript frontend
│   ├── api/tauri.ts            # Single source of truth for all invoke() calls
│   ├── screens/                # One file per screen
│   ├── store/index.ts          # Zustand global state
│   └── types/index.ts          # Shared TypeScript types
│
└── src-tauri/                  # Rust backend
    └── src/
        ├── agent/
        │   ├── router.rs       # Classify intent → plan type
        │   ├── planner.rs      # Build typed step list
        │   ├── executor.rs     # Execute steps with tool calls
        │   ├── react.rs        # ReAct multi-step loop
        │   ├── runner.rs       # Orchestrate full agent run
        │   ├── registry.rs     # CRUD for user agents
        │   ├── memory_writer.rs
        │   └── tools/          # One file per tool handler
        ├── connectors/
        │   ├── mod.rs          # LlmConfig + unified dispatch
        │   ├── ollama.rs
        │   └── openai.rs
        ├── scheduler/mod.rs    # Background scheduler + Hand templates
        ├── watcher/mod.rs      # File watcher (notify v6)
        ├── db/
        │   ├── schema.rs       # All DDL in one constant
        │   └── mod.rs          # Connection + migrations
        ├── crypto.rs           # Argon2id + XChaCha20-Poly1305
        ├── state.rs            # BotSharedState (Arc, Send+Sync)
        └── commands.rs         # All #[tauri::command] handlers
```

### Security model

- **Key derivation**: Argon2id (m=65536, t=3, p=1)
- **Encryption**: XChaCha20-Poly1305 authenticated encryption, unique 192-bit nonce per record
- **FTS trade-off**: First 240 chars of each document chunk are stored in plaintext for full-text search. All other content is encrypted. See the [security notes](#security-notes) below.
- **Key lifetime**: The derived key lives in memory only while the vault is unlocked. Locking the vault zeroes the key array.
- **No telemetry**: Aegis makes no outbound network calls except to the LLM provider you explicitly configure.

---

## Development

```bash
# Type-check the frontend
npm run typecheck

# Run frontend tests
npm run test

# Check Rust (no compilation output, fast)
cd src-tauri && cargo check

# Run Rust tests
cd src-tauri && cargo test

# Hot-reload dev mode (frontend + backend)
npm run tauri:dev
```

---

## Security notes

1. **FTS plaintext preview**: 240-character plaintext previews are stored for full-text search. If this is unacceptable for your threat model, disable FTS in settings (search degrades to vector-only).
2. **Shell tool**: Agents with the `shell` tool can execute arbitrary commands in the configured `agent_work_dir`. Only grant this tool to agents you trust with shell access.
3. **Desktop automation**: The `desktop_type` and `desktop_key` tools produce real OS-level input events. Use with care.
4. **Key rotation**: Changing the vault password requires re-encrypting all records. Not yet implemented — the vault password should be set once and kept safe.

---

## Roadmap

- [ ] CI/CD pipeline with pre-built binary releases
- [ ] Web search tool
- [ ] Browser automation (Chrome DevTools Protocol)
- [ ] Encrypted FTS (eliminate plaintext preview)
- [ ] Vault password rotation
- [ ] Plugin API for community-built tools
- [ ] Hands marketplace
- [ ] Encrypted device sync (local network, no cloud)

---

## Contributing

Contributions are welcome. Please open an issue before starting significant work so we can align on approach.

```bash
# 1. Fork and clone
git checkout -b feat/my-feature

# 2. Make changes, then verify
npm run typecheck
cd src-tauri && cargo check && cargo test

# 3. Open a pull request
```

Style:
- Rust: run `cargo fmt` before committing
- TypeScript: follow existing patterns
- Keep PRs focused — one feature or fix per PR

---

## Security disclosure

If you find a security vulnerability, please **do not** open a public issue. Email `security@your-domain.com` instead. We will respond within 72 hours.

---

## License

[MIT](LICENSE) — free for personal and commercial use.

---

## Acknowledgements

Built with [Tauri](https://tauri.app) · [Ollama](https://ollama.ai) · [rusqlite](https://github.com/rusqlite/rusqlite) · [notify](https://github.com/notify-rs/notify) · [chacha20poly1305](https://docs.rs/chacha20poly1305) · [argon2](https://docs.rs/argon2) · [React](https://react.dev) · [Zustand](https://zustand-demo.pmnd.rs)
