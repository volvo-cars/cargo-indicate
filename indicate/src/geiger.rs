#[derive(Debug)]
pub struct Geiger {
    used: GeigerCount,
    unused: GeigerCount,
}

#[derive(Debug)]
pub struct GeigerCount {
    safe_count: u32,
    unsafe_count: u32,
}

impl GeigerCount {
    pub fn new(safe_count: u32, unsafe_count: u32) -> Self {
        Self {
            safe_count,
            unsafe_count,
        }
    }

    pub fn safe_count(&self) -> u32 {
        self.safe_count
    }

    pub fn unsafe_count(&self) -> u32 {
        self.unsafe_count
    }

    /// Returns the total count (safe + unsafe)
    pub fn total_count(&self) -> u32 {
        self.safe_count + self.unsafe_count
    }

    /// Calculates what percentage of the code is `unsafe`, rounded to two
    /// decimal points
    ///
    /// Since `total_count` >= `unsafe_count`, this function will handle
    /// `0 / 0` to be equal to `0.0` (all code is safe, there is no code).
    pub fn percentage_unsafe(&self) -> f64 {
        let res = f64::from(self.unsafe_count) / f64::from(self.total_count());
        if res.is_finite() {
            // We only really care about at most two decimal points
            (res * 10000.0).round() / 100.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod test {
    use super::GeigerCount;
    use test_case::test_case;

    #[test_case(0, 0 => 0.0)]
    #[test_case(3, 1 => 25.0)]
    #[test_case(9, 1 => 10.0)]
    #[test_case(2, 1 => 33.33)]
    fn percentage_unsafe(safe_count: u32, unsafe_count: u32) -> f64 {
        GeigerCount::new(safe_count, unsafe_count).percentage_unsafe()
    }
}
