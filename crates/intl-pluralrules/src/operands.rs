//! A number's plural operands, the shape crates.io's `intl_pluralrules`
//! gives them (Fluent fills one in and adjusts its fraction digits), and
//! the number they describe as digits, which is what ICU4X reads.
//!
//! See the operands of UTS #35:
//! <https://unicode.org/reports/tr35/tr35-numbers.html#Operands>.

#[derive(Debug, Clone, PartialEq)]
pub struct PluralOperands {
    /// The absolute value.
    pub n: f64,
    /// The integer digits.
    pub i: u64,
    /// How many fraction digits are visible, trailing zeros included.
    pub v: usize,
    /// How many fraction digits are visible, trailing zeros left out.
    pub w: usize,
    /// The visible fraction digits, trailing zeros included.
    pub f: u64,
    /// The visible fraction digits, trailing zeros left out.
    pub t: u64,
}

impl PluralOperands {
    /// The number as digits, its visible fraction digits kept: `1.50`
    /// for i = 1, v = 2, f = 50.
    pub fn digits(&self) -> String {
        if self.v == 0 {
            self.i.to_string()
        } else {
            format!("{}.{:0>width$}", self.i, self.f, width = self.v)
        }
    }
}

pub(crate) const NOT_A_NUMBER: &str = "Argument can not be parsed to operands.";

impl<'a> TryFrom<&'a str> for PluralOperands {
    type Error = &'static str;

    /// A number written in digits, with a sign and a fraction or not
    /// (`-1234.50`).
    fn try_from(input: &'a str) -> Result<Self, Self::Error> {
        let abs = input.strip_prefix('-').unwrap_or(input);
        let (int, fraction) = abs.split_once('.').unwrap_or((abs, ""));
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if !digits(int) || !(fraction.is_empty() || digits(fraction)) {
            return Err(NOT_A_NUMBER);
        }
        let n: f64 = abs.parse().map_err(|_| NOT_A_NUMBER)?;
        let i: u64 = int.parse().map_err(|_| NOT_A_NUMBER)?;
        let trimmed = fraction.trim_end_matches('0');
        let number = |s: &str| -> Result<u64, &'static str> {
            if s.is_empty() {
                Ok(0)
            } else {
                s.parse().map_err(|_| NOT_A_NUMBER)
            }
        };
        Ok(PluralOperands {
            n,
            i,
            v: fraction.len(),
            w: trimmed.len(),
            f: number(fraction)?,
            t: number(trimmed)?,
        })
    }
}

macro_rules! unsigned {
    ($($ty:ty)+) => {$(
        impl From<$ty> for PluralOperands {
            fn from(input: $ty) -> Self {
                PluralOperands {
                    n: input as f64,
                    i: input as u64,
                    v: 0,
                    w: 0,
                    f: 0,
                    t: 0,
                }
            }
        }
    )+};
}

macro_rules! signed {
    ($($ty:ty)+) => {$(
        impl TryFrom<$ty> for PluralOperands {
            type Error = &'static str;
            fn try_from(input: $ty) -> Result<Self, Self::Error> {
                let abs = input.checked_abs().ok_or("Number too big")?;
                Ok(PluralOperands {
                    n: abs as f64,
                    i: abs as u64,
                    v: 0,
                    w: 0,
                    f: 0,
                    t: 0,
                })
            }
        }
    )+};
}

macro_rules! written {
    ($($ty:ty)+) => {$(
        impl TryFrom<$ty> for PluralOperands {
            type Error = &'static str;
            /// As the number writes itself (`1.5`; a float never writes
            /// an exponent).
            fn try_from(input: $ty) -> Result<Self, Self::Error> {
                PluralOperands::try_from(input.to_string().as_str())
            }
        }
    )+};
}

unsigned!(u8 u16 u32 u64 usize);
signed!(i8 i16 i32 i64 isize);
written!(f32 f64 String);
