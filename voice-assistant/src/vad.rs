use anyhow::Result;
use crate::config::AssistantConfig;

pub struct VoiceActivityDetector {
    threshold: f32,
}

impl VoiceActivityDetector {
    pub fn new(config: &AssistantConfig) -> Result<Self> {
        Ok(Self {
            threshold: 0.01, // Simple energy-based threshold
        })
    }

    pub fn detect_speech(&self, audio_data: &[f32]) -> Result<bool> {
        // Simple energy-based VAD
        let energy: f32 = audio_data.iter().map(|x| x * x).sum();
        let avg_energy = energy / audio_data.len() as f32;
        
        Ok(avg_energy > self.threshold)
    }
}
