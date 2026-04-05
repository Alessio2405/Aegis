# Security Model

## Overview

Local-First Agent OS uses **record-level encryption** for sensitive fields within a plaintext SQLite database. This is NOT full-database encryption (like SQLCipher). The trade-offs are documented below.

---

## Encryption Scheme

### Key Derivation

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Algorithm | Argon2id | Memory-hard; resistant to GPU/ASIC brute force |
| Memory cost | 64 MB | Balances security vs. desktop startup time |
| Iterations | 3 | Combined with memory cost gives adequate work factor |
| Parallelism | 4 | Matches typical desktop CPU |
| Output length | 32 bytes | Matches XChaCha20 key size |
| Salt length | 16 bytes | Stored in `vault_salt.bin` |

Salt is randomly generated on vault creation and stored in plaintext alongside the DB.
**The derived key is NEVER written to disk.**

### Encryption

| Parameter | Value |
|-----------|-------|
| Algorithm | XChaCha20-Poly1305 |
| Nonce | 24 bytes, cryptographically random per encryption |
| Tag | 16 bytes (AEAD) |
| Storage | `nonce (24B) || ciphertext || tag (16B)` as BLOB |

Each encryption call generates a fresh random nonce → ciphertext uniqueness guaranteed even for identical plaintexts.

### What IS Encrypted

| Table | Field | Encrypted |
|-------|-------|-----------|
| `chunks` | `ciphertext_text` | ✅ Full chunk text |
| `memories` | `content_ciphertext` | ✅ Full memory content |

### What is NOT Encrypted

| Field | Reason |
|-------|--------|
| `chunks.plaintext_preview` | Required for FTS5 indexing. First 240 chars only. |
| `memories.plaintext_preview` | Short preview for UI display without decryption. |
| Document paths, titles | Needed for retrieval and display |
| Workspace paths, names | Needed for UI and file system operations |
| Audit log fields | Intentionally plaintext for forensic value |
| Permission records | Intentionally plaintext for enforcement |

---

## Threat Model

### Threats Mitigated

| Threat | Mitigation |
|--------|-----------|
| Theft of vault.db file without password | Full chunk/memory text encrypted; attacker gets only previews and metadata |
| Brute-force of vault password | Argon2id makes each guess ~100ms; 10^9 guesses ≈ 317 years on GPU |
| Ciphertext malleability | Poly1305 AEAD tag detects tampering; decryption fails |
| Nonce reuse | 24-byte random nonces; collision probability negligible for any realistic vault size |
| Key persistence in swap | `CryptoContext::drop()` zeroes key bytes; OS swap not addressed |
| LLM executing arbitrary actions | Executor is a typed enum step-runner; no arbitrary code path |
| Unauthorized data access | Capability checks on every operation + audit log |

### Threats NOT Mitigated (Limitations)

| Threat | Status |
|--------|--------|
| OS-level memory scraping | Key lives in process RAM; advanced attackers with OS access can extract it |
| Swap/hibernation leaking key | Key may appear in swap; mitigated partially by mlock (not implemented in MVP) |
| 240-char preview leaks | FTS preview is plaintext; attacker with DB file sees first 240 chars of each chunk |
| Metadata leakage | File paths, document titles, timestamps are plaintext |
| Side-channel attacks | Not defended against in MVP |
| Passphrase in process args | Password passed via IPC, not process args; risk is IPC pipe snooping (mitigated by OS) |
| Vault file integrity | No HMAC over the DB; table structure could be modified by attacker |

---

## Vault Salt File

`vault_salt.bin` (16 bytes) is stored alongside `vault.db`. It is **not** secret — Argon2 salts are designed to be public. Its only purpose is to prevent precomputed rainbow table attacks.

**Do not delete `vault_salt.bin`**: without it, the vault cannot be unlocked even with the correct password.

---

## Permission Security

The capability system enforces:
- `model.use` is **off by default** — no network calls happen without explicit user action
- Every capability check is logged to the audit trail
- "Denied" events are logged even when not granted — useful for detecting misuse

The permission table is plaintext in SQLite. An attacker with write access to `vault.db` could forge grants. This is a known MVP limitation; a production version would MAC-protect the permissions table.

---

## No Telemetry

- Zero network calls by default
- No crash reporting, no analytics, no update checks
- Optional Ollama calls are local-only (localhost)
- Any future external call would require explicit user consent + `model.use` capability grant

---

## Recommendations for Users

1. **Use a strong, unique vault password.** The Argon2id parameters make brute-force expensive, but a weak password is still the weakest link.
2. **Enable full-disk encryption** (FileVault / BitLocker / LUKS) on your OS for defense in depth.
3. **Lock the vault** when away from your machine — this zeroes the key from RAM.
4. **Back up `vault.db` + `vault_salt.bin` together** — the salt is required to unlock the backup.
5. **Never commit your vault files** to git (both are in `.gitignore`).

---

## Future Improvements

- [ ] `mlock()` to prevent key from being swapped to disk
- [ ] HMAC-protected audit log entries
- [ ] Full-DB encryption via SQLCipher as opt-in
- [ ] Encrypted FTS index (removes plaintext_preview requirement)
- [ ] Key rotation support
- [ ] Multi-key support (per-workspace keys)
