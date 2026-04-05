# Data Model

All data is stored in a single SQLite database (`vault.db`) using WAL mode and foreign keys.

## Entity Relationship

```
workspaces ──< documents ──< chunks ──< embeddings
                                └──> chunk_fts (FTS5 virtual table)

memories (optional workspace_id FK)
permissions (optional workspace_id FK)
audit_events (optional workspace_id FK)
settings (key-value)
```

---

## Tables

### `workspaces`

Represents a watched folder on disk.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | UUID v4 |
| `name` | TEXT NOT NULL | User-defined display name |
| `path` | TEXT NOT NULL UNIQUE | Absolute path on disk |
| `created_at` | TEXT | ISO 8601 RFC 3339 |

---

### `documents`

A single ingested file within a workspace.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | UUID v4 |
| `workspace_id` | TEXT FK→workspaces | CASCADE DELETE |
| `path` | TEXT | Absolute file path |
| `title` | TEXT | File stem (e.g., `project_alpha`) |
| `mtime` | INTEGER | Unix timestamp of file modification time |
| `hash` | TEXT | SHA-256 hex digest — used for change detection |
| `created_at` | TEXT | ISO 8601 |

Index: `(workspace_id)`, `(path)`

---

### `chunks`

A text segment (600–900 chars with ~100-char overlap) from a document.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | UUID v4 |
| `document_id` | TEXT FK→documents | CASCADE DELETE |
| `idx` | INTEGER | Chunk position within document (0-based) |
| `start_char` | INTEGER | Start character offset in original text |
| `end_char` | INTEGER | End character offset in original text |
| `ciphertext_text` | BLOB | **Encrypted** full chunk text (XChaCha20-Poly1305) |
| `plaintext_preview` | TEXT | First 240 chars **plaintext** — used for FTS and citation UI |
| `created_at` | TEXT | ISO 8601 |

Index: `(document_id)`

> **Security note**: `ciphertext_text` stores `nonce(24B) || ciphertext || tag(16B)`. The `plaintext_preview` is an intentional trade-off for FTS performance. See SECURITY.md.

---

### `chunk_fts` (FTS5 virtual table)

Full-text search index over chunk previews.

```sql
CREATE VIRTUAL TABLE chunk_fts USING fts5(
    title,
    plaintext_preview,
    content='',
    tokenize='porter unicode61'
);
```

- Populated via `INSERT INTO chunk_fts(rowid, title, plaintext_preview) SELECT rowid, ... FROM chunks WHERE id = ?`
- BM25 ranking via `bm25(chunk_fts)` — returns most relevant chunks first
- Porter stemmer: "running" matches "run"

---

### `embeddings` (optional)

Stores float32 embedding vectors for semantic similarity search.

| Column | Type | Notes |
|--------|------|-------|
| `chunk_id` | TEXT PK FK→chunks | One-to-one with chunks |
| `dim` | INTEGER | Vector dimension (e.g., 768 for nomic-embed-text) |
| `vector_blob` | BLOB | Raw `float32[]` in little-endian byte order |

Only populated when `embeddings_enabled = true` and Ollama is running.

---

### `memories`

User-curated knowledge extracted from documents and Q&A sessions.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | UUID v4 |
| `workspace_id` | TEXT NULL | NULL = global scope |
| `type` | TEXT | `Profile` \| `Project` \| `Procedure` \| `Episodic` |
| `status` | TEXT | `proposed` \| `saved` \| `rejected` |
| `content_ciphertext` | BLOB | **Encrypted** full memory text |
| `plaintext_preview` | TEXT | First 120 chars **plaintext** for UI |
| `source` | TEXT | Origin: `Q&A session`, `document:<id>`, `user_input` |
| `created_at` | TEXT | ISO 8601 |
| `updated_at` | TEXT | ISO 8601 — updated on accept/edit/reject |

Index: `(status)`

> Memories are **never auto-saved**. They must be explicitly accepted by the user in the Memory Inbox.

---

### `permissions`

Capability grants.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | UUID v4 |
| `workspace_id` | TEXT NULL | NULL = applies to all workspaces |
| `capability` | TEXT | `vault.read` \| `vault.write` \| `file.read` \| `model.use` |
| `scope_json` | TEXT NULL | Reserved for fine-grained scope (future use) |
| `grant_type` | TEXT | `once` \| `always` |
| `created_at` | TEXT | ISO 8601 |

Index: `(capability)`

**Default grants** created on vault creation:
- `vault.read` → always, global
- `vault.write` → always, global
- `file.read` → always, global
- `model.use` → **NOT granted** (requires user action in Settings)

---

### `audit_events`

Append-only log of all accesses and actions.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | UUID v4 |
| `ts` | TEXT | ISO 8601 timestamp |
| `actor` | TEXT | `user` \| `system` \| `agent` |
| `action_type` | TEXT | `read` \| `write` \| `ingest` \| `permission_check` \| `vault_create` \| `vault_unlock` \| `settings_update` \| `delete` \| `permission_revoke` |
| `capability` | TEXT NULL | Capability that was checked (if applicable) |
| `target_type` | TEXT NULL | `chunk` \| `memory` \| `document` \| `workspace` \| `vault` \| `permission` \| `settings` |
| `target_id` | TEXT NULL | ID of the target entity |
| `workspace_id` | TEXT NULL | Associated workspace |
| `outcome` | TEXT | `allowed` \| `denied` |
| `details_json` | TEXT NULL | Optional JSON with extra context |

Index: `(ts DESC)`, `(workspace_id)`, `(outcome)`

**Records are never deleted or modified.**

---

### `settings`

Simple key-value store for application settings.

| Key | Default | Notes |
|-----|---------|-------|
| `model_enabled` | `false` | Whether AI model calls are allowed |
| `model_provider` | `ollama` | Currently only `ollama` |
| `ollama_url` | `http://localhost:11434` | Ollama API base URL |
| `ollama_model` | `llama3` | Chat model name |
| `ollama_embed_model` | `nomic-embed-text` | Embedding model name |
| `embeddings_enabled` | `false` | Whether to compute/use embeddings |

---

## Chunking Parameters

| Parameter | Value |
|-----------|-------|
| Target chunk size | 600–900 chars |
| Overlap | ~100 chars |
| Preview length | 240 chars |
| Split preference | Paragraph boundary (`\n\n`) → sentence end (`./?/!`) → hard split |

## Indexing Strategy

1. **FTS5 (always)**: BM25 keyword search over `plaintext_preview + title`. Fast, no model required.
2. **Cosine similarity (optional)**: Brute-force over stored `float32` embeddings. Enabled only when `embeddings_enabled=true` and Ollama is running.
3. **Retrieval order**: FTS results ranked by BM25 score; top-K returned (default K=5).
