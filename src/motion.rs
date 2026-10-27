//! How the overlay pill moves: a short ease-out entrance (fade in while rising
//! a few pixels into place), a hold, and an ease-in fade-out.
//!
//! Every frame is computed from the time since the phase started, not by
//! stepping a counter, so a late timer tick (Windows timers run at ~15.6 ms
//! and can slip) only skips a frame instead of slowing the whole animation.

/// How long the pill takes to appear.
pub const ENTER_MS: u32 = 180;
/// How long it takes to fade away.
pub const EXIT_MS: u32 = 200;
/// How far below its place it starts (96-DPI pixels).
pub const RISE_PX: f32 = 4.0;
/// How far to the right of its place a corner pill starts (96-DPI pixels).
pub const SLIDE_PX: f32 = 16.0;
/// How long a fade takes for someone who turned animation off in Windows:
/// opacity only, no movement.
pub const CALM_MS: u32 = 90;

/// Where in its life the pill is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Enter,
    Hold,
    Exit,
}

/// One frame: opacity (0–255), how far below its resting place the pill is
/// drawn and how far to the right of it (96-DPI pixels), `elapsed_ms` into
/// `phase`. `done` is true when the phase is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub alpha: u8,
    pub drop_px: f32,
    pub slide_px: f32,
    pub done: bool,
}

pub fn frame(phase: Phase, elapsed_ms: u32) -> Frame {
    frame_for(phase, elapsed_ms, false)
}

/// [`frame`], or with `calm` the version for reduced motion: only the opacity
/// changes, and quickly.
pub fn frame_for(phase: Phase, elapsed_ms: u32, calm: bool) -> Frame {
    match phase {
        Phase::Enter => {
            let t = progress(elapsed_ms, if calm { CALM_MS } else { ENTER_MS });
            let e = ease_out_cubic(t);
            let (rise, slide) = if calm {
                (0.0, 0.0)
            } else {
                (RISE_PX, SLIDE_PX)
            };
            Frame {
                alpha: (255.0 * e).round() as u8,
                drop_px: rise * (1.0 - e),
                slide_px: slide * (1.0 - e),
                done: t >= 1.0,
            }
        }
        Phase::Hold => Frame {
            alpha: 255,
            drop_px: 0.0,
            slide_px: 0.0,
            done: false,
        },
        Phase::Exit => {
            let t = progress(elapsed_ms, if calm { CALM_MS } else { EXIT_MS });
            Frame {
                alpha: (255.0 * (1.0 - ease_in_quad(t))).round() as u8,
                drop_px: 0.0,
                slide_px: 0.0,
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
    fn a_corner_pill_slides_in_from_the_right_and_settles() {
        let start = frame(Phase::Enter, 0);
        assert_eq!(start.slide_px, SLIDE_PX);
        let mut last = SLIDE_PX;
        for ms in 0..=ENTER_MS {
            let s = frame(Phase::Enter, ms).slide_px;
            assert!(s <= last, "never moves back out");
            last = s;
        }
        assert_eq!(last, 0.0);
        assert_eq!(frame(Phase::Hold, 0).slide_px, 0.0);
    }

    #[test]
    fn reduced_motion_only_fades_and_is_quick() {
        let start = frame_for(Phase::Enter, 0, true);
        assert_eq!((start.alpha, start.drop_px, start.slide_px), (0, 0.0, 0.0));
        let mid = frame_for(Phase::Enter, CALM_MS / 2, true);
        assert_eq!((mid.drop_px, mid.slide_px), (0.0, 0.0));
        assert!(frame_for(Phase::Enter, CALM_MS, true).done);
        assert!(frame_for(Phase::Exit, CALM_MS, true).done);
        const _: () = assert!(CALM_MS < ENTER_MS && CALM_MS < EXIT_MS);
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
