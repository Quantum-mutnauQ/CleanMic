use oxiaudio_dsp::{biquad, BiquadFilter, Compressor, DeEsser, Expander, MultibandCompressor, NoiseGate,ParametricEq,};
use std::f32::consts::FRAC_1_SQRT_2;
use gtk::prelude::*;
use gtk::*;
use std::boxed::Box as HeapBox;

pub const SAMPLE_RATE: u32 = 48_000;

// ───────────────────────────── Trait & Kette ─────────────────────────────

pub type ParamSender = std::rc::Rc<dyn Fn(&'static str, f32)>;

pub trait Effect: Send + 'static {
    fn process(&mut self, samples: &mut [f32]);

    fn configGUI(&self, send: ParamSender) -> gtk::Box;

    fn set_param(&mut self, _param: &str, _value: f32) {}

    fn new() -> Self
    where
        Self: Sized;

    fn name() -> &'static str
    where
        Self: Sized;

    fn description() -> &'static str
    where
        Self: Sized;

    fn category() -> &'static str
    where
        Self: Sized;
}

pub type EffectId = u64;

pub struct EffectChain {
    effects: Vec<Slot>,
    enabled: bool,
}

struct Slot {
    id: EffectId,
    enabled: bool,
    fx: HeapBox<dyn Effect>
}

impl Default for EffectChain {
    fn default() -> Self {
        Self { effects: Vec::new(), enabled: true }
    }
}

pub struct EffectDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub constructor: fn() -> HeapBox<dyn Effect>,
}

pub fn available_effects() -> Vec<EffectDescriptor> {
    vec![
        EffectDescriptor {
            name: Gain::name(),
            description: Gain::description(),
            category: Gain::category(),
            constructor: || HeapBox::new(Gain::new()),
        },
    ]
}

impl EffectChain {
    pub fn add(&mut self, id: EffectId, fx: HeapBox<dyn Effect>) {
        self.effects.push(Slot { id, enabled: true, fx });
    }

    pub fn remove(&mut self, id: EffectId) {
        self.effects.retain(|s| s.id != id);
    }

    pub fn move_to(&mut self, id: EffectId, new_index: usize) {
        if let Some(from) = self.effects.iter().position(|s| s.id == id) {
            let slot = self.effects.remove(from);
            self.effects.insert(new_index.min(self.effects.len()), slot);
        }
    }

    pub fn set_slot_enabled(&mut self, id: EffectId, on: bool) {
        if let Some(s) = self.effects.iter_mut().find(|s| s.id == id) {
            s.enabled = on;
        }
    }

    pub fn set_param(&mut self, id: EffectId, p: &str, v: f32) {
        if let Some(s) = self.effects.iter_mut().find(|s| s.id == id) {
            s.fx.set_param(p, v);
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }
        for s in self.effects.iter_mut().filter(|s| s.enabled) {
            s.fx.process(samples);
        }
    }

}

// ───────────────────────────── Hilfsfunktionen ─────────────────────────────

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[inline]
fn lin_to_db(x: f32) -> f32 {
    if x > 1e-10 { 20.0 * x.log10() } else { -200.0 }
}

fn time_coeff(ms: f32, sr: u32) -> f32 {
    if ms > 0.0 { (-1.0 / (ms * sr as f32 / 1000.0)).exp() } else { 0.0 }
}

#[inline]
fn smooth_gain(env: f32, target: f32, attack: f32, release: f32) -> f32 {
    if target < env {
        attack * env + (1.0 - attack) * target
    } else {
        release * env + (1.0 - release) * target
    }
}

#[inline]
fn compressor_gain_db(level_db: f32, threshold: f32, ratio: f32, knee: f32) -> f32 {
    let target = if knee <= 0.0 {
        if level_db < threshold { level_db } else { threshold + (level_db - threshold) / ratio }
    } else {
        let start = threshold - knee / 2.0;
        let end = threshold + knee / 2.0;
        if level_db < start {
            level_db
        } else if level_db > end {
            threshold + (level_db - threshold) / ratio
        } else {
            let t = (level_db - start) / knee;
            level_db + (1.0 / ratio - 1.0) * t * t * knee / 2.0
        }
    };
    target - level_db
}

// ───────────────────────────── Biquad / Filter ─────────────────────────────

#[derive(Clone)]
struct Biquad {
    c: BiquadFilter,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn new(c: BiquadFilter) -> Self {
        Self { c, z1: 0.0, z2: 0.0 }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.c.b0 * x + self.z1;
        self.z1 = self.c.b1 * x - self.c.a1 * y + self.z2;
        self.z2 = self.c.b2 * x - self.c.a2 * y;
        y
    }
}

pub struct Filter {
    sections: Vec<Biquad>,
}

impl Filter {
    pub fn from_sections(sections: impl IntoIterator<Item = BiquadFilter>) -> Self {
        Self { sections: sections.into_iter().map(Biquad::new).collect() }
    }

    pub fn highpass(order: usize, hz: f32, sr: u32) -> Self {
        Self::from_sections(biquad::butterworth_highpass(order, hz, sr))
    }

    pub fn lowpass(order: usize, hz: f32, sr: u32) -> Self {
        Self::from_sections(biquad::butterworth_lowpass(order, hz, sr))
    }

    pub fn notch(hz: f32, q: f32, sr: u32) -> Self {
        Self::from_sections([BiquadFilter::notch(hz, q, sr)])
    }

    pub fn peaking(hz: f32, q: f32, gain_db: f32, sr: u32) -> Self {
        Self::from_sections([BiquadFilter::peaking_eq(hz, q, gain_db, sr)])
    }

    pub fn low_shelf(hz: f32, gain_db: f32, sr: u32) -> Self {
        Self::from_sections([BiquadFilter::low_shelf(hz, gain_db, sr)])
    }

    pub fn high_shelf(hz: f32, gain_db: f32, sr: u32) -> Self {
        Self::from_sections([BiquadFilter::high_shelf(hz, gain_db, sr)])
    }

    pub fn parametric(eq: ParametricEq) -> Self {
        Self::from_sections(eq.bands)
    }

    pub fn graphic_eq(gains_db: &[f32], n_bands: usize, sr: u32) -> Option<Self> {
        ParametricEq::graphic_eq(gains_db, n_bands, sr).ok().map(Self::parametric)
    }
}

impl Effect for Filter {
    fn process(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            let mut v = *s;
            for sec in &mut self.sections {
                v = sec.tick(v);
            }
            *s = v;
        }
    }
    fn new() -> Self {
        Self::highpass(2, 80.0, SAMPLE_RATE)
    }
    fn configGUI(&self, send: ParamSender) -> gtk::Box{
        gtk::Box::new(Orientation::Vertical, 0)
    }

    fn name() -> &'static str {
        "Filter"
    }

    fn description() -> &'static str {
        ""
    }

    fn category() -> &'static str {
        ""
    }
}

// ───────────────────────────── Kompressor / Limiter ─────────────────────────────

pub struct CompressorFx {
    p: Compressor,
    attack: f32,
    release: f32,
    makeup: f32,
    env: f32,
}

impl CompressorFx {
    pub fn new(p: Compressor, sr: u32) -> Self {
        Self {
            attack: time_coeff(p.attack_ms, sr),
            release: time_coeff(p.release_ms, sr),
            makeup: db_to_lin(p.makeup_gain_db),
            env: 0.0,
            p,
        }
    }

    pub fn limiter(threshold_db: f32, release_ms: f32, sr: u32) -> Self {
        Self::new(Compressor::new(threshold_db, 1000.0, 0.01, release_ms), sr)
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let gain_db = compressor_gain_db(
            lin_to_db(x.abs()),
            self.p.threshold_db,
            self.p.ratio.max(1.0),
            self.p.knee_db,
        );
        self.env = smooth_gain(self.env, gain_db, self.attack, self.release);
        x * db_to_lin(self.env) * self.makeup
    }
}

impl Effect for CompressorFx {
    fn process(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            *s = self.tick(*s);
        }
    }
    fn new() -> Self {
        let mut p = Compressor::new(-18.0, 4.0, 10.0, 100.0);
        p.knee_db = 6.0;
        Self::new(p, SAMPLE_RATE)
    }

    fn configGUI(&self, send: ParamSender) -> gtk::Box{
        gtk::Box::new(Orientation::Vertical, 0)
    }
    fn name() -> &'static str {
        "CompressorFx"
    }

    fn description() -> &'static str {
        ""
    }

    fn category() -> &'static str {
        ""
    }
}

// ───────────────────────────── Expander ─────────────────────────────

pub struct ExpanderFx {
    p: Expander,
    attack: f32,
    release: f32,
    env: f32,
}

impl ExpanderFx {
    pub fn new(p: Expander, sr: u32) -> Self {
        Self {
            attack: time_coeff(p.attack_ms, sr),
            release: time_coeff(p.release_ms, sr),
            env: 0.0,
            p,
        }
    }
}

impl Effect for ExpanderFx {
    fn process(&mut self, samples: &mut [f32]) {
        let ratio = self.p.ratio.max(1.0);
        for s in samples.iter_mut() {
            let x_db = lin_to_db(s.abs());
            let gain_db = if x_db >= self.p.threshold_db {
                0.0
            } else {
                ((x_db - self.p.threshold_db) * (ratio - 1.0)).max(self.p.range_db)
            };
            self.env = smooth_gain(self.env, gain_db, self.attack, self.release);
            *s *= db_to_lin(self.env);
        }
    }
    fn new() -> Self {
        Self::new(Expander::new(-40.0, 2.0, 5.0, 100.0), SAMPLE_RATE)
    }
    fn configGUI(&self, send: ParamSender) -> gtk::Box{
        gtk::Box::new(Orientation::Vertical, 0)
    }

    fn name() -> &'static str {
        "ExpanderFX"
    }

    fn description() -> &'static str {
        ""
    }

    fn category() -> &'static str {
        ""
    }
}

// ───────────────────────────── Noise Gate ─────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum GateState {
    Closed,
    Attack,
    Open,
    Hold,
    Release,
}

pub struct GateFx {
    threshold: f32,
    floor: f32,
    attack_n: usize,
    hold_n: usize,
    release_n: usize,
    state: GateState,
    counter: usize,
    gain: f32,
}

impl GateFx {
    pub fn new(p: NoiseGate, sr: u32) -> Self {
        let n = |ms: f32| (ms * sr as f32 / 1000.0).max(1.0) as usize;
        let floor = db_to_lin(p.range_db);
        Self {
            threshold: if p.threshold_db <= -199.0 { 0.0 } else { db_to_lin(p.threshold_db) },
            floor,
            attack_n: n(p.attack_ms),
            hold_n: n(p.hold_ms),
            release_n: n(p.release_ms),
            state: GateState::Closed,
            counter: 0,
            gain: floor,
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        use GateState::*;
        let above = x.abs() >= self.threshold;
        let (state, counter) = match self.state {
            Closed => if above { (Attack, 0) } else { (Closed, self.counter) },
            Attack => {
                let c = self.counter + 1;
                if !above && c >= self.attack_n {
                    (Closed, 0)
                } else if c >= self.attack_n {
                    (Open, 0)
                } else {
                    (Attack, c)
                }
            }
            Open => if above { (Open, 0) } else { (Hold, 0) },
            Hold => {
                if above {
                    (Open, 0)
                } else {
                    let c = self.counter + 1;
                    if c >= self.hold_n { (Release, 0) } else { (Hold, c) }
                }
            }
            Release => {
                if above {
                    (Open, 0)
                } else {
                    let c = self.counter + 1;
                    if c >= self.release_n { (Closed, 0) } else { (Release, c) }
                }
            }
        };
        self.state = state;
        self.counter = counter;

        let target = match self.state {
            Closed => self.floor,
            Open | Hold => 1.0,
            Attack => self.floor + (1.0 - self.floor) * counter as f32 / self.attack_n as f32,
            Release => 1.0 - (1.0 - self.floor) * counter as f32 / self.release_n as f32,
        };
        self.gain = self.gain * 0.9 + target * 0.1;
        x * self.gain
    }
}

impl Effect for GateFx {
    fn process(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            *s = self.tick(*s);
        }
    }
    fn new() -> Self {
        Self::new(NoiseGate::new(-50.0), SAMPLE_RATE)
    }
    fn configGUI(&self, send: ParamSender) -> gtk::Box{
       gtk::Box::new(Orientation::Vertical, 0)
    }

    fn name() -> &'static str {
        "GateFx"
    }

    fn description() -> &'static str {
        ""
    }

    fn category() -> &'static str {
        ""
    }
}

// ───────────────────────────── De-Esser ─────────────────────────────

pub struct DeEsserFx {
    detect: Biquad,
    attack: f32,
    release: f32,
    threshold_db: f32,
    ratio: f32,
    env: f32,
}

impl DeEsserFx {
    pub fn new(p: DeEsser) -> Self {
        let center = (p.low_freq * p.high_freq).sqrt();
        let q = center / (p.high_freq - p.low_freq).max(1.0);
        Self {
            detect: Biquad::new(BiquadFilter::bandpass(center, q, p.sample_rate)),
            attack: time_coeff(p.attack_ms, p.sample_rate),
            release: time_coeff(p.release_ms, p.sample_rate),
            threshold_db: p.threshold_db,
            ratio: p.ratio.max(1.0),
            env: 0.0,
        }
    }
}

impl Effect for DeEsserFx {
    fn process(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            let d = self.detect.tick(*s).abs();
            let coeff = if d > self.env { self.attack } else { self.release };
            self.env = coeff * self.env + (1.0 - coeff) * d;

            let level_db = if self.env > 1e-9 { 20.0 * self.env.log10() } else { -120.0 };
            let gain_db = if level_db > self.threshold_db {
                self.threshold_db + (level_db - self.threshold_db) / self.ratio - level_db
            } else {
                0.0
            };
            *s *= db_to_lin(gain_db);
        }
    }
    fn new() -> Self {
        Self::new(DeEsser::new(-20.0, SAMPLE_RATE))
    }
    fn configGUI(&self, send: ParamSender) -> gtk::Box{
        gtk::Box::new(Orientation::Vertical, 0)
    }

    fn name() -> &'static str {
        "DeEsserFx"
    }

    fn description() -> &'static str {
        ""
    }

    fn category() -> &'static str {
        ""
    }
}

// ───────────────────────────── Multiband-Kompressor ─────────────────────────────

struct Band {
    split: Option<([Biquad; 2], [Biquad; 2])>,
    comp: CompressorFx,
}

pub struct MultibandFx {
    bands: Vec<Band>,
}

impl MultibandFx {
    pub fn new(m: MultibandCompressor, sr: u32) -> Self {
        let n = m.bands.len();
        let bands = m
            .bands
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let split = match b.crossover_hz {
                    Some(cf) if i + 1 < n => {
                        let lp = BiquadFilter::lowpass(cf, FRAC_1_SQRT_2, sr);
                        let hp = BiquadFilter::highpass(cf, FRAC_1_SQRT_2, sr);
                        Some(([Biquad::new(lp), Biquad::new(lp)], [Biquad::new(hp), Biquad::new(hp)]))
                    }
                    _ => None,
                };
                let comp = CompressorFx::new(
                    Compressor::new(b.threshold_db, b.ratio, b.attack_ms, b.release_ms)
                        .with_makeup(b.makeup_gain_db),
                    sr,
                );
                Band { split, comp }
            })
            .collect();
        Self { bands }
    }
}

impl Effect for MultibandFx {
    fn process(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            let mut remaining = *s;
            let mut sum = 0.0;
            for band in &mut self.bands {
                match &mut band.split {
                    Some((lp, hp)) => {
                        let low_mid = lp[0].tick(remaining);
                        let low = lp[1].tick(low_mid);
                        let high_mid = hp[0].tick(remaining);
                        let high = hp[1].tick(high_mid);
                        sum += band.comp.tick(low);
                        remaining = high;
                    }
                    None => {
                        sum += band.comp.tick(remaining);
                        break;
                    }
                }
            }
            *s = sum;
        }
    }
    fn new() -> Self {
        Self::new(MultibandCompressor::three_band(-20.0, -20.0, -20.0), SAMPLE_RATE)
    }
    fn configGUI(&self, send: ParamSender) -> gtk::Box{
        gtk::Box::new(Orientation::Vertical, 0)
    }

    fn name() -> &'static str {
        "Multiband Fx"
    }

    fn description() -> &'static str {
        ""
    }

    fn category() -> &'static str {
        ""
    }
}

// ───────────────────────────── Gain ─────────────────────────────

pub struct Gain(f32);

impl Gain {
    pub fn db(db: f32) -> Self {
        Self(db_to_lin(db))
    }
}

impl Effect for Gain {
    fn process(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            *s *= self.0;
        }
    }
    fn new() -> Self{
        Self(1.0)
    }

    fn configGUI(&self, send: ParamSender) -> gtk::Box{
        let settings = gtk::Box::new(Orientation::Vertical, 0);

        let spin = gtk::SpinButton::with_range(-40.0, 40.0, 0.5);

        spin.connect_value_changed(move |s| send("gain_db", s.value() as f32));

        settings.append(&spin);

        settings
    }

    fn set_param(&mut self, p: &str, v: f32) {
        if p == "gain_db" { self.0 = db_to_lin(v); }
    }

    fn name() -> &'static str {
        "Gain"
    }

    fn description() -> &'static str {
        "Adjusts the audio volume."
    }

    fn category() -> &'static str {
        "Dynamics"
    }
}
