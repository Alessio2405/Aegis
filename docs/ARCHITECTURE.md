# Architecture

## High-Level Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                        React UI (TypeScript)                │
│  Onboarding │ Workspace │ Q&A │ MemoryInbox │ Permissions   │
│                    Audit │ Settings                         │
│                                                             │
│   store/ (Zustand)  ←→  api/tauri.ts (typed invoke())      │
└───────────────────────────┬─────────────────────────────────┘
                            │  Tauri IPC (JSON over pipe)
┌───────────────────────────▼─────────────────────────────────┐
│                     Tauri Core (Rust)                       │
│                                                             │
│  commands.rs — all IPC handlers, input validation           │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌────────────┐  ┌──────────┐ │
│  │  crypto/ │  │   db/    │  │ permissions│  │  audit/  │ │
│  │Argon2id  │  │ SQLite   │  │capability  │  │append-   │ │
│  │XChaCha20 │  │  FTS5    │  │checks      │  │only log  │ │
│  └──────────┘  └──────────┘  └────────────┘  └──────────┘ │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌────────────────────────────┐│
│  │ ingest/  │  │retrieval/│  │       agent/               ││
│  │ chunk    │  │ FTS5 +   │  │ router → planner →         ││
│  │ encrypt  │  │ cosine   │  │ executor → verifier →      ││
│  │ FTS idx  │  │ sim.     │  │ memory_writer              ││
│  └──────────┘  └──────────┘  └────────────────────────────┘│
│                                                             │
│  connectors/ (optional, feature-gated)                      │
│  └── ollama.rs  (http://localhost:11434)                    │
└─────────────────────────────────────────────────────────────┘
                            │
                     SQLite vault.db
                     vault_salt.bin  (salt only, never key)
```

## Component Responsibilities

### Frontend (src/)

| Component | Responsibility |
|-----------|---------------|
| `store/index.ts` | Zustand global state: vault status, active workspace, settings, toasts |
| `api/tauri.ts` | Typed wrappers around `invoke()` — single source of truth for all IPC |
| `screens/Onboarding` | Vault create + unlock flows |
| `screens/Workspace` | Add workspace folder, trigger ingest, view documents |
| `screens/QA` | Question input, answer display, citations panel, source viewer |
| `screens/MemoryInbox` | Accept/edit/reject proposed memories |
| `screens/Permissions` | View and revoke capability grants |
| `screens/Audit` | Filterable timeline of all events |
| `screens/Settings` | Enable/configure optional Ollama model |
| `components/CitationList` | Renders citations with doc title, snippet, "Open source" button |
| `components/SourceViewer` | Modal showing full decrypted chunk text + file path |

### Backend (src-tauri/src/)

| Module | Responsibility |
|--------|---------------|
| `main.rs` | Tauri builder, state initialization, command registration |
| `commands.rs` | All `#[tauri::command]` handlers |
| `state.rs` | `AppState`: `Mutex<Connection>`, `Mutex<Option<CryptoContext>>` |
| `db/` | `open()` → schema migration; `get/set_setting()` |
| `db/schema.rs` | All DDL in one constant (`SCHEMA_SQL`) |
| `crypto/` | `CryptoContext`: `derive(password, salt)`, `encrypt/decrypt` |
| `ingest/` | `ingest_workspace()`: scan → chunk → encrypt → FTS index |
| `retrieval/` | `fts_search()`, `get_chunk_full_text()`, `embedding_search()` |
| `permissions/` | `check()`, `require()`, `grant()`, `revoke()`, `bootstrap_defaults()` |
| `audit/` | `log()`, `query()` — append-only audit trail |
| `agent/router` | Heuristic classifier: Q&A / MemoryOps / Ingest / Settings |
| `agent/planner` | Deterministic plan builder (JSON plan steps) |
| `agent/executor` | Step runner: retrieves chunks → synthesizes → proposes memories |
| `agent/memory_writer` | `propose()`, `accept()`, `reject()`, `edit_and_accept()`, `list()` |
| `connectors/ollama` | Optional HTTP wrapper for Ollama chat + embeddings |

## Agent Runtime Flow

```
User question
     │
     ▼
 Router.classify()  →  RequestKind::QAndA
     │
     ▼
 Planner.plan()     →  Plan { steps: [RetrieveChunks, SynthesizeAnswer, ProposeMemory] }
     │
     ▼
 Executor.execute()
     ├─ Step: RetrieveChunks
     │    └─ permissions::require(file.read)  ← CHECKED
     │    └─ retrieval::fts_search()
     │    └─ Verifier: if empty → polite refusal
     │
     ├─ Step: SynthesizeAnswer
     │    └─ if model_enabled → connectors::ollama::chat()
     │    └─ else → extractive answer (stitched snippets)
     │
     └─ Step: ProposeMemory
          └─ memory_writer::propose()  → status=proposed → Inbox
```

## Data Flow: Vault Creation

```
User enters password
       │
       ▼
 crypto::generate_salt()  →  saved to vault_salt.bin
       │
       ▼
 CryptoContext::derive(password, salt)  →  held in memory only
       │
       ▼
 permissions::bootstrap_defaults()     →  vault.read + vault.write + file.read granted
       │
       ▼
 audit::log(vault_create)
```

## Security Boundaries

```
┌─────────────────────────────────────┐
│  PLAINTEXT ZONE (in-process RAM)    │
│  - CryptoContext.key (zeroed on     │
│    drop / vault lock)               │
│  - Decrypted chunk text (briefly    │
│    during retrieval, never written) │
└─────────────────────────────────────┘
              │ encrypt before write
              ▼
┌─────────────────────────────────────┐
│  SQLITE (vault.db)                  │
│  - ciphertext_text BLOB  ← encrypted│
│  - content_ciphertext BLOB ← enc.  │
│  - plaintext_preview TEXT ← 240ch  │
│    preview for FTS (acceptable)     │
│  - All other fields: plaintext      │
└─────────────────────────────────────┘
```

## Capability Enforcement

Every tool call in the executor calls `permissions::require()` before any data access:

```
require(VaultRead)  → checked before any chunk/memory read
require(VaultWrite) → checked before any write (memories, settings)
require(FileRead)   → checked before ingest / retrieval
require(ModelUse)   → checked before Ollama call (disabled by default)
```

If denied: returns `Err(PermissionDenied)` → logged in audit → surfaced to UI.

## Key Design Decisions

1. **No arbitrary code execution**: The Executor is a strict step-runner over a typed enum. There is no `eval`, no shell exec, no dynamic dispatch from LLM output.

2. **Memory is never auto-saved**: `memory_writer::propose()` always writes `status='proposed'`. Only user action (Accept/Edit+Accept) transitions to `saved`.

3. **Write operations require explicit user approval**: All write paths go through the UI (accept memory, add workspace, update settings). The backend never initiates writes autonomously.

4. **Encryption key never touches disk**: `vault_salt.bin` stores only the Argon2 salt (16 bytes). The derived key lives only in `AppState::crypto` (behind `Mutex<Option<CryptoContext>>`). `CryptoContext::drop()` zeroes the key array.

5. **FTS preview is acceptable**: We store a 240-char plaintext preview for FTS indexing. This is a documented trade-off (see SECURITY.md). Full text is always encrypted.
