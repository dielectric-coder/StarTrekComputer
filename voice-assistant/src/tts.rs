use anyhow::{bail, Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use crate::config::AssistantConfig;
use tracing::{info, error, warn};

pub struct TextToSpeech {
    config: AssistantConfig,
    sample_rate: u32,
    kokoro: Mutex<Option<KokoroProcess>>,
}

/// Helper that keeps the Kokoro model loaded (see scripts/kokoro_server.py).
const KOKORO_SERVER: &str = include_str!("../scripts/kokoro_server.py");

struct KokoroProcess {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
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

        // Start Kokoro up front so a bad setup fails at startup, not on the first reply
        let kokoro = if config.tts.engine == "kokoro" {
            Some(start_kokoro(config).await?)
        } else {
            None
        };

        Ok(Self {
            config: config.clone(),
            sample_rate,
            kokoro: Mutex::new(kokoro),
        })
    }

    pub async fn speak(&self, text: &str) -> Result<()> {
        match self.config.tts.engine.as_str() {
            "piper" => self.speak_piper(text).await,
            "kokoro" => self.speak_kokoro(text).await,
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
            play_pcm(&output.stdout, self.sample_rate).await
        } else {
            error!("Piper TTS failed: {}", String::from_utf8_lossy(&output.stderr));
            Err(anyhow::anyhow!("TTS failed"))
        }
    }

    async fn speak_kokoro(&self, text: &str) -> Result<()> {
        info!("Speaking: {}", text);

        let mut kokoro = self.kokoro.lock().await;
        if kokoro.is_none() {
            *kokoro = Some(start_kokoro(&self.config).await?);
        }

        let request = serde_json::json!({
            "text": text,
            "voice": self.config.tts.voice,
            "speed": self.config.tts.speed.clamp(0.5, 2.0),
            "lang": kokoro_language(&self.config.tts.voice),
        });
        let result = match kokoro.as_mut() {
            Some(process) => process.synthesize(&request).await,
            None => unreachable!(),
        };
        let (pcm, rate) = match result {
            Ok(audio) => audio,
            Err(e) => {
                // Restart the helper on the next request in case it died
                *kokoro = None;
                return Err(e);
            }
        };
        drop(kokoro);

        play_pcm(&pcm, rate).await
    }
}

impl KokoroProcess {
    async fn synthesize(&mut self, request: &serde_json::Value) -> Result<(Vec<u8>, u32)> {
        let mut line = request.to_string();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;

        let mut header = String::new();
        if self.stdout.read_line(&mut header).await? == 0 {
            bail!("kokoro helper exited");
        }
        let header = header.trim();
        if let Some(message) = header.strip_prefix("ERR ") {
            bail!("kokoro failed: {}", message);
        }
        let mut fields = header.strip_prefix("OK ").context("unexpected kokoro response")?.split(' ');
        let len: usize = fields.next().context("missing length")?.parse()?;
        let rate: u32 = fields.next().context("missing sample rate")?.parse()?;

        let mut pcm = vec![0u8; len];
        self.stdout.read_exact(&mut pcm).await?;
        Ok((pcm, rate))
    }
}

async fn start_kokoro(config: &AssistantConfig) -> Result<KokoroProcess> {
    let tts = &config.tts;
    for (path, setting) in [(&tts.kokoro_model, "kokoro_model"), (&tts.kokoro_voices, "kokoro_voices")] {
        anyhow::ensure!(
            path.exists(),
            "kokoro file not found at {} (set tts.{} in config.toml)",
            path.display(),
            setting
        );
    }

    let script = config.general.data_dir.join("kokoro_server.py");
    tokio::fs::create_dir_all(&config.general.data_dir).await?;
    tokio::fs::write(&script, KOKORO_SERVER).await?;

    let mut child = Command::new(&tts.kokoro_python)
        .arg("-I")
        .arg(&script)
        .arg(&tts.kokoro_model)
        .arg(&tts.kokoro_voices)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("failed to start kokoro helper with {}", tts.kokoro_python.display()))?;

    let stdin = child.stdin.take().context("kokoro helper has no stdin")?;
    let mut stdout = BufReader::new(child.stdout.take().context("kokoro helper has no stdout")?);

    let mut ready = String::new();
    stdout.read_line(&mut ready).await?;
    let voices = ready.trim().strip_prefix("READY ").with_context(|| {
        format!(
            "kokoro helper failed to start (is kokoro-onnx installed for {}?)",
            tts.kokoro_python.display()
        )
    })?;
    if !voices.split(',').any(|v| v == tts.voice) {
        bail!("unknown kokoro voice '{}'; available: {}", tts.voice, voices.replace(',', ", "));
    }

    info!("Kokoro TTS ready with voice {}", tts.voice);
    Ok(KokoroProcess { _child: child, stdin, stdout })
}

/// Kokoro voice names start with a language letter: a = American, b = British English.
fn kokoro_language(voice: &str) -> &'static str {
    match voice.chars().next() {
        Some('b') => "en-gb",
        Some('a') => "en-us",
        _ => {
            warn!("Voice {} is not English; using en-us pronunciation", voice);
            "en-us"
        }
    }
}

async fn play_pcm(pcm: &[u8], sample_rate: u32) -> Result<()> {
    let mut play_cmd = Command::new("aplay")
        .arg("-f")
        .arg("S16_LE")
        .arg("-r")
        .arg(sample_rate.to_string())
        .arg("-c")
        .arg("1")
        .arg("-q")
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to start aplay for audio playback")?;

    if let Some(mut stdin) = play_cmd.stdin.take() {
        stdin.write_all(pcm).await?;
    }

    play_cmd.wait().await?;
    Ok(())
}

/// Piper voices ship a `<model>.onnx.json` that records the output sample rate.
fn read_voice_sample_rate(model_path: &std::path::Path) -> Option<u32> {
    let mut config_path = model_path.as_os_str().to_owned();
    config_path.push(".json");
    let content = std::fs::read_to_string(config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json["audio"]["sample_rate"].as_u64().map(|rate| rate as u32)
}
