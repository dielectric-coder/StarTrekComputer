use anyhow::Result;
use tempfile::NamedTempFile;
use tokio::process::Command as TokioCommand;
use crate::config::AssistantConfig;
use tracing::{info, error};
use hound::{WavWriter, WavSpec};

pub struct SpeechToText {
    config: AssistantConfig,
}

impl SpeechToText {
    pub async fn new(config: &AssistantConfig) -> Result<Self> {
        info!("Initializing Speech-to-Text with engine: {}", config.stt.engine);
        if config.stt.engine == "whisper" {
            anyhow::ensure!(
                config.stt.whisper_binary.exists(),
                "whisper.cpp binary not found at {} (set stt.whisper_binary in config.toml)",
                config.stt.whisper_binary.display()
            );
            anyhow::ensure!(
                config.stt.model_path.exists(),
                "whisper model not found at {} (set stt.model_path in config.toml)",
                config.stt.model_path.display()
            );
        }
        Ok(Self {
            config: config.clone(),
        })
    }

    pub async fn transcribe(&self, audio_data: &[f32]) -> Result<Option<String>> {
        match self.config.stt.engine.as_str() {
            "whisper" => self.transcribe_whisper(audio_data).await,
            "vosk" => self.transcribe_vosk(audio_data).await,
            _ => Err(anyhow::anyhow!("Unknown STT engine: {}", self.config.stt.engine)),
        }
    }

    async fn transcribe_whisper(&self, audio_data: &[f32]) -> Result<Option<String>> {
        // Save audio to temporary wav file with C920's format
        let temp_file = NamedTempFile::new()?;
        let temp_path = temp_file.path();
        
        let spec = WavSpec {
            channels: 1,  // Mono after conversion
            sample_rate: 16000,  // C920's working sample rate
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        
        let mut writer = WavWriter::create(&temp_path, spec)?;
        
        // Convert f32 to i16 for WAV file
        for sample in audio_data {
            let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer.write_sample(sample_i16)?;
        }
        writer.finalize()?;

        // Run whisper.cpp with correct model path
        let threads = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4);
        let output = TokioCommand::new(&self.config.stt.whisper_binary)
            .arg("-m")
            .arg(&self.config.stt.model_path)
            .arg("-f")
            .arg(temp_path)
            .arg("--no-timestamps")
            .arg("--no-prints")
            .arg("--threads")
            .arg(threads.to_string())
            .arg("--language")
            .arg(&self.config.stt.language)
            .output()
            .await?;

        if output.status.success() {
            let text = strip_non_speech_tags(&String::from_utf8_lossy(&output.stdout));
            let text = text.trim();
            if !text.is_empty() && text.len() > 3 {
                info!("Whisper transcription: {}", text);
                Ok(Some(text.to_string()))
            } else {
                Ok(None)
            }
        } else {
            error!("Whisper failed: {}", String::from_utf8_lossy(&output.stderr));
            Ok(None)
        }
    }

    async fn transcribe_vosk(&self, _audio_data: &[f32]) -> Result<Option<String>> {
        // TODO: Implement Vosk integration
        Ok(None)
    }
}

/// Remove whisper's non-speech annotations such as `[BLANK_AUDIO]` or `(wind blowing)`.
fn strip_non_speech_tags(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => result.push(c),
            _ => {}
        }
    }
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}
