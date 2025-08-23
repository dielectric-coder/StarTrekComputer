use anyhow::Result;
use tokio::sync::mpsc;
use crate::{config::AssistantConfig, AssistantEvent, stt::SpeechToText};
use tracing::{info, debug};
use std::sync::Arc;

pub struct WakeWordDetector {
    wake_word: String,
    sensitivity: f32,
    stt: Arc<SpeechToText>,
    last_audio_buffer: Vec<f32>,
}

impl WakeWordDetector {
    pub async fn new(config: &AssistantConfig) -> Result<Self> {
        info!("Initializing wake word detection for: {}", config.wake_word.wake_word);
        let stt = Arc::new(SpeechToText::new(config).await?);
        
        Ok(Self {
            wake_word: "computer".to_string(), // Fixed wake word for Star Trek computer
            sensitivity: config.wake_word.sensitivity,
            stt,
            last_audio_buffer: Vec::with_capacity(16000 * 3), // 3 seconds of audio buffer
        })
    }

    pub async fn start(&self, _event_tx: mpsc::UnboundedSender<AssistantEvent>) -> Result<()> {
        info!("Wake word detector started, listening for '{}'", self.wake_word);
        Ok(())
    }

    pub async fn detect(&mut self, audio_data: &[f32]) -> Result<bool> {
        // Add new audio to rolling buffer
        self.last_audio_buffer.extend_from_slice(audio_data);
        
        // Keep only last 3 seconds (48000 samples at 16kHz)
        if self.last_audio_buffer.len() > 48000 {
            self.last_audio_buffer.drain(0..self.last_audio_buffer.len() - 48000);
        }
        
        // Only process if we have enough audio (at least 1 second)
        if self.last_audio_buffer.len() < 16000 {
            return Ok(false);
        }
        
        // Use STT to transcribe recent audio and check for wake word
        if let Some(transcription) = self.stt.transcribe(&self.last_audio_buffer).await? {
            let transcription_lower = transcription.to_lowercase();
            debug!("Wake word detection transcription: {}", transcription_lower);
            
            if transcription_lower.contains(&self.wake_word) {
                info!("Wake word '{}' detected!", self.wake_word);
                self.last_audio_buffer.clear(); // Clear buffer after detection
                return Ok(true);
            }
        }
        
        Ok(false)
    }
}
