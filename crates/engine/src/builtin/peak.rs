#[derive(Default, Debug)]
pub struct PeakMeter {
    peak: f32,
}

impl PeakMeter {
    pub fn update(&mut self, sample_rate: u32, samples: &[f32]) -> f32 {
        let peak = samples
            .iter()
            .copied()
            .map(f32::abs)
            .fold(0.0_f32, f32::max);

        // Instant attack, exponential decay (300 ms)
        const TAU_DECAY: f32 = 0.3;
        let dt = samples.len() as f32 / sample_rate as f32;
        let a = dt / (dt + TAU_DECAY);

        if peak >= self.peak {
            self.peak = peak;
        } else {
            self.peak += a * (peak - self.peak);
        }

        self.peak
    }

    pub fn value(&self) -> f32 {
        self.peak
    }
}
