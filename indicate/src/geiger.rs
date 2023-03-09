//! Module to parse the output of `cargo-geiger`
//!
//! This module relies on the using the flags `--output-format Json --quiet`.
//! The output can still be quite noicy, so sometimes running `2>/dev/null` is
//! required (pipe `stderr` to the black hole at the center of the Linuxverse).
//!
//! The output of `cargo-geiger` is on the form (some fields omitted)
//! ```json
//! {
//!     "packages": [
//!         {
//!             "package": {
//!                 "id": {
//!                     "name": "libc",
//!                     "version": "0.2.139",
//!                     // Etc.
//!                 },
//!                 "unsafety": {
//!                     "used": {
//!                         "functions": {"safe":0,"unsafe_":0},
//!                         "exprs": {"safe":253,"unsafe_":12},
//!                         "item_impls": {"safe":11,"unsafe_":0},
//!                         "item_traits":{"safe":0,"unsafe_":0},
//!                         "methods":{"safe":5,"unsafe_":2}
//!                     },
//!                     "unused": {
//!                         "functions": {"safe":6,"unsafe_":24},
//!                         "exprs": {"safe":3934,"unsafe_":432},
//!                         "item_impls": {"safe":103,"unsafe_":2},
//!                         "item_traits": {"safe":0,"unsafe_":0},
//!                         "methods":{"safe":46,"unsafe_":43}},
//!                     },
//!                     "forbids_unsafe":false
//!                 }
//!             }
//!         }
//!     ]
//! }
//! ```

use serde::Deserialize;

/// Calculates a percentage, rounded to two
/// decimal points
///
/// This function will handle `0 / 0` to be equal to `0.0` (all code is safe,
/// there is no code).
fn two_digit_percentage(part: u32, total: u32) -> f64 {
    let res = f64::from(part) / f64::from(total);
    if res.is_finite() {
        // We only really care about at most two decimal points
        (res * 10000.0).round() / 100.0
    } else {
        0.0
    }
}

/// The full output of `cargo-geiger`
#[derive(Debug, Clone, Deserialize)]
pub struct GeigerOutput(Vec<GeigerPackageOutput>);

/// The output of `cargo-geiger` for one package/dependency
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct GeigerPackageOutput {
    usage: GeigerUsage,
    forbids_unsafe: bool,
}

impl GeigerPackageOutput {
    pub fn unsafety(&self) -> GeigerUsage {
        self.usage
    }

    pub fn forbids_unsafe(&self) -> bool {
        self.forbids_unsafe
    }
}

/// The output of `cargo-geiger` for the usage of unsafe code found
///
/// `used` and `unused` refers to if the code is used by the package used
/// to provide the Geiger data. A package may have a high unsafe usage, but
/// nothing is used by the analyzed package.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct GeigerUsage {
    used: GeigerUnsafety,
    unused: GeigerUnsafety,
}

impl GeigerUsage {
    pub fn used(&self) -> GeigerUnsafety {
        self.used
    }

    pub fn unused(&self) -> GeigerUnsafety {
        self.unused
    }
}

/// The safety stats for a package analyzed by `cargo-geiger`
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct GeigerUnsafety {
    safe: GeigerTargets,
    unsafe_: GeigerTargets,
}

impl GeigerUnsafety {
    fn safe(&self) -> GeigerTargets {
        self.safe
    }

    fn unsafe_(&self) -> GeigerTargets {
        self.unsafe_
    }

    /// The total amount of counts made by Geiger
    fn total(&self) -> GeigerTargets {
        GeigerTargets {
            functions: self.safe.functions + self.unsafe_.functions,
            exprs: self.safe.exprs + self.unsafe_.exprs,
            item_impls: self.safe.item_impls + self.unsafe_.item_impls,
            item_traits: self.safe.item_traits + self.unsafe_.item_traits,
            methods: self.safe.methods + self.unsafe_.methods,
        }
    }
}

/// All different targets in Rust code that `cargo-geiger` counts
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct GeigerTargets {
    functions: u32,
    exprs: u32,
    item_impls: u32,
    item_traits: u32,
    methods: u32,
}

#[cfg(test)]
mod test {
    use test_case::test_case;

    #[test_case(0, 0 => 0.0)]
    #[test_case(3, 1 => 25.0)]
    #[test_case(9, 1 => 10.0)]
    #[test_case(2, 1 => 33.33)]
    fn two_digit_percentage(safe_count: u32, unsafe_count: u32) -> f64 {
        two_digit_percentage(unsafe_count, safe_count + unsafe_count)
    }
}
