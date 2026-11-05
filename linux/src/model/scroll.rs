//! User multipliers for the Naga's wheel and tilt. Linux wheels report both
//! detents (`REL_WHEEL`) and high-resolution units (`REL_WHEEL_HI_RES`, 120
//! per detent). Scaling works on high-resolution units with a fractional
//! remainder, and detents are derived from what was emitted, so 0.3x still
//! scrolls one line every few ticks and smooth-scrolling apps stay exact.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScrollSensitivity {
    pub vertical: f64,
    pub horizontal: f64,
}

impl Default for ScrollSensitivity {
    fn default() -> Self {
        ScrollSensitivity::BASELINE
    }
}

impl ScrollSensitivity {
    pub const BASELINE: ScrollSensitivity = ScrollSensitivity { vertical: 1.0, horizontal: 1.0 };
    pub const MIN: f64 = 0.1;
    pub const MAX: f64 = 5.0;

    pub fn clamped(self) -> Self {
        ScrollSensitivity { vertical: clamp(self.vertical), horizontal: clamp(self.horizontal) }
    }
}

fn clamp(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(ScrollSensitivity::MIN, ScrollSensitivity::MAX)
    } else {
        1.0
    }
}

/// Port of the macOS `ScrollScaler.emit`: truncation toward zero with an
/// exact remainder, so the emitted average converges on the target rate.
pub fn emit(delta: i64, factor: f64, residual: &mut f64) -> i64 {
    if delta == 0 {
        return 0;
    }
    if factor == 1.0 {
        *residual = 0.0;
        return delta;
    }
    // A reverse tick must not spend a remainder collected in the other direction.
    if *residual != 0.0 && (delta > 0) != (*residual > 0.0) {
        *residual = 0.0;
    }
    let total = *residual + delta as f64 * factor;
    if total >= i64::MAX as f64 {
        *residual = 0.0;
        return i64::MAX;
    }
    if total <= i64::MIN as f64 {
        *residual = 0.0;
        return i64::MIN;
    }
    let nearest = total.round();
    if (total - nearest).abs() < 1e-9 {
        *residual = 0.0;
        return nearest as i64;
    }
    let emitted = total.trunc();
    *residual = total - emitted;
    emitted as i64
}

pub const HI_RES_PER_DETENT: i32 = 120;

/// One axis: scaled high-resolution units in, `(hi_res, detents)` out.
#[derive(Clone, Debug, Default)]
pub struct AxisScaler {
    residual: f64,
    detent_accumulator: i32,
    factor: Option<f64>,
}

impl AxisScaler {
    pub fn reset(&mut self) {
        *self = AxisScaler::default();
    }

    pub fn apply(&mut self, hi_res: i32, factor: f64) -> (i32, i32) {
        if self.factor != Some(factor) {
            self.residual = 0.0;
            self.detent_accumulator = 0;
            self.factor = Some(factor);
        }
        if hi_res == 0 {
            return (0, 0);
        }
        let scaled = emit(hi_res as i64, factor, &mut self.residual).clamp(i32::MIN as i64 / 2, i32::MAX as i64 / 2) as i32;
        if scaled == 0 {
            return (0, 0);
        }
        if self.detent_accumulator != 0 && (self.detent_accumulator > 0) != (scaled > 0) {
            self.detent_accumulator = 0;
        }
        self.detent_accumulator += scaled;
        let detents = self.detent_accumulator / HI_RES_PER_DETENT;
        self.detent_accumulator -= detents * HI_RES_PER_DETENT;
        (scaled, detents)
    }
}

#[derive(Clone, Debug, Default)]
pub struct WheelScaler {
    pub vertical: AxisScaler,
    pub horizontal: AxisScaler,
}

impl WheelScaler {
    pub fn reset(&mut self) {
        self.vertical.reset();
        self.horizontal.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_is_exact() {
        let mut axis = AxisScaler::default();
        for _ in 0..10 {
            assert_eq!(axis.apply(120, 1.0), (120, 1));
            assert_eq!(axis.apply(-120, 1.0), (-120, -1));
        }
    }

    #[test]
    fn slow_factor_emits_one_detent_every_few_ticks() {
        let mut axis = AxisScaler::default();
        let mut detents = 0;
        let mut hi_res = 0;
        for _ in 0..10 {
            let (h, d) = axis.apply(120, 0.3);
            hi_res += h;
            detents += d;
        }
        assert_eq!(hi_res, 360);
        assert_eq!(detents, 3);
    }

    #[test]
    fn fast_factor_multiplies() {
        let mut axis = AxisScaler::default();
        assert_eq!(axis.apply(120, 2.5), (300, 2));
        assert_eq!(axis.apply(120, 2.5), (300, 3));
    }

    #[test]
    fn reversing_drops_the_remainder() {
        let mut axis = AxisScaler::default();
        assert_eq!(axis.apply(120, 0.5), (60, 0));
        assert_eq!(axis.apply(-120, 0.5), (-60, 0));
        assert_eq!(axis.apply(-120, 0.5), (-60, -1));
    }

    #[test]
    fn emit_matches_macos_behaviour() {
        let mut residual = 0.0;
        let total: i64 = (0..1000).map(|_| emit(1, 0.7, &mut residual)).sum();
        assert_eq!(total, 700);
        let mut residual = 0.0;
        assert_eq!(emit(3, 1.0 / 3.0, &mut residual), 1);
        assert_eq!(residual, 0.0, "near-integer totals snap");
        assert_eq!(emit(0, 2.0, &mut residual), 0);
    }

    #[test]
    fn clamping() {
        let s = ScrollSensitivity { vertical: 0.0, horizontal: f64::NAN }.clamped();
        assert_eq!(s, ScrollSensitivity { vertical: 0.1, horizontal: 1.0 });
        assert_eq!(ScrollSensitivity { vertical: 9.0, horizontal: 2.0 }.clamped().vertical, 5.0);
    }
}
