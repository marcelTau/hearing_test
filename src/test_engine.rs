use crate::domain::{
    CEILING_DBFS, Ear, FLOOR_DBFS, QualityAssessment, QualityIssue, START_DBFS, Threshold,
    ThresholdOutcome, Trial,
};
use std::collections::BTreeMap;

const MAX_PRESENTATIONS_PER_POINT: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Presentation {
    pub ear: Ear,
    pub frequency_hz: u32,
    pub dbfs: Option<f32>,
    pub is_catch: bool,
    pub ascending: bool,
}

#[derive(Debug, Clone, Copy)]
struct MeasurePoint {
    ear: Ear,
    frequency_hz: u32,
    is_retest: bool,
}

#[derive(Debug, Default, Clone)]
struct SearchState {
    level_dbfs: f32,
    ascending: bool,
    presentations: u32,
    ascending_attempts: BTreeMap<i32, u32>,
    ascending_hits: BTreeMap<i32, u32>,
}

impl SearchState {
    fn fresh() -> Self {
        Self {
            level_dbfs: START_DBFS,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestEngine {
    points: Vec<MeasurePoint>,
    point_index: usize,
    search: SearchState,
    completed: Vec<(MeasurePoint, Threshold)>,
    trials: Vec<Trial>,
    catch_trials: u32,
    false_positive_catches: u32,
    pending: Option<Presentation>,
}

impl TestEngine {
    pub fn new(first_ear: Ear) -> Self {
        let second_ear = match first_ear {
            Ear::Left => Ear::Right,
            Ear::Right => Ear::Left,
        };
        let mut points = Vec::new();
        for ear in [first_ear, second_ear] {
            for frequency_hz in [1_000, 2_000, 3_000, 4_000, 6_000, 8_000] {
                points.push(MeasurePoint {
                    ear,
                    frequency_hz,
                    is_retest: false,
                });
            }
            points.push(MeasurePoint {
                ear,
                frequency_hz: 1_000,
                is_retest: true,
            });
            for frequency_hz in [500, 250] {
                points.push(MeasurePoint {
                    ear,
                    frequency_hz,
                    is_retest: false,
                });
            }
        }
        Self {
            points,
            point_index: 0,
            search: SearchState::fresh(),
            completed: Vec::new(),
            trials: Vec::new(),
            catch_trials: 0,
            false_positive_catches: 0,
            pending: None,
        }
    }

    pub fn progress(&self) -> f32 {
        self.point_index as f32 / self.points.len() as f32
    }

    pub fn is_complete(&self) -> bool {
        self.point_index >= self.points.len()
    }

    pub fn next_presentation(&mut self, catch: bool) -> Option<Presentation> {
        if self.pending.is_some() || self.is_complete() {
            return None;
        }
        let point = self.points[self.point_index];
        let presentation = Presentation {
            ear: point.ear,
            frequency_hz: point.frequency_hz,
            dbfs: (!catch).then_some(self.search.level_dbfs),
            is_catch: catch,
            ascending: self.search.ascending,
        };
        self.pending = Some(presentation);
        Some(presentation)
    }

    pub fn record_response(
        &mut self,
        heard: bool,
        response_latency_ms: Option<u32>,
    ) -> Result<(), &'static str> {
        let presentation = self.pending.take().ok_or("no presentation is pending")?;
        self.trials.push(Trial {
            ordinal: self.trials.len() as u32 + 1,
            ear: presentation.ear,
            frequency_hz: presentation.frequency_hz,
            dbfs: presentation.dbfs,
            is_catch: presentation.is_catch,
            ascending: presentation.ascending,
            heard,
            response_latency_ms,
        });

        if presentation.is_catch {
            self.catch_trials += 1;
            self.false_positive_catches += u32::from(heard);
            return Ok(());
        }

        self.search.presentations += 1;
        if self.search.presentations >= MAX_PRESENTATIONS_PER_POINT {
            self.finish_point(ThresholdOutcome::NoResponse, None);
            return Ok(());
        }

        let key = self.search.level_dbfs.round() as i32;
        if self.search.ascending {
            *self.search.ascending_attempts.entry(key).or_default() += 1;
            if heard {
                *self.search.ascending_hits.entry(key).or_default() += 1;
            }
        }

        if heard {
            if self.search.level_dbfs <= FLOOR_DBFS {
                self.finish_point(ThresholdOutcome::BelowFloor, Some(FLOOR_DBFS));
                return Ok(());
            }

            if self.search.ascending {
                let attempts = self.search.ascending_attempts[&key];
                let hits = self.search.ascending_hits[&key];
                if hits >= 2 && hits * 2 >= attempts {
                    self.finish_point(ThresholdOutcome::Measured, Some(self.search.level_dbfs));
                    return Ok(());
                }
            }

            self.search.level_dbfs = (self.search.level_dbfs - 10.0).max(FLOOR_DBFS);
            self.search.ascending = false;
        } else {
            if self.search.level_dbfs >= CEILING_DBFS {
                self.finish_point(ThresholdOutcome::NoResponse, None);
                return Ok(());
            }
            self.search.level_dbfs = (self.search.level_dbfs + 5.0).min(CEILING_DBFS);
            self.search.ascending = true;
        }
        Ok(())
    }

    pub fn record_out_of_window_response(&mut self) {
        if self.is_complete() {
            return;
        }
        let point = self.points[self.point_index];
        self.catch_trials += 1;
        self.false_positive_catches += 1;
        self.trials.push(Trial {
            ordinal: self.trials.len() as u32 + 1,
            ear: point.ear,
            frequency_hz: point.frequency_hz,
            dbfs: None,
            is_catch: true,
            ascending: self.search.ascending,
            heard: true,
            response_latency_ms: None,
        });
    }

    pub fn cancel_pending(&mut self) {
        self.pending = None;
    }

    pub fn trials(&self) -> &[Trial] {
        &self.trials
    }

    fn finish_point(&mut self, outcome: ThresholdOutcome, dbfs: Option<f32>) {
        let point = self.points[self.point_index];
        self.completed.push((
            point,
            Threshold {
                ear: point.ear,
                frequency_hz: point.frequency_hz,
                dbfs,
                outcome,
            },
        ));
        self.point_index += 1;
        self.search = SearchState::fresh();
    }

    pub fn finish(self) -> Option<(Vec<Threshold>, Vec<Trial>, QualityAssessment)> {
        if !self.is_complete() {
            return None;
        }

        let mut thresholds = Vec::new();
        let mut max_retest_difference_db: f32 = 0.0;
        for ear in [Ear::Right, Ear::Left] {
            for frequency_hz in crate::domain::FREQUENCIES_HZ {
                let entries: Vec<_> = self
                    .completed
                    .iter()
                    .filter(|(point, _)| point.ear == ear && point.frequency_hz == frequency_hz)
                    .collect();
                if frequency_hz == 1_000 && entries.len() == 2 {
                    let primary = entries
                        .iter()
                        .find(|(point, _)| !point.is_retest)
                        .expect("1000 Hz primary exists");
                    let retest = entries
                        .iter()
                        .find(|(point, _)| point.is_retest)
                        .expect("1000 Hz retest exists");
                    if let (Some(a), Some(b)) = (primary.1.dbfs, retest.1.dbfs) {
                        max_retest_difference_db = max_retest_difference_db.max((a - b).abs());
                    }
                    let selected = entries
                        .iter()
                        .min_by(|a, b| {
                            a.1.dbfs
                                .unwrap_or(f32::INFINITY)
                                .total_cmp(&b.1.dbfs.unwrap_or(f32::INFINITY))
                        })
                        .expect("two entries exist");
                    thresholds.push(selected.1.clone());
                } else if let Some((_, threshold)) = entries.first() {
                    thresholds.push(threshold.clone());
                }
            }
        }

        let catch_rate = if self.catch_trials == 0 {
            0.0
        } else {
            self.false_positive_catches as f32 / self.catch_trials as f32
        };
        let mut reasons = Vec::new();
        if max_retest_difference_db > 5.0 {
            reasons.push(
                QualityIssue::RepeatDifference {
                    db: max_retest_difference_db,
                }
                .into(),
            );
        }
        if self.catch_trials < 4 {
            reasons.push(QualityIssue::TooFewSilentChecks.into());
        }
        if catch_rate > 0.2 {
            reasons.push(
                QualityIssue::SilentResponses {
                    percent: catch_rate * 100.0,
                }
                .into(),
            );
        }
        let quality = QualityAssessment {
            high_confidence: reasons.is_empty(),
            catch_trials: self.catch_trials,
            false_positive_catches: self.false_positive_catches,
            max_retest_difference_db,
            reasons,
        };
        Some((thresholds, self.trials, quality))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misses_walk_up_to_safe_ceiling_and_stop() {
        let mut engine = TestEngine::new(Ear::Right);
        for _ in 0..20 {
            let p = engine.next_presentation(false).unwrap();
            assert!(p.dbfs.unwrap() <= CEILING_DBFS);
            engine.record_response(false, None).unwrap();
            if engine.point_index > 0 {
                break;
            }
        }
        assert_eq!(engine.point_index, 1);
        assert_eq!(engine.completed[0].1.outcome, ThresholdOutcome::NoResponse);
    }

    #[test]
    fn catch_trial_does_not_change_search_level() {
        let mut engine = TestEngine::new(Ear::Left);
        let before = engine.search.level_dbfs;
        let p = engine.next_presentation(true).unwrap();
        assert!(p.dbfs.is_none());
        engine.record_response(true, Some(100)).unwrap();
        assert_eq!(engine.search.level_dbfs, before);
        assert_eq!(engine.false_positive_catches, 1);
    }

    #[test]
    fn requires_two_ascending_hits() {
        let mut engine = TestEngine::new(Ear::Right);
        // Initial response descends to -70, then miss ascends toward -60.
        engine.next_presentation(false);
        engine.record_response(true, None).unwrap();
        engine.next_presentation(false);
        engine.record_response(false, None).unwrap();
        engine.next_presentation(false);
        engine.record_response(false, None).unwrap();
        let p = engine.next_presentation(false).unwrap();
        assert_eq!(p.dbfs, Some(-60.0));
        engine.record_response(true, None).unwrap();
        assert_eq!(engine.point_index, 0);
        // A second ascending series confirms the same level.
        engine.next_presentation(false);
        engine.record_response(false, None).unwrap();
        engine.next_presentation(false);
        engine.record_response(false, None).unwrap();
        let p = engine.next_presentation(false).unwrap();
        assert_eq!(p.dbfs, Some(-60.0));
        engine.record_response(true, None).unwrap();
        assert_eq!(engine.point_index, 1);
        assert_eq!(engine.completed[0].1.dbfs, Some(-60.0));
    }

    #[test]
    fn test_has_eighteen_points() {
        let engine = TestEngine::new(Ear::Right);
        assert_eq!(engine.points.len(), 18);
        assert_eq!(engine.points.iter().filter(|p| p.is_retest).count(), 2);
    }
}
