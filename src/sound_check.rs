//! A shared-volume suitability check, separate from threshold measurement.
use crate::audio::ToneRequest;
use crate::domain::{CEILING_DBFS, Ear, FLOOR_DBFS};

const POINTS: [(Ear, u32, bool); 6] = [
    (Ear::Left, 1_000, true),
    (Ear::Right, 1_000, true),
    (Ear::Left, 1_000, false),
    (Ear::Right, 1_000, false),
    (Ear::Left, 4_000, false),
    (Ear::Right, 4_000, false),
];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CheckPhase {
    #[default]
    Idle,
    Playing(u64),
    AwaitingResponse,
}

#[derive(Debug, Clone, Copy)]
pub enum CheckResponse {
    Comfortable,
    NotHeard,
    Heard,
}

#[derive(Debug, Default)]
pub struct SoundCheck {
    index: usize,
    pub phase: CheckPhase,
    started: bool,
    volume: Option<f32>,
    pub floor_heard: bool,
    pub reference_unheard: bool,
    limits_accepted: bool,
}

pub fn volumes_match(expected: Option<f32>, current: Option<f32>) -> bool {
    match (expected, current) {
        (Some(a), Some(b)) => (a - b).abs() <= 0.5,
        (None, None) => true,
        _ => false,
    }
}

impl SoundCheck {
    pub fn point(&self) -> Option<(Ear, u32, bool)> {
        POINTS.get(self.index).copied()
    }

    pub fn answered_all(&self) -> bool {
        self.index == POINTS.len()
    }

    pub fn ready(&self) -> bool {
        self.answered_all() && (!self.floor_heard || self.limits_accepted)
    }

    pub fn accept_limits(&mut self) {
        if self.answered_all() && self.floor_heard {
            self.limits_accepted = true;
        }
    }

    /// Invalidate both ears' checks, including completed checks, on a change.
    pub fn observe_volume(&mut self, current: Option<f32>) -> bool {
        if self.started && !volumes_match(self.volume, current) {
            *self = Self::default();
            return true;
        }
        false
    }

    pub fn begin(&mut self, token: u64, volume: Option<f32>) -> Option<ToneRequest> {
        self.observe_volume(volume);
        if matches!(self.phase, CheckPhase::Playing(_)) {
            return None;
        }
        let (ear, frequency_hz, reference) = self.point()?;
        self.started = true;
        self.volume = volume;
        self.phase = CheckPhase::Playing(token);
        Some(ToneRequest {
            token,
            ear,
            frequency_hz,
            dbfs: Some(if reference { CEILING_DBFS } else { FLOOR_DBFS }),
        })
    }

    pub fn finished(&mut self, token: u64) {
        if self.phase == CheckPhase::Playing(token) {
            self.phase = CheckPhase::AwaitingResponse;
        }
    }

    pub fn answer(&mut self, response: CheckResponse) -> bool {
        if self.phase != CheckPhase::AwaitingResponse {
            return false;
        }
        let Some((_, _, reference)) = self.point() else {
            return false;
        };
        match (reference, response) {
            (true, CheckResponse::Comfortable) | (false, CheckResponse::NotHeard) => {}
            (true, CheckResponse::NotHeard) => self.reference_unheard = true,
            (false, CheckResponse::Heard) => self.floor_heard = true,
            _ => return false,
        }
        self.index += 1;
        self.phase = CheckPhase::Idle;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(check: &mut SoundCheck, volume: Option<f32>, floor_heard: bool) {
        while let Some((_, _, reference)) = check.point() {
            let request = check.begin(42, volume).unwrap();
            check.finished(request.token);
            assert!(check.answer(if reference {
                CheckResponse::Comfortable
            } else if floor_heard {
                CheckResponse::Heard
            } else {
                CheckResponse::NotHeard
            }));
        }
    }

    #[test]
    fn checks_both_ears_at_shared_volume_and_waits_for_playback() {
        let mut check = SoundCheck::default();
        assert!(!check.answer(CheckResponse::Comfortable));
        let request = check.begin(1, Some(25.0)).unwrap();
        assert_eq!(request.ear, Ear::Left);
        assert_eq!(request.dbfs, Some(CEILING_DBFS));
        assert!(!check.answer(CheckResponse::Comfortable));
        check.finished(2);
        assert!(!check.answer(CheckResponse::Comfortable));
        check.finished(1);
        assert!(check.answer(CheckResponse::Comfortable));
        assert_eq!(check.point(), Some((Ear::Right, 1_000, true)));
        complete(&mut check, Some(25.0), false);
        assert!(check.ready());
    }

    #[test]
    fn audible_floor_requires_explicit_acceptance_or_a_fresh_check() {
        let mut check = SoundCheck::default();
        complete(&mut check, Some(100.0), true);
        assert!(check.floor_heard);
        assert!(!check.ready());
        check.accept_limits();
        assert!(check.ready());
        assert!(check.observe_volume(Some(20.0)));
        assert!(!check.ready());
        complete(&mut check, Some(20.0), false);
        assert!(check.ready());
        assert!(!check.floor_heard);
    }

    #[test]
    fn changes_during_playback_and_loss_of_volume_reading_invalidate_checks() {
        let mut check = SoundCheck::default();
        check.begin(1, Some(50.0));
        assert!(check.observe_volume(Some(60.0)));
        check.finished(1);
        assert!(!check.answer(CheckResponse::Comfortable));
        complete(&mut check, Some(50.0), false);
        assert!(check.observe_volume(None));
        complete(&mut check, None, false);
        assert!(check.ready());
        assert!(check.observe_volume(Some(50.0)));
    }

    #[test]
    fn inaudible_reference_does_not_require_increasing_volume() {
        let mut check = SoundCheck::default();
        check.begin(1, Some(20.0));
        check.finished(1);
        assert!(check.answer(CheckResponse::NotHeard));
        complete(&mut check, Some(20.0), false);
        assert!(check.reference_unheard);
        assert!(check.ready());
    }
}
