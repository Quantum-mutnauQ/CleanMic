pub trait Effect: Send {
    fn process(&mut self, samples: &mut [f32]);
}

#[derive(Default)]
pub struct EffectChain {
    effects: Vec<Box<dyn Effect>>,
}

impl EffectChain {
    pub fn push(&mut self, e: impl Effect + 'static) {
        self.effects.push(Box::new(e));
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for e in &mut self.effects {
            e.process(samples);
        }
    }
}

pub struct Gain(pub f32);

impl Effect for Gain {
    fn process(&mut self, samples: &mut [f32]) {
        for s in samples {
            *s *= self.0;
        }
    }
}


pub struct Normalizer {
    target_rms: f32,
    max_gain: f32,
    gain: f32,
    noise_gate: f32,
}

impl Normalizer {
    pub fn new() -> Self {
        Self { target_rms: 0.1, max_gain: 20.0, gain: 1.0, noise_gate: 0.002 }
    }
}

impl Effect for Normalizer {
    fn process(&mut self, samples: &mut [f32]) {
        if samples.is_empty() { return; }

        let len = samples.len() as f32;
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / len).sqrt();

        let desired = if rms < self.noise_gate {
            self.gain
        } else {
            (self.target_rms / rms).clamp(0.1, self.max_gain)
        };

        let coeff = if desired < self.gain { 0.3 } else { 0.02 };
        let step = coeff * 16.0 / len;

        for s in samples.iter_mut() {
            self.gain += (desired - self.gain) * step;
            *s = (*s * self.gain).clamp(-1.0, 1.0);
        }
    }
}
