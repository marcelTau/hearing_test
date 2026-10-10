use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

pub const FREQUENCIES_HZ: [u32; 8] = [250, 500, 1_000, 2_000, 3_000, 4_000, 6_000, 8_000];
pub const FLOOR_DBFS: f32 = -90.0;
pub const CEILING_DBFS: f32 = -30.0;
pub const START_DBFS: f32 = -60.0;
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ear {
    Left,
    Right,
}

impl Ear {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }

    pub const fn channel(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }
}

impl fmt::Display for Ear {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSetup {
    pub device_id: String,
    pub device_name: String,
    pub backend: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub sink_volume_percent: Option<f32>,
    pub listening_mode: String,
    pub ceiling_dbfs: f32,
    pub protocol_version: u32,
}

impl AudioSetup {
    pub fn fingerprint(&self) -> String {
        // Keep the source fields in the database as well: the readable key makes
        // incompatibilities explainable instead of hiding them behind a hash.
        format!(
            "{}|{}|{}|{}|{}|{:.1}|{}|{:.1}|v{}",
            self.device_id,
            self.device_name,
            self.backend,
            self.sample_rate,
            self.channels,
            self.sink_volume_percent.unwrap_or(-1.0),
            self.listening_mode,
            self.ceiling_dbfs,
            self.protocol_version
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThresholdOutcome {
    Measured,
    BelowFloor,
    NoResponse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Threshold {
    pub ear: Ear,
    pub frequency_hz: u32,
    pub dbfs: Option<f32>,
    pub outcome: ThresholdOutcome,
}

impl Threshold {
    pub fn relative_db(&self) -> Option<f32> {
        self.dbfs.map(relative_db)
    }
}

pub fn relative_db(dbfs: f32) -> f32 {
    dbfs - FLOOR_DBFS
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trial {
    pub ordinal: u32,
    pub ear: Ear,
    pub frequency_hz: u32,
    pub dbfs: Option<f32>,
    pub is_catch: bool,
    pub ascending: bool,
    pub heard: bool,
    pub response_latency_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    InProgress,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityAssessment {
    pub high_confidence: bool,
    pub catch_trials: u32,
    pub false_positive_catches: u32,
    pub max_retest_difference_db: f32,
    pub reasons: Vec<QualityReason>,
}

/// Legacy strings remain readable without rewriting historical session JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QualityReason {
    Issue(QualityIssue),
    Legacy(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum QualityIssue {
    RepeatDifference { db: f32 },
    TooFewSilentChecks,
    SilentResponses { percent: f32 },
    VolumeUnavailable,
}

impl From<QualityIssue> for QualityReason {
    fn from(issue: QualityIssue) -> Self {
        Self::Issue(issue)
    }
}

impl QualityIssue {
    pub fn from_legacy(value: &str) -> Option<Self> {
        match value {
            "Too few silent checks were completed" => Some(Self::TooFewSilentChecks),
            "Linux output volume was unavailable, so setup matching cannot be verified" => {
                Some(Self::VolumeUnavailable)
            }
            _ => {
                if let Some(db) = value
                    .strip_prefix("The 1000 Hz repeat differed by ")
                    .and_then(|value| value.strip_suffix(" dB"))
                    .and_then(|value| value.parse().ok())
                {
                    Some(Self::RepeatDifference { db })
                } else {
                    value
                        .strip_prefix("Responses occurred on ")
                        .and_then(|value| value.strip_suffix("% of silent checks"))
                        .and_then(|value| value.parse().ok())
                        .map(|percent| Self::SilentResponses { percent })
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSession {
    pub id: Uuid,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
    pub status: SessionStatus,
    pub setup: AudioSetup,
    pub setup_fingerprint: String,
    pub thresholds: Vec<Threshold>,
    pub trials: Vec<Trial>,
    pub quality: Option<QualityAssessment>,
}

impl TestSession {
    pub fn new(setup: AudioSetup) -> Self {
        let setup_fingerprint = setup.fingerprint();
        Self {
            id: Uuid::now_v7(),
            started_at: OffsetDateTime::now_utc(),
            completed_at: None,
            status: SessionStatus::InProgress,
            setup,
            setup_fingerprint,
            thresholds: Vec::new(),
            trials: Vec::new(),
            quality: None,
        }
    }

    pub fn is_comparable_with(&self, other: &Self) -> bool {
        self.status == SessionStatus::Completed
            && other.status == SessionStatus::Completed
            && self.quality.as_ref().is_some_and(|q| q.high_confidence)
            && other.quality.as_ref().is_some_and(|q| q.high_confidence)
            && self.setup_fingerprint == other.setup_fingerprint
    }

    pub fn threshold(&self, ear: Ear, frequency_hz: u32) -> Option<&Threshold> {
        self.thresholds
            .iter()
            .find(|t| t.ear == ear && t.frequency_hz == frequency_hz)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThresholdDelta {
    pub ear: Ear,
    pub frequency_hz: u32,
    pub current_relative_db: Option<f32>,
    pub baseline_relative_db: Option<f32>,
    pub change_db: Option<f32>,
}

pub fn compare_sessions(
    current: &TestSession,
    baseline: &TestSession,
) -> Option<Vec<ThresholdDelta>> {
    if !current.is_comparable_with(baseline) {
        return None;
    }
    Some(
        [Ear::Right, Ear::Left]
            .into_iter()
            .flat_map(|ear| {
                FREQUENCIES_HZ.into_iter().map(move |frequency_hz| {
                    let current_relative_db = current
                        .threshold(ear, frequency_hz)
                        .and_then(Threshold::relative_db);
                    let baseline_relative_db = baseline
                        .threshold(ear, frequency_hz)
                        .and_then(Threshold::relative_db);
                    ThresholdDelta {
                        ear,
                        frequency_hz,
                        current_relative_db,
                        baseline_relative_db,
                        change_db: current_relative_db
                            .zip(baseline_relative_db)
                            .map(|(current, baseline)| current - baseline),
                    }
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_structured_quality_reasons_are_compatible() {
        use crate::localization::Language;
        let legacy = r#"[
            "The 1000 Hz repeat differed by 10 dB",
            "Too few silent checks were completed",
            "Responses occurred on 25% of silent checks",
            "Linux output volume was unavailable, so setup matching cannot be verified",
            "Unrecognized historical note"
        ]"#;
        let reasons: Vec<QualityReason> = serde_json::from_str(legacy).unwrap();
        let expected = [
            QualityIssue::RepeatDifference { db: 10.0 },
            QualityIssue::TooFewSilentChecks,
            QualityIssue::SilentResponses { percent: 25.0 },
            QualityIssue::VolumeUnavailable,
        ];
        for (reason, issue) in reasons.iter().zip(expected) {
            let structured = QualityReason::from(issue);
            for language in [Language::English, Language::German] {
                assert_eq!(
                    language.quality_reason(reason),
                    language.quality_reason(&structured)
                );
            }
            let json = serde_json::to_string(&structured).unwrap();
            assert!(json.contains("code"));
            assert_eq!(
                serde_json::from_str::<QualityReason>(&json).unwrap(),
                structured
            );
        }
        assert_eq!(
            Language::German.quality_reason(&reasons[4]),
            "Unrecognized historical note"
        );
        assert_eq!(
            serde_json::from_str::<Vec<QualityReason>>(&serde_json::to_string(&reasons).unwrap())
                .unwrap(),
            reasons
        );
    }

    fn completed_session(device: &str, dbfs: f32) -> TestSession {
        let setup = AudioSetup {
            device_id: device.into(),
            device_name: "AirPods".into(),
            backend: "ALSA".into(),
            sample_rate: 48_000,
            channels: 2,
            sink_volume_percent: Some(50.0),
            listening_mode: "Noise cancellation".into(),
            ceiling_dbfs: CEILING_DBFS,
            protocol_version: PROTOCOL_VERSION,
        };
        let mut session = TestSession::new(setup);
        session.status = SessionStatus::Completed;
        session.quality = Some(QualityAssessment {
            high_confidence: true,
            catch_trials: 6,
            false_positive_catches: 0,
            max_retest_difference_db: 0.0,
            reasons: vec![],
        });
        session.thresholds.push(Threshold {
            ear: Ear::Right,
            frequency_hz: 1_000,
            dbfs: Some(dbfs),
            outcome: ThresholdOutcome::Measured,
        });
        session
    }

    #[test]
    fn comparison_requires_matching_setup_and_reports_signed_change() {
        let baseline = completed_session("same", -60.0);
        let current = completed_session("same", -50.0);
        let deltas = compare_sessions(&current, &baseline).unwrap();
        assert_eq!(
            deltas
                .iter()
                .find(|delta| delta.ear == Ear::Right && delta.frequency_hz == 1_000)
                .unwrap()
                .change_db,
            Some(10.0)
        );

        let incompatible = completed_session("different", -50.0);
        assert!(compare_sessions(&incompatible, &baseline).is_none());
    }
}
