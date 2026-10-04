//! How the overlay pill moves: a short ease-out entrance (fade in while rising
//! a few pixels into place), a hold, and an ease-in fade-out.
//!
//! Every frame is computed from the time since the phase started, not by
//! stepping a counter, so a late timer tick (Windows timers run at ~15.6 ms
//! and can slip) only skips a frame instead of slowing the whole animation.

/// How long the pill takes to appear.
pub const ENTER_MS: u32 = 140;
/// How long it takes to fade away.
pub const EXIT_MS: u32 = 220;
/// How far below its place it starts (96-DPI pixels).
pub const RISE_PX: f32 = 4.0;

/// Where in its life the pill is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Enter,
    Hold,
    Exit,
}

/// One frame: opacity (0–255) and how far below its resting place the pill
/// is drawn (96-DPI pixels), `elapsed_ms` into `phase`. `done` is true when
/// the phase is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub alpha: u8,
    pub drop_px: f32,
    pub done: bool,
}

pub fn frame(phase: Phase, elapsed_ms: u32) -> Frame {
    match phase {
        Phase::Enter => {
            let t = progress(elapsed_ms, ENTER_MS);
            let e = ease_out_cubic(t);
            Frame {
                alpha: (255.0 * e).round() as u8,
                drop_px: RISE_PX * (1.0 - e),
                done: t >= 1.0,
            }
        }
        Phase::Hold => Frame {
            alpha: 255,
            drop_px: 0.0,
            done: false,
        },
        Phase::Exit => {
            let t = progress(elapsed_ms, EXIT_MS);
            Frame {
                alpha: (255.0 * (1.0 - ease_in_quad(t))).round() as u8,
                drop_px: 0.0,
                done: t >= 1.0,
            }
        }
    }
}

fn progress(elapsed_ms: u32, total_ms: u32) -> f32 {
    (elapsed_ms as f32 / total_ms as f32).clamp(0.0, 1.0)
}

/// How long a selection takes to slide to the next row.
pub const SLIDE_MS: u32 = 110;

/// How far (0 to 1) a slide that started `elapsed_ms` ago has gone,
/// easing out: quick at first, settling gently.
pub fn slide(elapsed_ms: u32) -> f32 {
    ease_out_cubic(progress(elapsed_ms, SLIDE_MS))
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_quad(t: f32) -> f32 {
    t * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_rises_into_place_and_fades_out() {
        let start = frame(Phase::Enter, 0);
        assert_eq!(
            (start.alpha, start.drop_px, start.done),
            (0, RISE_PX, false)
        );
        // Ease-out: most of the way there early on.
        assert!(frame(Phase::Enter, ENTER_MS / 2).alpha > 200);
        let end = frame(Phase::Enter, ENTER_MS);
        assert_eq!((end.alpha, end.drop_px, end.done), (255, 0.0, true));

        // Ease-in: barely fading at first, then quickly.
        assert!(frame(Phase::Exit, EXIT_MS / 4).alpha > 230);
        assert!(frame(Phase::Exit, EXIT_MS * 3 / 4).alpha < 130);
        let gone = frame(Phase::Exit, EXIT_MS + 50);
        assert_eq!((gone.alpha, gone.done), (0, true));
    }

    #[test]
    fn a_slide_settles() {
        assert_eq!(slide(0), 0.0);
        assert!(slide(SLIDE_MS / 3) > 0.5);
        assert_eq!(slide(SLIDE_MS), 1.0);
        assert_eq!(slide(SLIDE_MS * 4), 1.0);
    }

    #[test]
    fn opacity_never_goes_backwards() {
        let mut last = 0;
        for ms in 0..=ENTER_MS {
            let a = frame(Phase::Enter, ms).alpha;
            assert!(a >= last);
            last = a;
        }
        for ms in 0..=EXIT_MS {
            let a = frame(Phase::Exit, ms).alpha;
            assert!(a <= last);
            last = a;
        }
    }
}
