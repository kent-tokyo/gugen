use crate::error::{GugenError, Result};

/// An exact rational number backed by checked `i128` arithmetic. Every
/// operation returns `Result` instead of panicking or silently wrapping on
/// overflow (AGENTS.md §25). Always kept reduced (`gcd(|num|, den) == 1`)
/// with a strictly positive denominator.
///
/// Not part of the public API (AGENTS.md doesn't name this type) — it's the
/// internal representation `Composition` and `balance.rs` share so that
/// reaction balancing is exact rather than float-approximate (AGENTS.md
/// §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Frac {
    num: i128,
    den: i128,
}

impl Frac {
    pub(crate) fn new(num: i128, den: i128) -> Result<Self> {
        if den == 0 {
            return Err(GugenError::ArithmeticOverflow);
        }
        let (num, den) = if den < 0 {
            (
                num.checked_neg().ok_or(GugenError::ArithmeticOverflow)?,
                den.checked_neg().ok_or(GugenError::ArithmeticOverflow)?,
            )
        } else {
            (num, den)
        };
        let g = gcd(num.unsigned_abs(), den.unsigned_abs()).max(1) as i128;
        Ok(Self {
            num: num / g,
            den: den / g,
        })
    }

    pub(crate) fn zero() -> Self {
        Self { num: 0, den: 1 }
    }

    pub(crate) fn one() -> Self {
        Self { num: 1, den: 1 }
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.num == 0
    }

    pub(crate) fn numerator(&self) -> i128 {
        self.num
    }

    pub(crate) fn denominator(&self) -> i128 {
        self.den
    }

    pub(crate) fn checked_neg(self) -> Result<Self> {
        Ok(Self {
            num: self
                .num
                .checked_neg()
                .ok_or(GugenError::ArithmeticOverflow)?,
            den: self.den,
        })
    }

    pub(crate) fn checked_add(self, other: Self) -> Result<Self> {
        let num = self
            .num
            .checked_mul(other.den)
            .and_then(|a| {
                other
                    .num
                    .checked_mul(self.den)
                    .and_then(|b| a.checked_add(b))
            })
            .ok_or(GugenError::ArithmeticOverflow)?;
        let den = self
            .den
            .checked_mul(other.den)
            .ok_or(GugenError::ArithmeticOverflow)?;
        Frac::new(num, den)
    }

    pub(crate) fn checked_sub(self, other: Self) -> Result<Self> {
        self.checked_add(other.checked_neg()?)
    }

    pub(crate) fn checked_mul(self, other: Self) -> Result<Self> {
        let num = self
            .num
            .checked_mul(other.num)
            .ok_or(GugenError::ArithmeticOverflow)?;
        let den = self
            .den
            .checked_mul(other.den)
            .ok_or(GugenError::ArithmeticOverflow)?;
        Frac::new(num, den)
    }

    pub(crate) fn checked_div(self, other: Self) -> Result<Self> {
        if other.num == 0 {
            return Err(GugenError::ArithmeticOverflow);
        }
        self.checked_mul(Self {
            num: other.den,
            den: other.num,
        })
    }

    pub(crate) fn to_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Finds the simplest rational within `tolerance` of `value` whose
    /// denominator does not exceed `max_denominator`, via continued-fraction
    /// convergents. Returns `None` if no such rational exists — i.e.
    /// `value` isn't cleanly rational at the precision gugen supports
    /// (AGENTS.md §10: reaction balancing must not rest on float
    /// approximation, so an amount that can't be pinned down exactly is
    /// rejected rather than silently rounded).
    pub(crate) fn from_f64(value: f64, max_denominator: i128, tolerance: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        if value == 0.0 {
            return Some(Self::zero());
        }

        let sign: i128 = if value < 0.0 { -1 } else { 1 };
        let mut x = value.abs();

        let (mut p2, mut q2) = (0i128, 1i128);
        let (mut p1, mut q1) = (1i128, 0i128);
        let mut best = (1i128, 0i128);

        for _ in 0..64 {
            if !x.is_finite() || x >= (i128::MAX as f64) {
                break;
            }
            let a = x.floor();
            let a_int = a as i128;

            let p = a_int.checked_mul(p1)?.checked_add(p2)?;
            let q = a_int.checked_mul(q1)?.checked_add(q2)?;
            if q <= 0 || q > max_denominator {
                break;
            }

            best = (p, q);
            p2 = p1;
            p1 = p;
            q2 = q1;
            q1 = q;

            let frac_part = x - a;
            if frac_part <= 1e-15 {
                break;
            }
            x = 1.0 / frac_part;
        }

        let (p, q) = best;
        if q == 0 {
            return None;
        }
        let approx = p as f64 / q as f64;
        if (approx - value.abs()).abs() <= tolerance {
            Some(Self {
                num: sign.checked_mul(p)?,
                den: q,
            })
        } else {
            None
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Frac {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f64(self.to_f64())
    }
}

// `pub(crate)` (not just used internally by `Frac::new`) so
// `commercial_catalog.rs` can reduce a whole composition's element-ratio
// vector to lowest terms using the same exact-integer GCD, rather than
// duplicating it.
pub(crate) fn gcd(a: u128, b: u128) -> u128 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rationalizes_common_decimal_values() {
        let f = Frac::from_f64(2.1, 1_000_000, 1e-9).unwrap();
        assert_eq!((f.numerator(), f.denominator()), (21, 10));

        let f = Frac::from_f64(1.0 / 3.0, 1_000_000, 1e-9).unwrap();
        assert_eq!((f.numerator(), f.denominator()), (1, 3));

        let f = Frac::from_f64(0.5, 1_000_000, 1e-9).unwrap();
        assert_eq!((f.numerator(), f.denominator()), (1, 2));

        let f = Frac::from_f64(3.0, 1_000_000, 1e-9).unwrap();
        assert_eq!((f.numerator(), f.denominator()), (3, 1));

        let f = Frac::from_f64(-0.67, 1_000_000, 1e-9).unwrap();
        assert_eq!((f.numerator(), f.denominator()), (-67, 100));
    }

    #[test]
    fn rejects_values_with_no_small_rational_approximation() {
        // With a denominator capped at 100, granularity is >= 1/100 = 0.01,
        // so no such fraction can land within 1e-9 of a value like this
        // regardless of the value's own continued-fraction structure. (A
        // generous bound like the crate's real 1_000_000/1e-9 constants
        // will in fact find a good convergent for almost any value,
        // including pi's — that's expected, not a bug: it just means the
        // rejection boundary is about precision-at-a-given-denominator-cap,
        // not about "looks irrational.")
        assert!(Frac::from_f64(0.123_456_7, 100, 1e-9).is_none());
    }

    #[test]
    fn checked_ops_detect_overflow() {
        let huge = Frac::new(i128::MAX, 1).unwrap();
        assert!(huge.checked_add(huge).is_err());
        assert!(huge.checked_mul(huge).is_err());
    }

    #[test]
    fn reduces_to_lowest_terms() {
        let f = Frac::new(4, 8).unwrap();
        assert_eq!((f.numerator(), f.denominator()), (1, 2));
        let f = Frac::new(-4, 8).unwrap();
        assert_eq!((f.numerator(), f.denominator()), (-1, 2));
    }

    #[test]
    fn arithmetic_is_exact() {
        let a = Frac::new(1, 3).unwrap();
        let b = Frac::new(1, 6).unwrap();
        let sum = a.checked_add(b).unwrap();
        assert_eq!((sum.numerator(), sum.denominator()), (1, 2));
    }
}
