//! Bounded latency statistics for a stream peer.

/// Number of samples retained by a [`LatencyWindow`].
pub const LATENCY_SAMPLES: usize = 256;

/// Fixed-size ring of millisecond samples.
///
/// Recording and summary queries never allocate. Percentiles copy at most
/// [`LATENCY_SAMPLES`] values onto the stack before sorting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencyWindow {
    samples: [u32; LATENCY_SAMPLES],
    len: usize,
    next: usize,
}

impl Default for LatencyWindow {
    fn default() -> Self {
        Self {
            samples: [0; LATENCY_SAMPLES],
            len: 0,
            next: 0,
        }
    }
}

impl LatencyWindow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one sample. Values above `u32::MAX` are saturated.
    pub fn record(&mut self, millis: u64) {
        let value = millis.min(u32::MAX as u64) as u32;
        if self.len < LATENCY_SAMPLES {
            self.samples[self.len] = value;
            self.len += 1;
        } else {
            self.samples[self.next] = value;
            self.next = (self.next + 1) % LATENCY_SAMPLES;
        }
    }

    pub fn samples(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.next = 0;
    }

    /// Most recently recorded sample, or zero when empty.
    pub fn latest(&self) -> u32 {
        if self.len == 0 {
            return 0;
        }
        if self.len < LATENCY_SAMPLES {
            return self.samples[self.len - 1];
        }
        self.samples[(self.next + LATENCY_SAMPLES - 1) % LATENCY_SAMPLES]
    }

    pub fn p50(&self) -> u32 {
        self.percentile(50)
    }

    pub fn p95(&self) -> u32 {
        self.percentile(95)
    }

    pub fn p99(&self) -> u32 {
        self.percentile(99)
    }

    pub fn max(&self) -> u32 {
        self.samples[..self.len].iter().copied().max().unwrap_or(0)
    }

    /// Nearest-rank percentile. `percent` is clamped to `0..=100`.
    pub fn percentile(&self, percent: u8) -> u32 {
        if self.len == 0 {
            return 0;
        }
        let mut sorted = [0u32; LATENCY_SAMPLES];
        sorted[..self.len].copy_from_slice(&self.samples[..self.len]);
        let values = &mut sorted[..self.len];
        values.sort_unstable();

        let percent = (percent.min(100) as usize * self.len).div_ceil(100);
        let index = percent.saturating_sub(1).min(self.len - 1);
        values[index]
    }
}

/// Counters and latency windows reported by one stream peer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PeerMetrics {
    /// Messages fully written to the socket.
    pub frames_sent: u64,
    /// Video messages discarded by queue backpressure.
    pub frames_dropped: u64,
    /// Bytes fully written to the socket.
    pub bytes_sent: u64,
    /// Messages decoded from the socket.
    pub frames_received: u64,
    /// Wire bytes decoded from the socket.
    pub bytes_received: u64,
    /// Latest ping/pong round-trip estimate. Zero until a pong arrives.
    pub rtt_millis_estimate: u32,
    /// Age of received input frames, in milliseconds.
    pub input_latency: LatencyWindow,
    /// Time between video capture timestamps and completed socket writes.
    pub video_send_latency: LatencyWindow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_match_a_known_sample_set() {
        let mut window = LatencyWindow::new();
        for millis in 1..=100u64 {
            window.record(millis);
        }
        assert_eq!(window.samples(), 100);
        assert_eq!(window.p50(), 50);
        assert_eq!(window.p95(), 95);
        assert_eq!(window.p99(), 99);
        assert_eq!(window.max(), 100);
        assert_eq!(window.latest(), 100);
    }

    #[test]
    fn ring_evicts_the_oldest_samples() {
        let mut window = LatencyWindow::new();
        for millis in 1..=300u64 {
            window.record(millis);
        }
        assert_eq!(window.samples(), LATENCY_SAMPLES);
        assert_eq!(window.max(), 300);
        assert_eq!(window.latest(), 300);
        assert_eq!(window.p50(), 172);
    }

    #[test]
    fn empty_window_reports_zero() {
        let window = LatencyWindow::new();
        assert_eq!(window.p50(), 0);
        assert_eq!(window.p95(), 0);
        assert_eq!(window.p99(), 0);
        assert_eq!(window.max(), 0);
        assert_eq!(window.latest(), 0);
    }
}
