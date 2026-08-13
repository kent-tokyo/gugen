use crate::error::{GugenError, Result, require_finite};

/// Validated min/max condition ranges (AGENTS.md §6 "Conditions"): finite,
/// `min <= max`, and non-negative where physically required (duration,
/// pressure, ramp-rate magnitude — but not temperature, since a negative
/// Celsius value is physically ordinary, e.g. a cooling step).
///
/// `ProcessStep`, `Atmosphere`, and the rest of the solid-state process
/// template are Phase 4 work (AGENTS.md §26, docs/architecture.md); Phase 1
/// only needs these range primitives to exist.
macro_rules! validated_range {
    ($name:ident { $min:ident, $max:ident }, nonneg = $nonneg:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize))]
        pub struct $name {
            pub $min: f64,
            pub $max: f64,
        }

        impl $name {
            pub fn new($min: f64, $max: f64) -> Result<Self> {
                require_finite(stringify!($min), $min)?;
                require_finite(stringify!($max), $max)?;
                if $nonneg && ($min < 0.0 || $max < 0.0) {
                    return Err(GugenError::NegativeMagnitude {
                        field: stringify!($name),
                        value: $min.min($max),
                    });
                }
                if $min > $max {
                    return Err(GugenError::InvalidRange {
                        min: $min,
                        max: $max,
                    });
                }
                Ok(Self { $min, $max })
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                #[derive(serde::Deserialize)]
                struct Raw {
                    $min: f64,
                    $max: f64,
                }
                let raw = Raw::deserialize(deserializer)?;
                $name::new(raw.$min, raw.$max).map_err(serde::de::Error::custom)
            }
        }
    };
}

validated_range!(
    TemperatureRange {
        min_celsius,
        max_celsius
    },
    nonneg = false
);
validated_range!(
    DurationRange {
        min_hours,
        max_hours
    },
    nonneg = true
);
validated_range!(PressureRange { min_kpa, max_kpa }, nonneg = true);
validated_range!(
    RampRateRange {
        min_celsius_per_hour,
        max_celsius_per_hour
    },
    nonneg = true
);

/// Minimal Phase 1 placeholder for `ProcessEvidenceProvider` outputs.
/// Phase 4 extends this alongside the real `ProcessStep`/`Atmosphere` types.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProcessPrecedent {
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_inverted_and_non_finite_ranges() {
        assert!(TemperatureRange::new(900.0, 700.0).is_err());
        assert!(TemperatureRange::new(f64::NAN, 700.0).is_err());
        assert!(TemperatureRange::new(-10.0, 20.0).is_ok());
    }

    #[test]
    fn rejects_negative_duration() {
        assert!(DurationRange::new(-1.0, 2.0).is_err());
        assert!(DurationRange::new(1.0, 2.0).is_ok());
    }
}
