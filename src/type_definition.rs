macro_rules! impl_string {
    ($T:ident) => {
        impl PrettyPrint for $T {
            fn pprint(&self) -> &'static str {
                "string"
            }
        }
    };
}

macro_rules! impl_positive_number {
    ($T:ident) => {
        impl PrettyPrint for $T {
            fn pprint(&self) -> &'static str {
                "positive number"
            }
        }
    };
}

macro_rules! impl_decimal_number {
    ($T:ident) => {
        impl PrettyPrint for $T {
            fn pprint(&self) -> &'static str {
                "decimal number"
            }
        }
    };
}

macro_rules! impl_number {
    ($T:ident) => {
        impl PrettyPrint for $T {
            fn pprint(&self) -> &'static str {
                "number"
            }
        }
    };
}

macro_rules! impl_bool {
    ($T:ident) => {
        impl PrettyPrint for $T {
            fn pprint(&self) -> &'static str {
                "1 or 0"
            }
        }
    };
}

/// This trait implements a pretty print function to make a variable user readable. Useful for
/// messages towards the user.
///
/// | type      | output    |
/// | --------- | --------- |
/// | usize, u32, u64   | positive number   |
/// | f32, f64          | decimal           |
/// | isize, i32, i64   | number            |
/// | bool              | 1 or 0            |
/// | String, str       | string            |
pub trait PrettyPrint {
    fn pprint(&self) -> &'static str;
}

impl_string!(String);

impl_positive_number!(usize);
impl_positive_number!(u32);
impl_positive_number!(u64);

impl_decimal_number!(f32);
impl_decimal_number!(f64);

impl_number!(isize);
impl_number!(i32);
impl_number!(i64);

impl_bool!(bool);
