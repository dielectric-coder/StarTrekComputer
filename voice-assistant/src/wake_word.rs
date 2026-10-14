use anyhow::Result;
use tokio::sync::mpsc;
use crate::{config::AssistantConfig, AssistantEvent, stt::SpeechToText};
use tracing::{info, debug};
use std::sync::Arc;

pub struct WakeWordDetector {
    wake_word: String,
    sensitivity: f32,
    stt: Arc<SpeechToText>,
}

impl WakeWordDetector {
    pub async fn new(config: &AssistantConfig) -> Result<Self> {
        info!("Initializing wake word detection for: {}", config.wake_word.wake_word);
        let stt = Arc::new(SpeechToText::new(config).await?);
        
        Ok(Self {
            wake_word: "computer".to_string(), // Fixed wake word for Star Trek computer
            sensitivity: config.wake_word.sensitivity,
            stt,
        })
    }

    pub async fn start(&self, _event_tx: mpsc::UnboundedSender<AssistantEvent>) -> Result<()> {
        info!("Wake word detector started, listening for '{}'", self.wake_word);
        Ok(())
    }

    /// Transcribe one utterance; returns the transcription if it contains the wake word.
    pub async fn detect(&mut self, audio_data: &[f32]) -> Result<Option<String>> {
        if let Some(transcription) = self.stt.transcribe(audio_data).await? {
            let transcription_lower = transcription.to_lowercase();
            debug!("Wake word detection transcription: {}", transcription_lower);

            if transcription_lower.contains(&self.wake_word) {
                info!("Wake word '{}' detected!", self.wake_word);
                return Ok(Some(transcription));
            }
        }

        Ok(None)
    }
}
