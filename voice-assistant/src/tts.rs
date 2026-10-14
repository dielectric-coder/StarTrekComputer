use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::Command;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use crate::config::AssistantConfig;
use tracing::{info, error};

pub struct TextToSpeech {
    config: AssistantConfig,
    sample_rate: u32,
}

/// Piper's raw output rate when the voice config doesn't say otherwise.
const DEFAULT_PIPER_SAMPLE_RATE: u32 = 22050;

impl TextToSpeech {
    pub async fn new(config: &AssistantConfig) -> Result<Self> {
        info!("Initializing Text-to-Speech with engine: {}", config.tts.engine);

        let mut sample_rate = DEFAULT_PIPER_SAMPLE_RATE;
        if config.tts.engine == "piper" {
            anyhow::ensure!(
                config.tts.model_path.exists(),
                "piper voice not found at {} (set tts.model_path in config.toml)",
                config.tts.model_path.display()
            );
            sample_rate = read_voice_sample_rate(&config.tts.model_path).unwrap_or(DEFAULT_PIPER_SAMPLE_RATE);
        }

        Ok(Self {
            config: config.clone(),
            sample_rate,
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

        // tts.speed is a rate multiplier (1.0 = voice's natural pace, lower = slower)
        let length_scale = 1.0 / self.config.tts.speed.clamp(0.25, 4.0);
        
        let mut cmd = Command::new(&self.config.tts.piper_binary)
            .arg("--model")
            .arg(&self.config.tts.model_path)
            .arg("--output-raw")
            .arg("--length_scale")
            .arg(length_scale.to_string()) // piper's phoneme length: >1.0 is slower
            .arg("--noise_scale") 
            .arg("0.1") // Less variation for more monotone delivery
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to start piper ({})", self.config.tts.piper_binary.display()))?;

        // Write text to stdin
        // Close stdin after writing so piper knows the input is complete
        if let Some(mut stdin) = cmd.stdin.take() {
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
                .arg(self.sample_rate.to_string())
                .arg("-c")
                .arg("1")
                .arg("-q")
                .stdin(Stdio::piped())
                .spawn()
                .context("failed to start aplay for audio playback")?;

            if let Some(mut stdin) = play_cmd.stdin.take() {
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

/// Piper voices ship a `<model>.onnx.json` that records the output sample rate.
fn read_voice_sample_rate(model_path: &std::path::Path) -> Option<u32> {
    let mut config_path = model_path.as_os_str().to_owned();
    config_path.push(".json");
    let content = std::fs::read_to_string(config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json["audio"]["sample_rate"].as_u64().map(|rate| rate as u32)
}
