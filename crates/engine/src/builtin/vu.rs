#[derive(Default, Debug)]
pub struct VuMeter {
    vu: f32,
}

impl VuMeter {
    pub fn update(&mut self, sample_rate: u32, samples: &[f32]) -> f32 {
        let rectified = samples.iter().copied().map(f32::abs);
        let sum: f32 = rectified.sum();

        let avg_rectified = sum / samples.len() as f32;

        // Exponential smoothing to simulate 300 ms rise/fall
        const TAU: f32 = 0.3;
        let dt = samples.len() as f32 / sample_rate as f32;
        let a = dt / (dt + TAU);

        self.vu += a * (avg_rectified - self.vu);

        self.vu
    }

    pub fn value(&self) -> f32 {
        self.vu
    }
}
