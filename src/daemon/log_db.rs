use crate::protocol::{StreamKind, TimestampedLine};
use anyhow::Context;
use rusqlite::Connection;
use std::sync::Mutex;
use uuid::Uuid;

pub struct LogDb {
    conn: Mutex<Connection>,
}

impl LogDb {
    pub fn open() -> anyhow::Result<Self> {
        crate::util::ensure_mso_dir()?;
        let path = crate::util::mso_dir().join("logs.db");
        let conn = Connection::open(&path).context("opening logs.db")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                process_id TEXT NOT NULL,
                stream INTEGER NOT NULL,
                line TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_logs_pid ON logs(process_id);"
        ).context("creating logs table")?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Open an in-memory SQLite database for testing/benchmarking
    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                process_id TEXT NOT NULL,
                stream INTEGER NOT NULL,
                line TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_logs_pid ON logs(process_id);"
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn insert(&self, process_id: Uuid, stream: i32, line: &str, timestamp: u64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO logs (process_id, stream, line, timestamp) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![process_id.to_string(), stream, line, timestamp],
        )?;
        Ok(())
    }

    pub fn get_logs(&self, process_id: Uuid, offset: usize, limit: usize) -> anyhow::Result<(Vec<TimestampedLine>, usize)> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        let total: usize = conn.query_row(
            "SELECT COUNT(*) FROM logs WHERE process_id = ?1",
            rusqlite::params![process_id.to_string()],
            |row| row.get::<_, i64>(0),
        ).unwrap_or(0) as usize;

        let mut stmt = conn.prepare(
            "SELECT line, timestamp FROM logs WHERE process_id = ?1 ORDER BY id DESC LIMIT ?2 OFFSET ?3"
        )?;

        let rows = stmt.query_map(
            rusqlite::params![process_id.to_string(), limit as i64, offset as i64],
            |row| {
                let line: String = row.get(0)?;
                let timestamp: i64 = row.get(1)?;
                Ok(TimestampedLine { timestamp: timestamp as u64, line })
            },
        )?;

        let mut logs: Vec<TimestampedLine> = Vec::with_capacity(limit);
        for l in rows.flatten() {
            logs.push(l);
        }
        logs.reverse();

        Ok((logs, total))
    }

    pub fn search_logs(&self, process_id: Uuid, query: &str, stream: Option<StreamKind>, offset: usize, limit: usize) -> anyhow::Result<(Vec<TimestampedLine>, usize)> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let like = format!("%{}%", query);
        let pid_s = process_id.to_string();

        let (total, logs) = match stream {
            Some(s) => {
                let total: usize = conn.query_row(
                    "SELECT COUNT(*) FROM logs WHERE process_id = ?1 AND line LIKE ?2 AND stream = ?3",
                    rusqlite::params![pid_s, like, s as i32],
                    |row| row.get::<_, i64>(0),
                ).unwrap_or(0) as usize;

                let mut stmt = conn.prepare(
                    "SELECT line, timestamp FROM logs WHERE process_id = ?1 AND line LIKE ?2 AND stream = ?3 ORDER BY id DESC LIMIT ?4 OFFSET ?5"
                )?;
                let rows = stmt.query_map(
                    rusqlite::params![pid_s, like, s as i32, limit as i64, offset as i64],
                    |row| {
                        let line: String = row.get(0)?;
                        let timestamp: i64 = row.get(1)?;
                        Ok(TimestampedLine { timestamp: timestamp as u64, line })
                    },
                )?;
                let mut logs: Vec<TimestampedLine> = Vec::with_capacity(limit);
                for l in rows.flatten() { logs.push(l); }
                logs.reverse();
                (total, logs)
            }
            None => {
                let total: usize = conn.query_row(
                    "SELECT COUNT(*) FROM logs WHERE process_id = ?1 AND line LIKE ?2",
                    rusqlite::params![pid_s, like],
                    |row| row.get::<_, i64>(0),
                ).unwrap_or(0) as usize;

                let mut stmt = conn.prepare(
                    "SELECT line, timestamp FROM logs WHERE process_id = ?1 AND line LIKE ?2 ORDER BY id DESC LIMIT ?3 OFFSET ?4"
                )?;
                let rows = stmt.query_map(
                    rusqlite::params![pid_s, like, limit as i64, offset as i64],
                    |row| {
                        let line: String = row.get(0)?;
                        let timestamp: i64 = row.get(1)?;
                        Ok(TimestampedLine { timestamp: timestamp as u64, line })
                    },
                )?;
                let mut logs: Vec<TimestampedLine> = Vec::with_capacity(limit);
                for l in rows.flatten() { logs.push(l); }
                logs.reverse();
                (total, logs)
            }
        };

        Ok((logs, total))
    }

    pub fn prune_before(&self, older_than_epoch: u64, process_id: Option<Uuid>) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let count = match process_id {
            Some(pid) => {
                conn.execute(
                    "DELETE FROM logs WHERE process_id = ?1 AND timestamp < ?2",
                    rusqlite::params![pid.to_string(), older_than_epoch as i64],
                )?
            }
            None => {
                conn.execute(
                    "DELETE FROM logs WHERE timestamp < ?1",
                    rusqlite::params![older_than_epoch as i64],
                )?
            }
        };
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> LogDb {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                process_id TEXT NOT NULL,
                stream INTEGER NOT NULL,
                line TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_logs_pid ON logs(process_id);"
        ).unwrap();
        LogDb { conn: Mutex::new(conn) }
    }

    #[test]
    fn test_insert_and_count() {
        let db = test_db();
        let pid = Uuid::new_v4();
        db.insert(pid, 0, "hello", 1000).unwrap();
        db.insert(pid, 1, "world", 2000).unwrap();
        db.insert(pid, 0, "test", 3000).unwrap();
        let (logs, total) = db.get_logs(pid, 0, 10).unwrap();
        assert_eq!(total, 3);
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[0].line, "hello");
    }

    #[test]
    fn test_get_logs_pagination() {
        let db = test_db();
        let pid = Uuid::new_v4();
        for i in 0..10 {
            db.insert(pid, 0, &format!("line {}", i), i * 1000).unwrap();
        }
        let (logs, total) = db.get_logs(pid, 0, 3).unwrap();
        assert_eq!(total, 10);
        assert_eq!(logs.len(), 3);
        // Newest first page: ids 9,8,7 → reversed: line 7, 8, 9
        assert_eq!(logs[0].line, "line 7");
        assert_eq!(logs[2].line, "line 9");

        let (logs2, _) = db.get_logs(pid, 3, 3).unwrap();
        assert_eq!(logs2.len(), 3);
        assert_eq!(logs2[0].line, "line 4");
    }

    #[test]
    fn test_search_logs() {
        let db = test_db();
        let pid = Uuid::new_v4();
        db.insert(pid, 0, "error: connection refused", 1000).unwrap();
        db.insert(pid, 0, "info: server started", 2000).unwrap();
        db.insert(pid, 0, "error: timeout", 3000).unwrap();
        let (logs, total) = db.search_logs(pid, "error", None, 0, 10).unwrap();
        assert_eq!(total, 2);
        assert_eq!(logs.len(), 2);
        assert!(logs[0].line.contains("error"));
    }

    #[test]
    fn test_search_logs_with_stream_filter() {
        let db = test_db();
        let pid = Uuid::new_v4();
        db.insert(pid, 0, "stdout line", 1000).unwrap();
        db.insert(pid, 1, "stderr line", 2000).unwrap();
        let (logs, total) = db.search_logs(pid, "line", Some(StreamKind::Stderr), 0, 10).unwrap();
        assert_eq!(total, 1);
        assert_eq!(logs[0].line, "stderr line");
    }
}
