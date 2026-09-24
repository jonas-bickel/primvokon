//! SQLite persistence for recall captures, summaries, watcher events and agent transcripts.
//! All calls are synchronous; services wrap them in `spawn_blocking`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub id: i64,
    pub ts: i64,
    pub day: String,
    pub text: String,
    pub hash: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub day: String,
    pub summary: String,
    pub model: String,
    pub created_ts: i64,
    pub capture_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayOverview {
    pub day: String,
    pub capture_count: i64,
    pub has_summary: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchEvent {
    pub id: i64,
    pub ts: i64,
    pub reason: String,
    pub detail: String,
    pub thumbnail: Option<Vec<u8>>,
    pub notified: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub created_ts: i64,
    pub title: String,
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: i64,
    pub session_id: String,
    pub ts: i64,
    pub kind: String,
    pub content: String,
}

pub fn now_ts() -> i64 {
    Utc::now().timestamp()
}

/// Local calendar day (`YYYY-MM-DD`) of a unix timestamp.
pub fn day_of(ts: i64) -> String {
    let dt: DateTime<Local> = Local.timestamp_opt(ts, 0).single().unwrap_or_else(Local::now);
    dt.format("%Y-%m-%d").to_string()
}

pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

pub fn parse_day(day: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()
}

impl Storage {
    pub fn open_default() -> anyhow::Result<Self> {
        paths::ensure_dirs()?;
        Self::open(&paths::database_file())
    }

    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> anyhow::Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS recall_captures (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                day TEXT NOT NULL,
                text TEXT NOT NULL,
                hash TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'ocr'
             );
             CREATE INDEX IF NOT EXISTS idx_captures_day ON recall_captures(day);
             CREATE TABLE IF NOT EXISTS recall_summaries (
                day TEXT PRIMARY KEY,
                summary TEXT NOT NULL,
                model TEXT NOT NULL,
                created_ts INTEGER NOT NULL,
                capture_count INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS watch_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                reason TEXT NOT NULL,
                detail TEXT NOT NULL,
                thumbnail BLOB,
                notified INTEGER NOT NULL DEFAULT 0,
                error TEXT
             );
             CREATE TABLE IF NOT EXISTS agent_sessions (
                id TEXT PRIMARY KEY,
                created_ts INTEGER NOT NULL,
                title TEXT NOT NULL,
                mode TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS agent_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
                ts INTEGER NOT NULL,
                kind TEXT NOT NULL,
                content TEXT NOT NULL
             );",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> anyhow::Result<T> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("storage lock poisoned"))?;
        Ok(f(&conn)?)
    }

    // ----- recall -------------------------------------------------------------------

    pub fn last_capture_hash(&self) -> anyhow::Result<Option<String>> {
        self.with(|c| {
            c.query_row("SELECT hash FROM recall_captures ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
                .optional()
        })
    }

    pub fn insert_capture(&self, ts: i64, text: &str, hash: &str, source: &str) -> anyhow::Result<i64> {
        let day = day_of(ts);
        self.with(|c| {
            c.execute(
                "INSERT INTO recall_captures (ts, day, text, hash, source) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![ts, day, text, hash, source],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn captures_for_day(&self, day: &str) -> anyhow::Result<Vec<Capture>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id, ts, day, text, hash, source FROM recall_captures WHERE day = ?1 ORDER BY ts")?;
            let rows = stmt.query_map([day], |r| {
                Ok(Capture {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    day: r.get(2)?,
                    text: r.get(3)?,
                    hash: r.get(4)?,
                    source: r.get(5)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn upsert_summary(&self, day: &str, summary: &str, model: &str, capture_count: i64) -> anyhow::Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO recall_summaries (day, summary, model, created_ts, capture_count) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(day) DO UPDATE SET summary = excluded.summary, model = excluded.model,
                 created_ts = excluded.created_ts, capture_count = excluded.capture_count",
                params![day, summary, model, now_ts(), capture_count],
            )?;
            Ok(())
        })
    }

    pub fn summary_for_day(&self, day: &str) -> anyhow::Result<Option<Summary>> {
        self.with(|c| {
            c.query_row(
                "SELECT day, summary, model, created_ts, capture_count FROM recall_summaries WHERE day = ?1",
                [day],
                |r| {
                    Ok(Summary {
                        day: r.get(0)?,
                        summary: r.get(1)?,
                        model: r.get(2)?,
                        created_ts: r.get(3)?,
                        capture_count: r.get(4)?,
                    })
                },
            )
            .optional()
        })
    }

    /// Every day that has captures or a summary, newest first.
    pub fn days(&self) -> anyhow::Result<Vec<DayOverview>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT d.day, COALESCE(cnt.n, 0), (s.day IS NOT NULL)
                 FROM (SELECT day FROM recall_captures UNION SELECT day FROM recall_summaries) d
                 LEFT JOIN (SELECT day, COUNT(*) AS n FROM recall_captures GROUP BY day) cnt ON cnt.day = d.day
                 LEFT JOIN recall_summaries s ON s.day = d.day
                 ORDER BY d.day DESC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(DayOverview {
                    day: r.get(0)?,
                    capture_count: r.get(1)?,
                    has_summary: r.get::<_, i64>(2)? != 0,
                })
            })?;
            rows.collect()
        })
    }

    /// Days before `today` that have captures but no summary (oldest first).
    pub fn days_pending_summary(&self, today: &str) -> anyhow::Result<Vec<String>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT DISTINCT day FROM recall_captures
                 WHERE day < ?1 AND day NOT IN (SELECT day FROM recall_summaries)
                 ORDER BY day",
            )?;
            let rows = stmt.query_map([today], |r| r.get::<_, String>(0))?;
            rows.collect()
        })
    }

    /// Delete raw captures for summarised days older than `retention_days`.
    pub fn prune_captures(&self, today: &str, retention_days: u32) -> anyhow::Result<usize> {
        let cutoff = parse_day(today)
            .map(|d| d - chrono::Duration::days(i64::from(retention_days)))
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| today.to_string());
        self.with(|c| {
            c.execute(
                "DELETE FROM recall_captures WHERE day < ?1 AND day IN (SELECT day FROM recall_summaries)",
                [cutoff],
            )
        })
    }

    pub fn delete_day(&self, day: &str) -> anyhow::Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM recall_captures WHERE day = ?1", [day])?;
            c.execute("DELETE FROM recall_summaries WHERE day = ?1", [day])?;
            Ok(())
        })
    }

    // ----- watcher ------------------------------------------------------------------

    pub fn insert_watch_event(
        &self,
        reason: &str,
        detail: &str,
        thumbnail: Option<&[u8]>,
        notified: bool,
        error: Option<&str>,
    ) -> anyhow::Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO watch_events (ts, reason, detail, thumbnail, notified, error) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![now_ts(), reason, detail, thumbnail, notified as i64, error],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn watch_events(&self, limit: u32) -> anyhow::Result<Vec<WatchEvent>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, ts, reason, detail, thumbnail, notified, error FROM watch_events ORDER BY id DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit], |r| {
                Ok(WatchEvent {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    reason: r.get(2)?,
                    detail: r.get(3)?,
                    thumbnail: r.get(4)?,
                    notified: r.get::<_, i64>(5)? != 0,
                    error: r.get(6)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn clear_watch_events(&self) -> anyhow::Result<()> {
        self.with(|c| c.execute("DELETE FROM watch_events", []).map(|_| ()))
    }

    // ----- agent --------------------------------------------------------------------

    pub fn create_agent_session(&self, id: &str, title: &str, mode: &str) -> anyhow::Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO agent_sessions (id, created_ts, title, mode) VALUES (?1, ?2, ?3, ?4)",
                params![id, now_ts(), title, mode],
            )
            .map(|_| ())
        })
    }

    pub fn agent_sessions(&self, limit: u32) -> anyhow::Result<Vec<AgentSession>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id, created_ts, title, mode FROM agent_sessions ORDER BY created_ts DESC LIMIT ?1")?;
            let rows = stmt.query_map([limit], |r| {
                Ok(AgentSession {
                    id: r.get(0)?,
                    created_ts: r.get(1)?,
                    title: r.get(2)?,
                    mode: r.get(3)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn append_agent_message(&self, session_id: &str, kind: &str, content: &str) -> anyhow::Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO agent_messages (session_id, ts, kind, content) VALUES (?1, ?2, ?3, ?4)",
                params![session_id, now_ts(), kind, content],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn agent_messages(&self, session_id: &str) -> anyhow::Result<Vec<AgentMessage>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id, session_id, ts, kind, content FROM agent_messages WHERE session_id = ?1 ORDER BY id")?;
            let rows = stmt.query_map([session_id], |r| {
                Ok(AgentMessage {
                    id: r.get(0)?,
                    session_id: r.get(1)?,
                    ts: r.get(2)?,
                    kind: r.get(3)?,
                    content: r.get(4)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn delete_agent_session(&self, id: &str) -> anyhow::Result<()> {
        self.with(|c| c.execute("DELETE FROM agent_sessions WHERE id = ?1", [id]).map(|_| ()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_roundtrip_and_pending_days() {
        let s = Storage::open_in_memory().unwrap();
        let day1 = Local.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap().timestamp();
        let day2 = Local.with_ymd_and_hms(2026, 9, 23, 10, 0, 0).unwrap().timestamp();
        s.insert_capture(day1, "hello", "h1", "ocr").unwrap();
        s.insert_capture(day2, "world", "h2", "ocr").unwrap();
        assert_eq!(s.last_capture_hash().unwrap().as_deref(), Some("h2"));
        assert_eq!(s.days_pending_summary("2026-09-24").unwrap(), vec!["2026-09-22", "2026-09-23"]);
        s.upsert_summary("2026-09-22", "sum", "m", 1).unwrap();
        assert_eq!(s.days_pending_summary("2026-09-24").unwrap(), vec!["2026-09-23"]);
        let days = s.days().unwrap();
        assert_eq!(days.len(), 2);
        assert!(days[1].has_summary);
        assert_eq!(s.captures_for_day("2026-09-23").unwrap()[0].text, "world");
        // prune: 22nd is summarised and older than 1 day
        assert_eq!(s.prune_captures("2026-09-24", 1).unwrap(), 1);
        assert!(s.captures_for_day("2026-09-22").unwrap().is_empty());
        assert!(s.summary_for_day("2026-09-22").unwrap().is_some());
    }

    #[test]
    fn watch_events_and_agent_transcripts() {
        let s = Storage::open_in_memory().unwrap();
        s.insert_watch_event("ocr", "new text", Some(&[1, 2]), true, None).unwrap();
        let ev = s.watch_events(10).unwrap();
        assert_eq!(ev.len(), 1);
        assert!(ev[0].notified);
        s.create_agent_session("s1", "Reply to Anna", "ask").unwrap();
        s.append_agent_message("s1", "user", "hi").unwrap();
        assert_eq!(s.agent_messages("s1").unwrap().len(), 1);
        s.delete_agent_session("s1").unwrap();
        assert!(s.agent_messages("s1").unwrap().is_empty());
    }
}
