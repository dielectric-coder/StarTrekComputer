use anyhow::Result;
use std::process::Stdio;
use tokio::process::Command;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use crate::config::AssistantConfig;
use tracing::{info, error};

pub struct TextToSpeech {
    config: AssistantConfig,
}

impl TextToSpeech {
    pub async fn new(config: &AssistantConfig) -> Result<Self> {
        info!("Initializing Text-to-Speech with engine: {}", config.tts.engine);
        Ok(Self {
            config: config.clone(),
        })
    }

    pub async fn speak(&self, text: &str) -> Result<()> {
        match self.config.tts.engine.as_str() {
            "piper" => self.speak_piper(text).await,
            _ => Err(anyhow::anyhow!("Unknown TTS engine: {}", self.config.tts.engine)),
        }
    }

    async fn speak_piper(&self, text: &str) -> Result<()> {
        info!("Speaking: {}", text);
        
        let mut cmd = Command::new("/home/edward/startrek/piper/piper/piper")
            .arg("--model")
            .arg(&self.config.tts.model_path)
            .arg("--output_file")
            .arg("-")
            .arg("--length_scale")
            .arg("0.9") // Slightly faster speech for computer-like precision
            .arg("--noise_scale") 
            .arg("0.1") // Less variation for more monotone delivery
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Write text to stdin
        if let Some(stdin) = cmd.stdin.as_mut() {
            stdin.write_all(text.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Get audio output
        let output = cmd.wait_with_output().await?;
        
        if output.status.success() {
            // Play the audio using aplay
            let mut play_cmd = Command::new("aplay")
                .arg("-f")
                .arg("S16_LE")
                .arg("-r")
                .arg("22050")
                .arg("-c")
                .arg("1")
                .stdin(Stdio::piped())
                .spawn()?;
            
            if let Some(stdin) = play_cmd.stdin.as_mut() {
                stdin.write_all(&output.stdout).await?;
            }
            
            play_cmd.wait().await?;
            Ok(())
        } else {
            error!("Piper TTS failed: {}", String::from_utf8_lossy(&output.stderr));
            Err(anyhow::anyhow!("TTS failed"))
        }
    }
}
