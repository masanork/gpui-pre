// Added by ibis inc. (ibisOffice K0065, "every scroll stretches at its end"): the stretch past an end of a scroll area made by a finger's scroll, and
// its spring back, for every `Div` that scrolls (the physics are those of gpui-base's `ScrollBounce`, which wraps a scroll area from the outside).
// Not part of upstream gpui-pre; Apache-2.0.
//! The stretch of a scroll area past its end, for the scrolls fingers make (`ScrollWheelEvent::touch_fingers` not 0).
//!
//! * Pulled past an end by a finger, the area shows [`TRACKING`] of the pull at first and less and less, never as much as its own length (the rubber band:
//!   `x * d * c / (d + c * x)`, c = 0.55, d the length of the area).
//! * Let go past an end, it returns by a critically damped spring ([`RESPONSE_SECONDS`]), never crossing the end.
//! * A fling that arrives at an end is stopped by the stretch: what the fling has left is not scrolled on (it stretches once and returns).
//! * A wheel or a trackpad (`touch_fingers` 0) has none of it: the area is clamped at its end as before.

use crate::{Pixels, Point, Size, px};

/// The fraction of the finger's travel the stretched end follows at first (`UIScrollView`'s rubber band constant, 0.55).
pub(crate) const TRACKING: f32 = 0.55;
/// The time scale of the return, as `ScrollBounce` of gpui-base has it (not a published constant of UIKit).
pub(crate) const RESPONSE_SECONDS: f32 = 0.524;

/// The stretch along one axis.
#[derive(Default, Clone, Copy, Debug)]
pub(crate) struct AxisBounce {
    position: f32,
    velocity: f32,
    dragging: bool,
    suppress_momentum: bool,
    extent: f32,
}

impl AxisBounce {
    /// How far the area is displaced now (pixels; positive toward the start's side: past the top or the left end).
    pub(crate) fn offset(&self) -> f32 {
        if self.dragging {
            let d = self.extent.max(1.);
            self.position * TRACKING / (1. + TRACKING * self.position.abs() / d)
        } else {
            self.position
        }
    }

    pub(crate) fn begin(&mut self, extent: f32) {
        let offset = self.offset();
        // A displaced end keeps the extent it was stretched under.
        if offset == 0. {
            self.extent = extent.max(1.);
        }
        // Invert the rubber band so that grabbing a returning end is continuous.
        self.position = offset / (TRACKING * (1. - offset.abs() / self.extent).max(0.01));
        self.velocity = 0.;
        self.dragging = true;
        self.suppress_momentum = false;
    }

    /// The finger's displacement; returns the part that crosses back into the scroll area.
    pub(crate) fn pull(&mut self, delta: f32) -> f32 {
        let previous = self.position;
        let next = previous + delta;
        if previous != 0. && previous.signum() != next.signum() {
            self.position = 0.;
            next
        } else {
            self.position = next;
            0.
        }
    }

    pub(crate) fn release(&mut self) {
        self.position = self.offset();
        self.dragging = false;
        if self.position != 0. {
            self.suppress_momentum = true;
        }
    }

    pub(crate) fn is_stretched(&self) -> bool {
        self.position != 0.
    }

    pub(crate) fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub(crate) fn suppresses_momentum(&self) -> bool {
        self.suppress_momentum
    }

    /// The critically damped spring after `seconds`; whether it still moves.
    pub(crate) fn step(&mut self, seconds: f32) -> bool {
        if self.dragging || self.position == 0. {
            return false;
        }
        let omega = std::f32::consts::TAU / RESPONSE_SECONDS;
        let decay = (-omega * seconds).exp();
        let c = self.velocity + omega * self.position;
        self.position = (self.position + c * seconds) * decay;
        self.velocity = (self.velocity - omega * c * seconds) * decay;
        if self.position.abs() < 0.1 && self.velocity.abs() < 1. {
            self.position = 0.;
            self.velocity = 0.;
            false
        } else {
            true
        }
    }
}

/// The stretch of a scroll area: its two axes, and what it learns of its extent at each layout.
#[derive(Default, Debug)]
pub(crate) struct DivBounce {
    pub(crate) x: AxisBounce,
    pub(crate) y: AxisBounce,
    /// The farthest the area scrolls and its size (written as it is laid out).
    pub(crate) scroll_max: Point<Pixels>,
    pub(crate) size: Size<Pixels>,
    pub(crate) sampled_at: Option<scheduler::Instant>,
}

impl DivBounce {
    /// How far the content is displaced now.
    pub(crate) fn displacement(&self) -> Point<Pixels> {
        Point { x: px(self.x.offset()), y: px(self.y.offset()) }
    }
}

/// One axis of a finger's scroll: the scroll offset (zero at the start, `-max` at the far end) after a `delta` of the fingers, with what is stretched past an end kept in
/// `axis`. `scrolls` is whether the element scrolls along this axis.
#[allow(clippy::too_many_arguments)]
pub(crate) fn scroll_axis(axis: &mut AxisBounce, offset: f32, delta: f32, max: f32, extent: f32, started: bool, ended: bool, scrolls: bool) -> f32 {
    if !scrolls {
        return offset;
    }
    if started {
        axis.begin(extent);
    }
    // A fling that came to a stretched end goes on no further: the stretch returns by itself.
    if axis.suppresses_momentum() && !started {
        return offset;
    }
    let mut d = delta;
    if axis.is_stretched() {
        // The finger pulls on the stretched end: only what crosses back into the area scrolls it.
        d = axis.pull(d);
        if ended {
            axis.release();
        }
    }
    let new = (offset + d).clamp(-max.max(0.), 0.);
    let residual = d - (new - offset);
    let outward = (d > 0. && new == 0.) || (d < 0. && new == -max.max(0.));
    if residual.abs() > 0.01 && outward {
        let dragging = axis.is_dragging();
        if !dragging {
            axis.begin(extent);
        }
        axis.pull(residual);
        if !dragging || ended {
            axis.release();
        }
    } else if ended {
        axis.release();
    }
    new
}
