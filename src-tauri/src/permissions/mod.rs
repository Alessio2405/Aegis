//! Capability-based permission system.
//!
//! Capabilities:
//!   vault.read   — read any vault record
//!   vault.write  — write to vault (documents, memories, settings)
//!   file.read    — read files from a workspace folder
//!   model.use    — call an optional AI model
//!
//! Grant types:
//!   once    — single-use; consumed after first check (not persisted after use)
//!   always  — persisted; valid until revoked

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

use crate::error::{AppError, AppResult};
use crate::audit;

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    VaultRead,
    VaultWrite,
    FileRead,
    ModelUse,
}

impl Capability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::VaultRead  => "vault.read",
            Capability::VaultWrite => "vault.write",
            Capability::FileRead   => "file.read",
            Capability::ModelUse   => "model.use",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "vault.read"  => Some(Self::VaultRead),
            "vault.write" => Some(Self::VaultWrite),
            "file.read"   => Some(Self::FileRead),
            "model.use"   => Some(Self::ModelUse),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GrantType {
    Once,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub id: String,
    pub workspace_id: Option<String>,
    pub capability: String,
    pub scope_json: Option<String>,
    pub grant_type: String,
    pub created_at: String,
}

// ── Core enforcement ───────────────────────────────────────────────────────

/// Check whether `capability` is currently granted.
/// Logs an audit event either way.
/// Does NOT consume "once" grants — call `consume_once` after the action succeeds.
pub fn check(
    conn: &Connection,
    capability: &Capability,
    workspace_id: Option<&str>,
    actor: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
) -> AppResult<bool> {
    let cap_str = capability.as_str();

    // Look for any matching always grant
    let always_granted: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM permissions WHERE capability = ?1
         AND grant_type = 'always'
         AND (workspace_id IS NULL OR workspace_id = ?2)",
        params![cap_str, workspace_id.unwrap_or("")],
        |r| r.get(0),
    )?;

    let once_granted: bool = if !always_granted {
        conn.query_row(
            "SELECT COUNT(*) > 0 FROM permissions WHERE capability = ?1
             AND grant_type = 'once'
             AND (workspace_id IS NULL OR workspace_id = ?2)",
            params![cap_str, workspace_id.unwrap_or("")],
            |r| r.get(0),
        )?
    } else {
        false
    };

    let granted = always_granted || once_granted;
    let outcome = if granted { "allowed" } else { "denied" };

    audit::log(conn, audit::AuditEvent {
        actor: actor.to_string(),
        action_type: "permission_check".to_string(),
        capability: Some(cap_str.to_string()),
        target_type: target_type.map(str::to_string),
        target_id: target_id.map(str::to_string),
        workspace_id: workspace_id.map(str::to_string),
        outcome: outcome.to_string(),
        details_json: None,
    })?;

    Ok(granted)
}

/// Enforce a permission check — returns `Err(PermissionDenied)` if not granted.
pub fn require(
    conn: &Connection,
    capability: &Capability,
    workspace_id: Option<&str>,
    actor: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
) -> AppResult<()> {
    if check(conn, capability, workspace_id, actor, target_type, target_id)? {
        Ok(())
    } else {
        Err(AppError::PermissionDenied(capability.as_str().to_string()))
    }
}

// ── Grant management ───────────────────────────────────────────────────────

pub fn grant(
    conn: &Connection,
    capability: &Capability,
    workspace_id: Option<&str>,
    scope_json: Option<&str>,
    grant_type: GrantType,
) -> AppResult<Permission> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let cap_str = capability.as_str();
    let gt_str = match grant_type {
        GrantType::Once   => "once",
        GrantType::Always => "always",
    };
    conn.execute(
        "INSERT INTO permissions(id, workspace_id, capability, scope_json, grant_type, created_at)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, workspace_id, cap_str, scope_json, gt_str, now],
    )?;
    Ok(Permission {
        id,
        workspace_id: workspace_id.map(str::to_string),
        capability: cap_str.to_string(),
        scope_json: scope_json.map(str::to_string),
        grant_type: gt_str.to_string(),
        created_at: now,
    })
}

pub fn revoke(conn: &Connection, permission_id: &str) -> AppResult<()> {
    let rows = conn.execute("DELETE FROM permissions WHERE id = ?1", params![permission_id])?;
    if rows == 0 {
        Err(AppError::NotFound(format!("Permission {permission_id}")))
    } else {
        Ok(())
    }
}

pub fn list_all(conn: &Connection) -> AppResult<Vec<Permission>> {
    let mut stmt = conn.prepare(
        "SELECT id, workspace_id, capability, scope_json, grant_type, created_at
         FROM permissions ORDER BY created_at DESC"
    )?;
    let perms = stmt.query_map([], |r| {
        Ok(Permission {
            id: r.get(0)?,
            workspace_id: r.get(1)?,
            capability: r.get(2)?,
            scope_json: r.get(3)?,
            grant_type: r.get(4)?,
            created_at: r.get(5)?,
        })
    })?.filter_map(|r| r.ok()).collect();
    Ok(perms)
}

/// Bootstrap: grant vault.read + vault.write + file.read on vault creation.
pub fn bootstrap_defaults(conn: &Connection) -> AppResult<()> {
    for cap in [Capability::VaultRead, Capability::VaultWrite, Capability::FileRead] {
        grant(conn, &cap, None, None, GrantType::Always)?;
    }
    // model.use is intentionally NOT granted by default
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::db::schema::SCHEMA_SQL;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        conn
    }

    #[test]
    fn test_grant_and_check() {
        let conn = test_conn();
        grant(&conn, &Capability::VaultRead, None, None, GrantType::Always).unwrap();
        let ok = check(&conn, &Capability::VaultRead, None, "test", None, None).unwrap();
        assert!(ok);
    }

    #[test]
    fn test_denied_without_grant() {
        let conn = test_conn();
        let ok = check(&conn, &Capability::ModelUse, None, "test", None, None).unwrap();
        assert!(!ok);
    }

    #[test]
    fn test_require_returns_err_when_denied() {
        let conn = test_conn();
        let result = require(&conn, &Capability::ModelUse, None, "test", None, None);
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::PermissionDenied(cap) => assert_eq!(cap, "model.use"),
            other => panic!("Wrong error: {:?}", other),
        }
    }

    #[test]
    fn test_revoke() {
        let conn = test_conn();
        let perm = grant(&conn, &Capability::FileRead, None, None, GrantType::Always).unwrap();
        revoke(&conn, &perm.id).unwrap();
        let ok = check(&conn, &Capability::FileRead, None, "test", None, None).unwrap();
        assert!(!ok);
    }

    #[test]
    fn test_bootstrap_defaults() {
        let conn = test_conn();
        bootstrap_defaults(&conn).unwrap();
        // model.use must NOT be granted by default
        let model_ok = check(&conn, &Capability::ModelUse, None, "test", None, None).unwrap();
        assert!(!model_ok);
        // vault.read must be granted
        let read_ok = check(&conn, &Capability::VaultRead, None, "test", None, None).unwrap();
        assert!(read_ok);
    }
}
