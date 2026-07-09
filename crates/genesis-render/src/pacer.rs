//! Warp pacing: how many whole ticks a frame runs.
//!
//! The logic half of landing step 4 (docs/research/render-bootstrap.md).
//! Time warp is "more ticks per wall second, never a bigger dt"
//! (constitution), and the wall clock never enters the simulation — it
//! only decides *how many whole ticks* this frame runs, which is a
//! presentation concern: same seed + actions ⇒ same states regardless of
//! how ticks were grouped into frames.
//!
//! The app shell calls [`WarpPacer::ticks_for`] once per frame with the
//! measured frame time and a tick budget (its per-frame wall allowance),
//! then ticks the owned `Simulation` exactly that many times. The pacer
//! also feeds the warp UI's honesty display: target vs achieved ticks/s.

/// Accumulates fractional tick debt across frames so any target rate is
/// honored on average by whole ticks per frame.
#[derive(Debug, Clone)]
pub struct WarpPacer {
    /// Target simulation rate in ticks per wall second. 0 = paused.
    target_rate: f64,
    /// Fractional ticks owed but not yet run.
    carry: f64,
}

/// What one frame should do, plus what the honesty display needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePlan {
    /// Whole ticks to run this frame.
    pub ticks: u32,
    /// True when the budget clamped the plan — the sim is falling behind
    /// the warp target and the UI should say so.
    pub starved: bool,
}

impl WarpPacer {
    pub fn new(target_rate: f64) -> Self {
        WarpPacer {
            target_rate: target_rate.max(0.0),
            carry: 0.0,
        }
    }

    /// Target rate in ticks per wall second (the UI's "target").
    pub fn target_rate(&self) -> f64 {
        self.target_rate
    }

    /// Change the warp preset. Dropping accumulated debt on a rate change
    /// keeps a long-starved run from bursting when the player slows down.
    pub fn set_rate(&mut self, ticks_per_second: f64) {
        self.target_rate = ticks_per_second.max(0.0);
        self.carry = 0.0;
    }

    /// Plan the next frame: `frame_dt` is the measured wall seconds since
    /// the previous frame; `budget_ticks` caps how many ticks the frame
    /// may run (the shell derives it from its wall allowance). Paused
    /// (rate 0) plans zero ticks and accumulates nothing.
    pub fn ticks_for(&mut self, frame_dt: f64, budget_ticks: u32) -> FramePlan {
        if self.target_rate <= 0.0 {
            return FramePlan {
                ticks: 0,
                starved: false,
            };
        }
        // Hostile inputs (NaN dt from a paused debugger, negative dt from
        // a clock hiccup) must not poison the carry.
        let dt = if frame_dt.is_finite() && frame_dt > 0.0 {
            frame_dt
        } else {
            0.0
        };
        let owed = self.carry + self.target_rate * dt;
        let want = owed.floor();
        let ticks = (want.min(f64::from(budget_ticks))) as u32;
        let starved = want > f64::from(ticks);
        // Debt beyond the budget is dropped, not banked: warp promises a
        // rate while the machine keeps up, never a burst to catch up
        // (bursting would freeze the frame loop it was meant to protect).
        self.carry = if starved { 0.0 } else { owed - want };
        FramePlan { ticks, starved }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_runs_nothing_and_banks_nothing() {
        let mut p = WarpPacer::new(0.0);
        for _ in 0..10 {
            assert_eq!(
                p.ticks_for(0.5, 100),
                FramePlan {
                    ticks: 0,
                    starved: false
                }
            );
        }
        // Unpausing after a long pause must not burst.
        p.set_rate(60.0);
        let plan = p.ticks_for(1.0 / 60.0, 100);
        assert!(plan.ticks <= 1, "no backlog from the paused span");
    }

    #[test]
    fn one_x_at_sixty_fps_runs_one_tick_per_frame() {
        // 60 ticks/s at a steady 60 fps: exactly one tick per frame, no
        // drift — carry stays bounded below one tick.
        let mut p = WarpPacer::new(60.0);
        let mut total = 0;
        for _ in 0..600 {
            let plan = p.ticks_for(1.0 / 60.0, 10);
            assert!(!plan.starved);
            total += plan.ticks;
        }
        assert!(
            (599..=601).contains(&total),
            "600 frames -> ~600 ticks, got {total}"
        );
    }

    #[test]
    fn fractional_rates_accumulate_to_whole_ticks() {
        // 15 ticks/s at 60 fps = 0.25 ticks per frame: one tick every
        // fourth frame, never fractional.
        let mut p = WarpPacer::new(15.0);
        let ticks: Vec<u32> = (0..8).map(|_| p.ticks_for(1.0 / 60.0, 10).ticks).collect();
        assert_eq!(ticks.iter().sum::<u32>(), 2, "8 frames at 0.25/frame");
        assert!(ticks.iter().all(|&t| t <= 1));
    }

    #[test]
    fn warp_respects_the_budget_and_reports_starvation() {
        // 1000 ticks/s at 10 fps wants 100 ticks/frame; a budget of 30
        // clamps it and flags the honesty display.
        let mut p = WarpPacer::new(1000.0);
        let plan = p.ticks_for(0.1, 30);
        assert_eq!(plan.ticks, 30);
        assert!(plan.starved, "clamped frame must report starvation");
        // Dropped debt is not banked: the next healthy frame plans its
        // own share, not a catch-up burst.
        let next = p.ticks_for(0.01, 30);
        assert!(
            next.ticks <= 10,
            "no burst after starvation, got {}",
            next.ticks
        );
    }

    #[test]
    fn hostile_frame_times_do_not_poison_the_carry() {
        let mut p = WarpPacer::new(60.0);
        for bad in [f64::NAN, f64::INFINITY, -1.0] {
            let plan = p.ticks_for(bad, 10);
            assert_eq!(plan.ticks, 0, "bad dt {bad} must plan zero ticks");
        }
        // Still healthy afterwards.
        let plan = p.ticks_for(1.0, 100);
        assert_eq!(plan.ticks, 60);
    }

    #[test]
    fn rate_change_drops_debt() {
        let mut p = WarpPacer::new(600.0);
        p.ticks_for(1.0, 5); // heavily starved
        p.set_rate(60.0);
        let plan = p.ticks_for(1.0 / 60.0, 100);
        assert!(
            plan.ticks <= 1,
            "slowing down must not burst, got {}",
            plan.ticks
        );
    }
}
