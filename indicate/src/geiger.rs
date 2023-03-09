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

use std::collections::HashMap;

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
pub struct GeigerOutput {
    packages: Vec<GeigerPackageOutput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeigerPackageOutput {
    unsafety: GeigerUnsafety,
    id: HashMap<String, String>,
}

/// The output of `cargo-geiger` for one package/dependency
///
/// Corresponds to the object named "unsafety" in a `cargo-geiger` output.
/// `used` and `unused` refers to if the code is used by the package used
/// to provide the Geiger data. A package may have a high unsafe usage, but
/// nothing is used by the analyzed package.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct GeigerUnsafety {
    used: GeigerTargets,
    unused: GeigerTargets,
    forbids_unsafe: bool,
}

impl GeigerUnsafety {
    pub fn used(&self) -> GeigerTargets {
        self.used
    }

    pub fn used_safe(&self) -> u32 {
        self.used.total_safe()
    }

    pub fn used_unsafe(&self) -> u32 {
        self.used.total_unsafe()
    }

    pub fn unused(&self) -> GeigerTargets {
        self.unused
    }

    pub fn unused_safe(&self) -> u32 {
        self.unused.total_safe()
    }

    pub fn unused_unsafe(&self) -> u32 {
        self.unused.total_unsafe()
    }

    pub fn forbids_unsafe(&self) -> bool {
        self.forbids_unsafe
    }
}

/// All different targets in Rust code that `cargo-geiger` counts
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct GeigerTargets {
    functions: GeigerCount,
    exprs: GeigerCount,
    item_impls: GeigerCount,
    item_traits: GeigerCount,
    methods: GeigerCount,
}

impl GeigerTargets {
    pub fn total_safe(&self) -> u32 {
        self.functions.safe
            + self.exprs.safe
            + self.item_impls.safe
            + self.item_traits.safe
            + self.methods.safe
    }

    pub fn total_unsafe(&self) -> u32 {
        self.functions.unsafe_
            + self.exprs.unsafe_
            + self.item_impls.unsafe_
            + self.item_traits.unsafe_
            + self.methods.unsafe_
    }
}

/// The safety stats for a package analyzed by `cargo-geiger`
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct GeigerCount {
    safe: u32,
    unsafe_: u32,
}

impl GeigerCount {
    pub fn safe(&self) -> u32 {
        self.safe
    }

    pub fn unsafe_(&self) -> u32 {
        self.unsafe_
    }

    /// The total amount of counts made by Geiger
    pub fn total(&self) -> u32 {
        self.safe + self.unsafe_
        // GeigerTargets {
        //     functions: self.safe.functions + self.unsafe_.functions,
        //     exprs: self.safe.exprs + self.unsafe_.exprs,
        //     item_impls: self.safe.item_impls + self.unsafe_.item_impls,
        //     item_traits: self.safe.item_traits + self.unsafe_.item_traits,
        //     methods: self.safe.methods + self.unsafe_.methods,
        // }
    }
}

#[cfg(test)]
mod test {
    use std::{fs, path::Path};

    use test_case::test_case;

    use super::GeigerOutput;

    #[test_case(0, 0 => 0.0)]
    #[test_case(3, 1 => 25.0)]
    #[test_case(9, 1 => 10.0)]
    #[test_case(2, 1 => 33.33)]
    fn two_digit_percentage(safe_count: u32, unsafe_count: u32) -> f64 {
        super::two_digit_percentage(unsafe_count, safe_count + unsafe_count)
    }

    #[test_case("simple_deps")]
    fn deserialize_geiger_output_smoke_test(crate_name: &'static str) {
        let path_string = format!("test_data/geiger-output/{crate_name}.json");
        let path = Path::new(&path_string);
        let json_string = fs::read_to_string(path).unwrap();
        serde_json::from_str::<GeigerOutput>(&json_string).unwrap();
    }
}
