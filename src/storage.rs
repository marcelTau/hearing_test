use crate::domain::{SessionStatus, TestSession};
use directories::ProjectDirs;
use rusqlite::{Connection, OptionalExtension, params};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("could not determine the local application-data directory")]
    NoDataDirectory,
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("database operation failed: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("stored session data is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the selected session cannot be used as this setup's baseline")]
    InvalidBaseline,
}

pub struct SessionRepository {
    connection: Connection,
    path: PathBuf,
}

impl SessionRepository {
    pub fn open_default() -> Result<Self, StorageError> {
        let dirs = ProjectDirs::from("org", "Personal", "Hearing Tracker")
            .ok_or(StorageError::NoDataDirectory)?;
        fs::create_dir_all(dirs.data_local_dir())?;
        Self::open(dirs.data_local_dir().join("history.sqlite3"))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(&path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS sessions (
                 id TEXT PRIMARY KEY,
                 started_at INTEGER NOT NULL,
                 status TEXT NOT NULL,
                 setup_fingerprint TEXT NOT NULL,
                 high_confidence INTEGER NOT NULL DEFAULT 0,
                 data_json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS sessions_started_at
                 ON sessions(started_at DESC);
             CREATE INDEX IF NOT EXISTS sessions_setup
                 ON sessions(setup_fingerprint, high_confidence, started_at);
             CREATE TABLE IF NOT EXISTS baselines (
                 setup_fingerprint TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS preferences (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             PRAGMA user_version = 1;",
        )?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }

        Ok(Self { connection, path })
    }

    #[cfg(test)]
    pub(crate) fn make_read_only_for_test(&self) {
        self.connection
            .execute_batch("PRAGMA query_only = ON;")
            .unwrap();
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&self, session: &TestSession) -> Result<(), StorageError> {
        let status = match session.status {
            SessionStatus::InProgress => "in_progress",
            SessionStatus::Completed => "completed",
            SessionStatus::Abandoned => "abandoned",
        };
        let high_confidence = session
            .quality
            .as_ref()
            .is_some_and(|quality| quality.high_confidence);
        self.connection.execute(
            "INSERT INTO sessions
                 (id, started_at, status, setup_fingerprint, high_confidence, data_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                 status = excluded.status,
                 setup_fingerprint = excluded.setup_fingerprint,
                 high_confidence = excluded.high_confidence,
                 data_json = excluded.data_json",
            params![
                session.id.to_string(),
                session.started_at.unix_timestamp(),
                status,
                session.setup_fingerprint,
                high_confidence,
                serde_json::to_string(session)?,
            ],
        )?;

        if session.status == SessionStatus::Completed && high_confidence {
            self.connection.execute(
                "INSERT OR IGNORE INTO baselines(setup_fingerprint, session_id)
                 VALUES (?1, ?2)",
                params![session.setup_fingerprint, session.id.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<TestSession>, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM sessions ORDER BY started_at DESC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(serde_json::from_str(&row?)?);
        }
        Ok(sessions)
    }

    pub fn delete(&self, id: Uuid) -> Result<(), StorageError> {
        let fingerprint: Option<String> = self
            .connection
            .query_row(
                "SELECT setup_fingerprint FROM sessions WHERE id = ?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        self.connection
            .execute("DELETE FROM sessions WHERE id = ?1", [id.to_string()])?;
        if let Some(fingerprint) = fingerprint {
            self.connection.execute(
                "INSERT OR IGNORE INTO baselines(setup_fingerprint, session_id)
                 SELECT setup_fingerprint, id FROM sessions
                 WHERE setup_fingerprint = ?1
                   AND status = 'completed' AND high_confidence = 1
                 ORDER BY started_at ASC LIMIT 1",
                [fingerprint],
            )?;
        }
        Ok(())
    }

    pub fn baseline_for(
        &self,
        setup_fingerprint: &str,
    ) -> Result<Option<TestSession>, StorageError> {
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT sessions.data_json
                 FROM baselines
                 JOIN sessions ON sessions.id = baselines.session_id
                 WHERE baselines.setup_fingerprint = ?1",
                [setup_fingerprint],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|value| serde_json::from_str(&value).map_err(StorageError::from))
            .transpose()
    }

    pub fn set_baseline(&self, session: &TestSession) -> Result<(), StorageError> {
        if session.status != SessionStatus::Completed
            || !session
                .quality
                .as_ref()
                .is_some_and(|quality| quality.high_confidence)
        {
            return Err(StorageError::InvalidBaseline);
        }
        self.connection.execute(
            "INSERT INTO baselines(setup_fingerprint, session_id)
             VALUES (?1, ?2)
             ON CONFLICT(setup_fingerprint) DO UPDATE SET session_id = excluded.session_id",
            params![session.setup_fingerprint, session.id.to_string()],
        )?;
        Ok(())
    }

    pub fn preference(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM preferences WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_preference(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO preferences(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AudioSetup, PROTOCOL_VERSION, QualityAssessment};
    use tempfile::tempdir;

    fn setup() -> AudioSetup {
        AudioSetup {
            device_id: "airpods".into(),
            device_name: "AirPods".into(),
            backend: "ALSA".into(),
            sample_rate: 48_000,
            channels: 2,
            sink_volume_percent: Some(50.0),
            listening_mode: "Noise cancellation".into(),
            ceiling_dbfs: crate::domain::CEILING_DBFS,
            protocol_version: PROTOCOL_VERSION,
        }
    }

    #[test]
    fn language_preference_survives_reopening_and_does_not_affect_baselines() {
        use crate::localization::Language;
        let directory = tempdir().unwrap();
        let path = directory.path().join("test.sqlite");
        let repository = SessionRepository::open(&path).unwrap();
        assert_eq!(
            Language::from_code(repository.preference("language").unwrap().as_deref()),
            Language::English
        );
        let mut session = TestSession::new(setup());
        session.status = SessionStatus::Completed;
        session.quality = Some(QualityAssessment {
            high_confidence: true,
            catch_trials: 6,
            false_positive_catches: 0,
            max_retest_difference_db: 0.0,
            reasons: vec![],
        });
        repository.save(&session).unwrap();
        let original = serde_json::to_string(&session).unwrap();
        repository
            .set_preference("language", Language::German.code())
            .unwrap();
        drop(repository);
        let repository = SessionRepository::open(&path).unwrap();
        assert_eq!(
            Language::from_code(repository.preference("language").unwrap().as_deref()),
            Language::German
        );
        let baseline = repository
            .baseline_for(&session.setup_fingerprint)
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::to_string(&baseline).unwrap(), original);
        assert_eq!(setup().fingerprint(), session.setup_fingerprint);
        repository
            .set_preference("language", "unsupported")
            .unwrap();
        assert_eq!(
            Language::from_code(repository.preference("language").unwrap().as_deref()),
            Language::English
        );
    }

    #[test]
    fn repository_reads_historical_and_new_quality_records() {
        use crate::domain::{QualityIssue, QualityReason};
        use crate::localization::Language;
        let directory = tempdir().unwrap();
        let repository = SessionRepository::open(directory.path().join("test.sqlite")).unwrap();
        let mut session = TestSession::new(setup());
        session.quality = Some(QualityAssessment {
            high_confidence: false,
            catch_trials: 2,
            false_positive_catches: 0,
            max_retest_difference_db: 0.0,
            reasons: vec![QualityReason::Legacy(
                "Too few silent checks were completed".into(),
            )],
        });
        repository.save(&session).unwrap();
        let old = repository.list().unwrap().remove(0);
        let reason = &old.quality.unwrap().reasons[0];
        assert_eq!(
            Language::German.quality_reason(reason),
            "Es wurden zu wenige stille Kontrollversuche abgeschlossen"
        );
        session.quality.as_mut().unwrap().reasons = vec![QualityIssue::TooFewSilentChecks.into()];
        repository.save(&session).unwrap();
        assert_eq!(
            repository
                .list()
                .unwrap()
                .remove(0)
                .quality
                .unwrap()
                .reasons,
            session.quality.unwrap().reasons
        );
    }

    #[test]
    fn sessions_round_trip_and_first_valid_becomes_baseline() {
        let directory = tempdir().unwrap();
        let repository = SessionRepository::open(directory.path().join("test.sqlite")).unwrap();
        let mut session = TestSession::new(setup());
        session.status = SessionStatus::Completed;
        session.quality = Some(QualityAssessment {
            high_confidence: true,
            catch_trials: 6,
            false_positive_catches: 0,
            max_retest_difference_db: 0.0,
            reasons: vec![],
        });
        repository.save(&session).unwrap();
        assert_eq!(repository.list().unwrap().len(), 1);
        assert_eq!(
            repository
                .baseline_for(&session.setup_fingerprint)
                .unwrap()
                .unwrap()
                .id,
            session.id
        );
        let mut replacement = TestSession::new(setup());
        replacement.status = SessionStatus::Completed;
        replacement.quality = session.quality.clone();
        repository.save(&replacement).unwrap();
        repository.delete(session.id).unwrap();
        assert_eq!(repository.list().unwrap().len(), 1);
        assert_eq!(
            repository
                .baseline_for(&replacement.setup_fingerprint)
                .unwrap()
                .unwrap()
                .id,
            replacement.id
        );
    }
}
