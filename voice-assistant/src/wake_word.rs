use anyhow::Result;
use tokio::sync::mpsc;
use crate::{config::AssistantConfig, AssistantEvent, stt::SpeechToText};
use tracing::{info, debug};
use std::sync::Arc;

pub struct WakeWordDetector {
    wake_word: String,
    stt: Arc<SpeechToText>,
}

impl WakeWordDetector {
    pub async fn new(config: &AssistantConfig) -> Result<Self> {
        info!("Initializing wake word detection for: {}", config.wake_word.wake_word);
        let stt = Arc::new(SpeechToText::new(config).await?);
        
        Ok(Self {
            wake_word: "computer".to_string(), // Fixed wake word for Star Trek computer
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

            if addressed_command(&transcription).is_some() {
                info!("Wake word '{}' detected!", self.wake_word);
                return Ok(Some(transcription));
            }
            if transcription_lower.contains(&self.wake_word) {
                info!("Ignoring '{}' said in passing: {}", self.wake_word, transcription);
            }
        }

        Ok(None)
    }
}

/// If the utterance is addressed to the computer, the command that follows the wake word
/// ("" when only "Computer" was said).
///
/// The wake word has to open the utterance (after an optional "hey", "okay"...), the way the
/// ship's computer is hailed. "Computer" in the middle or at the end of a sentence is just a
/// word in a conversation and must not wake the assistant.
pub fn addressed_command(text: &str) -> Option<String> {
    static ADDRESSED: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = ADDRESSED.get_or_init(|| {
        regex::Regex::new(
            r"(?i)^[\s\p{P}]*(?:(?:hey|hi|ok|okay|uh|um|oh|so|alright|all right|hé|he|euh|bon|dis|alors)[\s\p{P}]+)*(?:computer|computeur|compiouteur|ordinateur)\b[\s\p{P}]*(.*)$",
        )
        .expect("valid wake word pattern")
    });
    let rest = re.captures(text)?.get(1).map_or("", |m| m.as_str()).trim();
    if rest.chars().any(|c| c.is_alphanumeric()) {
        Some(rest.to_string())
    } else {
        Some(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::addressed_command;

    #[test]
    fn wake_word_opening_the_utterance_wakes() {
        assert_eq!(addressed_command("Computer.").as_deref(), Some(""));
        assert_eq!(addressed_command("computer").as_deref(), Some(""));
        assert_eq!(addressed_command("Computer, what time is it?").as_deref(), Some("what time is it?"));
        assert_eq!(addressed_command("Hey computer set a timer").as_deref(), Some("set a timer"));
        assert_eq!(addressed_command("Okay, computer. Stop.").as_deref(), Some("Stop."));
        assert_eq!(addressed_command("...Computer!").as_deref(), Some(""));
        // How the French model writes it
        assert_eq!(addressed_command("Computeur, allume la lampe de lecture.").as_deref(), Some("allume la lampe de lecture."));
        assert_eq!(addressed_command("Euh, ordinateur.").as_deref(), Some(""));
    }

    #[test]
    fn wake_word_in_conversation_is_ignored() {
        for text in [
            "in the computer.",
            "This entire folder will take it and put it somewhere in the computer.",
            "teamwork, collaboration, and computer.",
            "The computer is slow today",
            "computers are great",
            "",
        ] {
            assert_eq!(addressed_command(text), None, "{text}");
        }
    }
}
