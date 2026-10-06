//! Link simulation: throughput profiles and a small deterministic PRNG.

use std::time::Duration;

/// Progress is computed (and reported) once per tick: at most 10 Hz.
pub(crate) const TICK: Duration = Duration::from_millis(100);

/// Throughput profile used while the link is good.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// ~1 GiB/s, no latency. For UI tests and demos.
    Fast,
    /// ~80 MB/s, 3 ms RTT, ±5% jitter.
    WifiGood,
    /// ~3 MB/s, 120 ms RTT, ±40% jitter, occasional stalls.
    Weak,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LinkModel {
    pub bytes_per_sec: u64,
    pub rtt: Duration,
    jitter_pct: u64,
    stall_pct: u64,
}

impl Profile {
    pub(crate) fn model(self) -> LinkModel {
        match self {
            Self::Fast => LinkModel {
                bytes_per_sec: 1 << 30,
                rtt: Duration::ZERO,
                jitter_pct: 0,
                stall_pct: 0,
            },
            Self::WifiGood => LinkModel {
                bytes_per_sec: 80_000_000,
                rtt: Duration::from_millis(3),
                jitter_pct: 5,
                stall_pct: 0,
            },
            Self::Weak => LinkModel {
                bytes_per_sec: 3_000_000,
                rtt: Duration::from_millis(120),
                jitter_pct: 40,
                stall_pct: 5,
            },
        }
    }
}

impl LinkModel {
    /// Bytes that get through during one [`TICK`].
    pub(crate) fn bytes_this_tick(&self, rng: &mut SplitMix64) -> u64 {
        if self.stall_pct > 0 && rng.below(100) < self.stall_pct {
            return 0;
        }
        let base = self.bytes_per_sec / 10;
        // A factor in [100 - jitter, 100 + jitter] percent.
        let factor = 100 - self.jitter_pct + rng.below(2 * self.jitter_pct + 1);
        base * factor / 100
    }
}

/// `SplitMix64`: tiny, fast and deterministic for a given seed.
#[derive(Debug, Clone)]
pub(crate) struct SplitMix64(u64);

impl SplitMix64 {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..n` (`n > 0`).
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    pub(crate) fn fill(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}

/// FNV-1a, used to derive a per-file content seed from its path.
pub(crate) fn fnv1a(parts: &[String]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(b'/')) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prng_is_deterministic() {
        let (mut a, mut b) = (SplitMix64::new(7), SplitMix64::new(7));
        assert!((0..100).all(|_| a.next_u64() == b.next_u64()));
    }

    #[test]
    fn tick_budget_stays_within_jitter() {
        let mut rng = SplitMix64::new(1);
        let good = Profile::WifiGood.model();
        for _ in 0..1000 {
            let b = good.bytes_this_tick(&mut rng);
            assert!((7_600_000..=8_400_000).contains(&b), "{b}");
        }
        let weak = Profile::Weak.model();
        let stalls = (0..1000)
            .filter(|_| weak.bytes_this_tick(&mut rng) == 0)
            .count();
        assert!(stalls > 0 && stalls < 150, "{stalls}");
    }
}
