use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantConfig {
    pub audio: AudioConfig,
    pub stt: SttConfig,
    pub tts: TtsConfig,
    pub llm: LlmConfig,
    pub search: SearchConfig,
    pub wake_word: WakeWordConfig,
    pub rag: RagConfig,
    pub general: GeneralConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_size: usize,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    /// Minimum RMS level (0.0-1.0) counted as speech; the detector also adapts
    /// to the measured background noise and uses whichever is higher.
    #[serde(default = "default_vad_threshold")]
    pub vad_threshold: f32,
}

fn default_vad_threshold() -> f32 {
    0.015
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    pub engine: String, // "whisper" or "vosk"
    pub model_path: PathBuf,
    pub language: String,
    /// whisper.cpp command-line binary (whisper-cli)
    #[serde(default = "default_whisper_binary")]
    pub whisper_binary: PathBuf,
}

fn default_whisper_binary() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("startrek/whisper.cpp/build/bin/whisper-cli")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    pub engine: String, // "piper"
    pub model_path: PathBuf,
    pub voice: String,
    pub speed: f32,
    /// Piper executable; a bare name is looked up on PATH
    #[serde(default = "default_piper_binary")]
    pub piper_binary: PathBuf,
}

fn default_piper_binary() -> PathBuf {
    PathBuf::from("piper")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub context_length: usize,
    pub temperature: f32,
    /// Let reasoning models "think" before answering; much slower on CPU
    #[serde(default)]
    pub think: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    pub enabled: bool,
    pub search_script: PathBuf,
    pub max_results: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeWordConfig {
    pub enabled: bool,
    pub wake_word: String,
    pub sensitivity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagConfig {
    pub enabled: bool,
    pub max_conversations: i32,
    pub max_knowledge_entries: i32,
    pub cleanup_days: i64,
    pub similarity_threshold: f32,
    pub learning_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub log_level: String,
    pub data_dir: PathBuf,
}

impl Default for AssistantConfig {
    fn default() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/edward"));
        let startrek_dir = home_dir.join("startrek");
        
        Self {
            audio: AudioConfig {
                sample_rate: 16000,
                channels: 1,  // Mono for better processing
                buffer_size: 1024,
                input_device: None,  // Use system default microphone
                output_device: None, // Use system default speaker
                vad_threshold: default_vad_threshold(),
            },
            stt: SttConfig {
                engine: "whisper".to_string(),
                model_path: startrek_dir.join("whisper.cpp/models/ggml-base.en.bin"),
                language: "en".to_string(),
                whisper_binary: default_whisper_binary(),
            },
            tts: TtsConfig {
                engine: "piper".to_string(),
                model_path: startrek_dir.join("piper/voices/en_US-lessac-medium.onnx"),
                voice: "lessac".to_string(),
                speed: 0.9, // Slightly faster for computer-like speech
                piper_binary: default_piper_binary(),
            },
            llm: LlmConfig {
                base_url: "http://localhost:11434".to_string(),
                model: "qwen3:4b".to_string(), // Updated to match the specification
                context_length: 2048,
                temperature: 0.3, // Lower for more factual responses
                think: false,
            },
            search: SearchConfig {
                enabled: true,
                search_script: startrek_dir.join("search_tool.py"),
                max_results: 3,
            },
            wake_word: WakeWordConfig {
                enabled: true,
                wake_word: "computer".to_string(), // Star Trek wake word
                sensitivity: 0.5,
            },
            rag: RagConfig {
                enabled: true,
                max_conversations: 1000,
                max_knowledge_entries: 5000,
                cleanup_days: 30,
                similarity_threshold: 0.7,
                learning_rate: 0.1,
            },
            general: GeneralConfig {
                log_level: "info".to_string(),
                data_dir: startrek_dir.join(".voice-assistant"),
            },
        }
    }
}

impl AssistantConfig {
    pub async fn load() -> Result<Self> {
        let config_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/home/edward"))
            .join("startrek/.voice-assistant")
            .join("config.toml");

        if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path).await?;
            let config: AssistantConfig = toml::de::from_str(&content)?;
            Ok(config)
        } else {
            let config = Self::default();
            config.save().await?;
            Ok(config)
        }
    }

    pub async fn save(&self) -> Result<()> {
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/home/edward"))
            .join("startrek/.voice-assistant");
        
        tokio::fs::create_dir_all(&config_dir).await?;
        
        let config_path = config_dir.join("config.toml");
        let content = toml::ser::to_string_pretty(self)?;
        tokio::fs::write(&config_path, content).await?;
        
        Ok(())
    }
}
