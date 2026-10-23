use std::{cmp::Ordering, fmt, num::IntErrorKind};

/// A runtime number. Integer and floating-point values remain distinct for `eq`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.contains(['.', 'e', 'E']) {
            let value = text
                .parse::<f64>()
                .map_err(|_| format!("invalid number {text:?}"))?;
            Self::finite(value).map_err(str::to_owned)
        } else {
            text.parse::<i64>().map(Self::Int).map_err(|error| {
                if matches!(
                    error.kind(),
                    IntErrorKind::PosOverflow | IntErrorKind::NegOverflow
                ) {
                    format!("integer out of range for i64: {text:?}")
                } else {
                    format!("invalid number {text:?}")
                }
            })
        }
    }

    fn finite(value: f64) -> Result<Self, &'static str> {
        if value.is_finite() {
            Ok(Self::Float(value))
        } else {
            Err("number must be finite")
        }
    }

    pub(crate) fn as_float(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    pub(crate) fn negate(self) -> Result<Self, &'static str> {
        match self {
            Self::Int(value) => value.checked_neg().map(Self::Int).ok_or("integer overflow"),
            Self::Float(value) => Self::finite(-value),
        }
    }

    pub(crate) fn calculate(self, operator: &str, other: Self) -> Result<Self, &'static str> {
        if operator == "/" {
            let divisor = other.as_float();
            if divisor == 0.0 {
                return Err("division by zero");
            }
            return Self::finite(self.as_float() / divisor);
        }
        match (self, other) {
            (Self::Int(left), Self::Int(right)) => {
                let value = match operator {
                    "+" => left.checked_add(right),
                    "-" => left.checked_sub(right),
                    "*" => left.checked_mul(right),
                    _ => unreachable!("unknown arithmetic operator"),
                };
                value.map(Self::Int).ok_or("integer overflow")
            }
            (left, right) => {
                let (left, right) = (left.as_float(), right.as_float());
                Self::finite(match operator {
                    "+" => left + right,
                    "-" => left - right,
                    "*" => left * right,
                    _ => unreachable!("unknown arithmetic operator"),
                })
            }
        }
    }

    pub(crate) fn compare(self, other: Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Int(left), Self::Int(right)) => Some(left.cmp(&right)),
            (Self::Float(left), Self::Float(right)) => left.partial_cmp(&right),
            (Self::Int(integer), Self::Float(float)) => Self::compare_int_float(integer, float),
            (Self::Float(float), Self::Int(integer)) => {
                Self::compare_int_float(integer, float).map(Ordering::reverse)
            }
        }
    }

    fn compare_int_float(integer: i64, float: f64) -> Option<Ordering> {
        if float.is_nan() {
            return None;
        }
        // The upper bound is exclusive: i64::MAX rounds up to 2^63 as f64.
        if float < i64::MIN as f64 {
            return Some(Ordering::Greater);
        }
        if float >= -(i64::MIN as f64) {
            return Some(Ordering::Less);
        }

        // Compare the integer part without rounding the original integer to f64.
        let truncated = float as i64;
        Some(match integer.cmp(&truncated) {
            Ordering::Equal if float.fract() == 0.0 => Ordering::Equal,
            Ordering::Equal if float.is_sign_negative() => Ordering::Greater,
            Ordering::Equal => Ordering::Less,
            ordering => ordering,
        })
    }

    pub(crate) fn equals(self, other: Self) -> bool {
        self.compare(other) == Some(Ordering::Equal)
    }
}

impl fmt::Display for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(value) => value.fmt(formatter),
            Self::Float(value) => {
                let mut text = value.to_string();
                if value.is_finite() && !text.contains(['.', 'e', 'E']) {
                    text.push_str(".0");
                }
                formatter.write_str(&text)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_equality_preserves_integer_precision_and_checks_float_bounds() {
        for (integer, float, equal) in [
            (0, -0.0, true),
            (1, 1.5, false),
            (9_007_199_254_740_992, 9_007_199_254_740_992.0, true),
            (9_007_199_254_740_993, 9_007_199_254_740_992.0, false),
            (-9_007_199_254_740_993, -9_007_199_254_740_992.0, false),
            (i64::MIN, i64::MIN as f64, true),
            (i64::MIN + 1, i64::MIN as f64, false),
            (i64::MAX, i64::MAX as f64, false),
            (i64::MAX - 1023, (i64::MAX - 1023) as f64, true),
            (i64::MIN, -18_446_744_073_709_551_616.0, false),
            (i64::MAX, f64::INFINITY, false),
            (0, f64::NAN, false),
        ] {
            let (integer, float) = (Number::Int(integer), Number::Float(float));
            assert_eq!(integer.equals(float), equal, "{integer} = {float}");
            assert_eq!(float.equals(integer), equal, "{float} = {integer}");
        }
    }

    #[test]
    fn mixed_ordering_preserves_integer_precision_and_float_fractions() {
        use Ordering::{Equal, Greater, Less};

        for (integer, float, ordering) in [
            (0, -0.0, Equal),
            (1, 1.5, Less),
            (-1, -1.5, Greater),
            (0, 0.5, Less),
            (0, -0.5, Greater),
            (2, 1.5, Greater),
            (-2, -1.5, Less),
            (9_007_199_254_740_993, 9_007_199_254_740_992.0, Greater),
            (-9_007_199_254_740_993, -9_007_199_254_740_992.0, Less),
            (i64::MIN, i64::MIN as f64, Equal),
            (i64::MIN + 1, i64::MIN as f64, Greater),
            (i64::MAX, i64::MAX as f64, Less),
            (i64::MAX - 1023, (i64::MAX - 1023) as f64, Equal),
            (i64::MIN, -18_446_744_073_709_551_616.0, Greater),
            (i64::MAX, f64::INFINITY, Less),
            (i64::MIN, f64::NEG_INFINITY, Greater),
            (0, f64::MIN_POSITIVE, Less),
            (0, -f64::MIN_POSITIVE, Greater),
        ] {
            let (integer, float) = (Number::Int(integer), Number::Float(float));
            assert_eq!(
                integer.compare(float),
                Some(ordering),
                "{integer} vs {float}"
            );
            assert_eq!(
                float.compare(integer),
                Some(ordering.reverse()),
                "{float} vs {integer}"
            );
            assert_eq!(integer.equals(float), ordering == Equal);
        }
        assert_eq!(Number::Int(0).compare(Number::Float(f64::NAN)), None);
        assert_eq!(Number::Float(f64::NAN).compare(Number::Int(0)), None);
    }

    #[test]
    fn finite_float_text_round_trips_without_becoming_an_integer() {
        for value in [
            0.0,
            -0.0,
            4.0,
            0.1 + 0.2,
            1_000_000_000_000_000_100.0,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::MAX,
        ] {
            let text = Number::Float(value).to_string();
            let Number::Float(parsed) = Number::parse(&text).unwrap() else {
                panic!("float formatted as an integer: {text}")
            };
            assert_eq!(parsed.to_bits(), value.to_bits(), "{text}");
        }
        assert_eq!(Number::Float(4.0).to_string(), "4.0");
        assert_eq!(Number::Float(-0.0).to_string(), "-0.0");
    }
}
