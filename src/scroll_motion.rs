// Added by ibis inc. (ibisOffice K0065): the scroll feel of UIScrollView (coasting, the pull past an end, the spring back) as pure
// functions, for every platform with a touch screen. Not part of upstream gpui-pre; Apache-2.0.
//! One-axis scrolling that moves as `UIScrollView` does: it follows the finger, stretches past an end with resistance, coasts on
//! release, and springs back (modified by ibis inc., ibisOffice K0065).
//!
//! Everything here is a pure function of the time handed in: no window, no clock, no OS. The numbers are Apple's published ones
//! (nothing was fitted by eye to a recording):
//!
//! * **Deceleration** is `UIScrollView.DecelerationRate.normal`, 0.998 per millisecond (the same as
//!   [`ScrollPhysics::ios`](crate::ScrollPhysics::ios)): `v(t) = v₀ · 0.998ᵗ` with `t` in milliseconds.
//! * **The pull past an end** is the rubber band `f(x) = x·d·c / (d + c·x)` with `c = 0.55` and `d` the length of the visible area along
//!   the axis (height, or width), `x` the distance the finger went past the end. (The formula is the one the ibisOffice request K0065 gave
//!   as Apple's; the page of Apple's own documentation it comes from is not yet checked.)
//! * **The return** is a critically damped spring, `y(t) = (y₀ + (w₀ + ω·y₀)·t)·e^(−ω·t)`, with `ω = 2π / 0.5 s`: the angular frequency of
//!   SwiftUI's `Spring.smooth` (duration 0.5 s, bounce 0, i.e. critical damping), the nearest value Apple publishes. The value
//!   `UIScrollView` itself uses is not published: compare with a device and report a difference rather than add a factor.

use std::time::Duration;

/// Apple's constant of the rubber band (`c` in the module docs).
pub const RUBBER_BAND_COEFFICIENT: f32 = 0.55;
/// `UIScrollView.DecelerationRate.normal`: the fraction of the speed that is left after each millisecond.
pub const DECELERATION_PER_MS: f32 = 0.998;
/// The return spring's angular frequency (radians per second): `2π / 0.5` (SwiftUI's `Spring.smooth`, critically damped).
pub const SPRING_OMEGA: f32 = std::f32::consts::TAU / 0.5;
/// Below this speed (pixels per second) a coast has stopped (the same as the touch recognizer's `MOMENTUM_STOP_VELOCITY`).
const STOP_SPEED: f32 = 10.;
/// Below this distance (pixels) from the end, at this speed or less, the spring has come to rest.
const REST_DISTANCE: f32 = 0.01;
const REST_SPEED: f32 = 0.5;

/// How far the content shows past the end when the finger has gone `x` past it: `f(x) = x·d·c / (d + c·x)`. It is 0 at 0, grows more slowly
/// the further the finger goes, and is under `d` for any `x`.
pub fn rubber_band(x: f32, extent: f32) -> f32 {
    if x <= 0. || extent <= 0. {
        return 0.;
    }
    x * extent * RUBBER_BAND_COEFFICIENT / (extent + RUBBER_BAND_COEFFICIENT * x)
}

/// The inverse of [`rubber_band`]: how far the finger went for the content to show `shown` past the end (`shown < extent`).
pub fn rubber_band_inverse(shown: f32, extent: f32) -> f32 {
    if shown <= 0. || extent <= 0. {
        return 0.;
    }
    let shown = shown.min(extent * 0.999_999);
    shown * extent / (RUBBER_BAND_COEFFICIENT * (extent - shown))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// Not moving, at `offset`.
    Idle,
    /// A finger holds it: `virtual_offset` is where the content would be if it did not resist (it may be past an end).
    Dragging { virtual_offset: f32 },
    /// Coasting from `from` at `speed` (pixels per second), `elapsed` seconds ago.
    Coasting { from: f32, speed: f32, elapsed: f32 },
    /// Returning to the end `edge` from `over` (signed: positive past the far end) with `speed` (pixels per second, the same sign
    /// convention), `elapsed` seconds ago.
    Returning { edge: f32, over: f32, speed: f32, elapsed: f32 },
}

/// The scroll position of one axis, with `UIScrollView`'s feel. The offset is how far the content is scrolled: `0` at the start, `max`
/// at the far end (more than `max`, or less than 0, only while stretched).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollMotion {
    offset: f32,
    /// The farthest the content scrolls (content length less the visible length; 0 if it fits).
    max: f32,
    /// The visible length along the axis (`d` of the rubber band).
    extent: f32,
    phase: Phase,
}

impl ScrollMotion {
    pub fn new(offset: f32, max: f32, extent: f32) -> Self {
        let max = max.max(0.);
        Self { offset: offset.clamp(0., max), max, extent, phase: Phase::Idle }
    }

    /// The position to draw.
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// The farthest scroll and the visible length changed (the content or the window did). A coast or a stretch goes on.
    pub fn set_range(&mut self, max: f32, extent: f32) {
        self.max = max.max(0.);
        self.extent = extent;
        if self.phase == Phase::Idle {
            self.offset = self.offset.clamp(0., self.max);
        }
    }

    /// The offset moved to `offset` by something else than the finger (a scrollbar, a key, "scroll into view"): everything stops.
    pub fn jump_to(&mut self, offset: f32) {
        self.offset = offset.clamp(0., self.max);
        self.phase = Phase::Idle;
    }

    /// A finger is moving, or still: it is moving, coasting, or returning. `tick` has more to do.
    pub fn is_moving(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    pub fn is_dragging(&self) -> bool {
        matches!(self.phase, Phase::Dragging { .. })
    }

    /// Past an end: the amount (signed; positive past the far end), or 0.
    pub fn stretch(&self) -> f32 {
        if self.offset < 0. {
            self.offset
        } else if self.offset > self.max {
            self.offset - self.max
        } else {
            0.
        }
    }

    /// The content follows the finger by `delta` (the change of the offset the finger asks for). Past an end it resists: what shows is
    /// [`rubber_band`] of how far the finger went.
    pub fn drag(&mut self, delta: f32) {
        let mut virtual_offset = match self.phase {
            Phase::Dragging { virtual_offset } => virtual_offset,
            // Caught while coasting or returning: the finger takes it where it shows, with the stretch it has (the finger's distance
            // for a stretch is the inverse of what shows).
            _ => self.virtual_of_shown(),
        };
        virtual_offset += delta;
        self.phase = Phase::Dragging { virtual_offset };
        self.offset = self.shown_of_virtual(virtual_offset);
    }

    fn shown_of_virtual(&self, virtual_offset: f32) -> f32 {
        if virtual_offset < 0. {
            -rubber_band(-virtual_offset, self.extent)
        } else if virtual_offset > self.max {
            self.max + rubber_band(virtual_offset - self.max, self.extent)
        } else {
            virtual_offset
        }
    }

    fn virtual_of_shown(&self) -> f32 {
        if self.offset < 0. {
            -rubber_band_inverse(-self.offset, self.extent)
        } else if self.offset > self.max {
            self.max + rubber_band_inverse(self.offset - self.max, self.extent)
        } else {
            self.offset
        }
    }

    /// The finger lifted at `speed` (pixels per second, the change of the offset per second). Past an end the content springs back with
    /// that speed; inside, it coasts and slows by [`DECELERATION_PER_MS`].
    pub fn release(&mut self, speed: f32) {
        if !matches!(self.phase, Phase::Dragging { .. }) {
            return;
        }
        let stretch = self.stretch();
        if stretch != 0. {
            let edge = if stretch < 0. { 0. } else { self.max };
            self.phase = Phase::Returning { edge, over: stretch, speed, elapsed: 0. };
        } else if speed.abs() > STOP_SPEED {
            self.phase = Phase::Coasting { from: self.offset, speed, elapsed: 0. };
        } else {
            self.phase = Phase::Idle;
        }
    }

    /// Time passed by `dt`. Returns whether it is still moving.
    pub fn tick(&mut self, dt: Duration) -> bool {
        let dt = dt.as_secs_f32();
        match self.phase {
            Phase::Idle | Phase::Dragging { .. } => {}
            Phase::Coasting { from, speed, elapsed } => {
                let elapsed = elapsed + dt;
                let (position, now_speed) = coast_at(from, speed, elapsed);
                if position < 0. || position > self.max {
                    // It reached an end: the spring takes it with the speed it had there.
                    let edge = if position < 0. { 0. } else { self.max };
                    let reached = coast_reaching(from, speed, edge, elapsed);
                    let (_, edge_speed) = coast_at(from, speed, reached);
                    self.phase = Phase::Returning { edge, over: 0., speed: edge_speed, elapsed: 0. };
                    return self.tick_returning(elapsed - reached);
                }
                if now_speed.abs() <= STOP_SPEED {
                    self.offset = position;
                    self.phase = Phase::Idle;
                } else {
                    self.offset = position;
                    self.phase = Phase::Coasting { from, speed, elapsed };
                }
            }
            Phase::Returning { .. } => {
                return self.tick_returning(dt);
            }
        }
        self.is_moving()
    }

    fn tick_returning(&mut self, dt: f32) -> bool {
        let Phase::Returning { edge, over, speed, elapsed } = self.phase else { return self.is_moving() };
        let elapsed = elapsed + dt;
        let (y, w) = spring_at(over, speed, elapsed);
        self.offset = edge + y;
        if y.abs() <= REST_DISTANCE && w.abs() <= REST_SPEED {
            self.offset = edge;
            self.phase = Phase::Idle;
        } else {
            self.phase = Phase::Returning { edge, over, speed, elapsed };
        }
        self.is_moving()
    }
}

/// `ln` of the fraction of the speed left after one second: `0.998` per millisecond is `0.998¹⁰⁰⁰` per second.
fn decay_ln_per_second() -> f32 {
    DECELERATION_PER_MS.ln() * 1000.
}

/// Where a coast from `from` at `speed` (pixels per second) is after `t` seconds, and its speed then: `x = x₀ + v₀·(rᵗ − 1)/ln r`.
fn coast_at(from: f32, speed: f32, t: f32) -> (f32, f32) {
    let k = decay_ln_per_second();
    let left = (k * t).exp();
    (from + speed * (left - 1.) / k, speed * left)
}

/// The time (seconds, within `0..=within`) a coast reaches `edge`: the distance is monotonic in time, so bisection finds it.
fn coast_reaching(from: f32, speed: f32, edge: f32, within: f32) -> f32 {
    let (mut low, mut high) = (0., within);
    for _ in 0..40 {
        let middle = (low + high) / 2.;
        let (position, _) = coast_at(from, speed, middle);
        let past = if speed > 0. { position >= edge } else { position <= edge };
        if past {
            high = middle;
        } else {
            low = middle;
        }
    }
    high
}

/// The critically damped spring after `t` seconds: the distance from the end and its speed, from distance `y0` at speed `w0`.
fn spring_at(y0: f32, w0: f32, t: f32) -> (f32, f32) {
    let decay = (-SPRING_OMEGA * t).exp();
    let b = w0 + SPRING_OMEGA * y0;
    ((y0 + b * t) * decay, (w0 - SPRING_OMEGA * b * t) * decay)
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: f32 = 800.;

    #[test]
    fn rubber_band_values() {
        assert_eq!(rubber_band(0., D), 0.);
        // x = d: d·d·c / (d + c·d) = d · c/(1 + c).
        let at_extent = rubber_band(D, D);
        assert!((at_extent - D * 0.55 / 1.55).abs() < 1e-3, "{at_extent}");
        // It never reaches d, and it grows slower and slower.
        assert!(rubber_band(1e6, D) < D);
        assert!(rubber_band(1e6, D) > 0.99 * D);
        let (a, b, c) = (rubber_band(100., D), rubber_band(200., D), rubber_band(300., D));
        assert!(b - a > c - b && c - b > 0.);
        // Near 0 it follows the finger by `c`.
        assert!((rubber_band(1., D) - 0.55).abs() < 1e-2);
    }

    #[test]
    fn rubber_band_inverse_undoes_it() {
        for x in [0.5, 10., 123., 800., 5000.] {
            let back = rubber_band_inverse(rubber_band(x, D), D);
            assert!((back - x).abs() / x < 1e-3, "{x} -> {back}");
        }
    }

    #[test]
    fn coast_slows_by_998_per_millisecond() {
        // After 1 ms the speed is 0.998 of it; the distance in the first second is v₀·(1 − 0.998¹⁰⁰⁰)/(−ln 0.998¹⁰⁰⁰).
        let (_, speed) = coast_at(0., 1000., 0.001);
        assert!((speed - 998.).abs() < 0.01, "{speed}");
        let (position, _) = coast_at(0., 1000., 1.);
        let k = 0.998f32.ln() * 1000.;
        let expected = 1000. * ((k).exp() - 1.) / k;
        assert!((position - expected).abs() < 0.01);
        // The total distance of a fling is v₀ / (−ln 0.998 per ms · 1000) = v₀ / 2.002 for v₀ = 1000: 499.5 px.
        let (far, _) = coast_at(0., 1000., 60.);
        assert!((far - 499.5).abs() < 0.5, "{far}");
    }

    #[test]
    fn coast_stops_inside_the_range() {
        let mut motion = ScrollMotion::new(0., 100_000., D);
        motion.drag(10.);
        motion.release(1000.);
        let mut ticks = 0;
        while motion.tick(Duration::from_millis(16)) {
            ticks += 1;
            assert!(ticks < 2000, "it stops");
        }
        // It stops when the speed is down to STOP_SPEED (10 px/s), which leaves 10 / 2.002 = 5 px of the 499.5 px unspent.
        assert!((motion.offset() - (10. + 499.5 - STOP_SPEED / 2.002)).abs() < 1., "{}", motion.offset());
    }

    #[test]
    fn drag_past_the_end_resists_and_returns_without_overshoot() {
        let mut motion = ScrollMotion::new(0., 1000., D);
        motion.drag(-200.); // 200 px past the start
        assert!((motion.offset() + rubber_band(200., D)).abs() < 1e-3);
        assert!(motion.stretch() < 0.);
        motion.release(0.);
        let mut last = motion.offset();
        let mut ticks = 0;
        while motion.tick(Duration::from_millis(8)) {
            // Critical damping: it only comes back, never crosses the end.
            assert!(motion.offset() >= last - 1e-4 && motion.offset() <= 1e-4, "{} then {}", last, motion.offset());
            last = motion.offset();
            ticks += 1;
            assert!(ticks < 2000);
        }
        assert_eq!(motion.offset(), 0.);
    }

    #[test]
    fn a_fling_into_the_end_springs_with_its_speed() {
        let mut motion = ScrollMotion::new(0., 100., D);
        motion.drag(50.);
        motion.release(3000.); // far more than the 50 px left
        let mut peak = 0f32;
        let mut ticks = 0;
        while motion.tick(Duration::from_millis(8)) {
            peak = peak.max(motion.offset());
            ticks += 1;
            assert!(ticks < 4000);
        }
        assert!(peak > 100., "it went past the end ({peak})");
        assert_eq!(motion.offset(), 100.);
    }

    #[test]
    fn a_finger_catches_a_coast() {
        let mut motion = ScrollMotion::new(0., 100_000., D);
        motion.drag(10.);
        motion.release(2000.);
        motion.tick(Duration::from_millis(100));
        let held = motion.offset();
        motion.drag(5.);
        assert!(motion.is_dragging());
        assert!((motion.offset() - (held + 5.)).abs() < 1e-3);
    }
}
