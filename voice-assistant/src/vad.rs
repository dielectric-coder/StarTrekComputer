use anyhow::Result;
use crate::config::AssistantConfig;

/// Energy-based voice activity detector working on short frames.
///
/// The speech threshold is the higher of the configured minimum and a multiple
/// of the background noise floor, which is tracked while no speech is present.
pub struct VoiceActivityDetector {
    min_threshold: f32,
    noise_floor: f32,
}

/// Speech must be this many times louder than the background noise.
const NOISE_MARGIN: f32 = 3.0;
/// How quickly the noise floor follows quieter / louder background levels.
const NOISE_ADAPT_DOWN: f32 = 0.2;
const NOISE_ADAPT_UP: f32 = 0.01;

impl VoiceActivityDetector {
    pub fn new(config: &AssistantConfig) -> Result<Self> {
        Ok(Self {
            min_threshold: config.audio.vad_threshold,
            noise_floor: config.audio.vad_threshold / NOISE_MARGIN,
        })
    }

    pub fn rms(audio_data: &[f32]) -> f32 {
        if audio_data.is_empty() {
            return 0.0;
        }
        let energy: f32 = audio_data.iter().map(|x| x * x).sum();
        (energy / audio_data.len() as f32).sqrt()
    }

    pub fn threshold(&self) -> f32 {
        self.min_threshold.max(self.noise_floor * NOISE_MARGIN)
    }

    pub fn noise_floor(&self) -> f32 {
        self.noise_floor
    }

    /// Classify one frame as speech or not, updating the noise estimate on non-speech frames.
    pub fn detect_speech(&mut self, frame: &[f32]) -> Result<bool> {
        let level = Self::rms(frame);
        let is_speech = level > self.threshold();

        if !is_speech {
            let rate = if level < self.noise_floor { NOISE_ADAPT_DOWN } else { NOISE_ADAPT_UP };
            self.noise_floor += (level - self.noise_floor) * rate;
        }

        Ok(is_speech)
    }
}
