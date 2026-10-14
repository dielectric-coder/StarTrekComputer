use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use crate::{config::AssistantConfig, vad::VoiceActivityDetector, AssistantCommand};

pub enum AudioCommand {
    StartCapture,
    StopCapture,
    PlayAudio(Vec<f32>),
}

/// whisper.cpp only accepts 16 kHz mono input, so capture in that format directly.
const SAMPLE_RATE: usize = 16000;
const FRAME_MS: usize = 30;
const FRAME_SAMPLES: usize = SAMPLE_RATE * FRAME_MS / 1000;
/// Audio kept from before speech starts, so the first syllable isn't clipped.
const PRE_ROLL_FRAMES: usize = 300 / FRAME_MS;
/// Silence that ends an utterance.
const END_SILENCE_FRAMES: usize = 800 / FRAME_MS;
/// Utterances with less speech than this are treated as noise (clicks, bumps).
const MIN_SPEECH_FRAMES: usize = 250 / FRAME_MS;
const MAX_UTTERANCE_FRAMES: usize = 15_000 / FRAME_MS;

pub struct AudioManager {
    config: AssistantConfig,
    capture_task: Mutex<Option<JoinHandle<()>>>,
    paused: Arc<AtomicBool>,
}

impl AudioManager {
    pub async fn new(config: &AssistantConfig) -> Result<Self> {
        info!("Audio manager initialized");

        Ok(Self {
            config: config.clone(),
            capture_task: Mutex::new(None),
            paused: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn start_capture(&self, command_tx: mpsc::UnboundedSender<AssistantCommand>) -> Result<()> {
        let mut capture_task = self.capture_task.lock().await;

        if capture_task.is_some() {
            warn!("Audio capture already started");
            return Ok(());
        }

        let mut cmd = Command::new("parecord");
        cmd.arg("--raw")
            .arg(format!("--rate={}", SAMPLE_RATE))
            .arg("--channels=1")
            .arg("--format=s16le")
            .arg("--latency-msec=30");
        if let Some(device) = &self.config.audio.input_device {
            cmd.arg(format!("--device={}", device));
        }
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("failed to start `parecord` for microphone capture (is pipewire-pulse or pulseaudio installed?)")?;

        let stdout = child.stdout.take().context("parecord has no stdout")?;
        let vad = VoiceActivityDetector::new(&self.config)?;
        let paused = self.paused.clone();

        info!(
            "Audio capture started from {}",
            self.config.audio.input_device.as_deref().unwrap_or("default input device")
        );

        *capture_task = Some(tokio::spawn(async move {
            // Owning the child here means aborting the task kills parecord.
            let _child = child;
            if let Err(e) = capture_loop(stdout, vad, paused, command_tx).await {
                error!("Audio capture stopped: {}", e);
            }
        }));

        Ok(())
    }

    /// Ignore microphone input (e.g. while the assistant is speaking).
    pub fn pause_capture(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume_capture(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub async fn stop(&self) -> Result<()> {
        if let Some(task) = self.capture_task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }

        info!("Audio stopped");
        Ok(())
    }

    pub async fn play_audio(&self, _audio_data: Vec<f32>) -> Result<()> {
        info!("Audio playback (simplified - no actual audio playback)");
        // This would be replaced with actual audio playback using system tools
        Ok(())
    }
}

/// Read raw PCM from the recorder and send each complete utterance as one chunk.
async fn capture_loop(
    mut stdout: tokio::process::ChildStdout,
    mut vad: VoiceActivityDetector,
    paused: Arc<AtomicBool>,
    command_tx: mpsc::UnboundedSender<AssistantCommand>,
) -> Result<()> {
    let mut bytes = vec![0u8; FRAME_SAMPLES * 2];
    let mut pre_roll: VecDeque<Vec<f32>> = VecDeque::with_capacity(PRE_ROLL_FRAMES);
    let mut utterance: Vec<f32> = Vec::new();
    let mut in_utterance = false;
    let mut speech_frames = 0;
    let mut silent_frames = 0;
    let mut frame_count: usize = 0;

    loop {
        stdout
            .read_exact(&mut bytes)
            .await
            .context("microphone stream ended")?;
        let frame: Vec<f32> = bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();

        frame_count += 1;
        if frame_count % (5000 / FRAME_MS) == 0 {
            debug!(
                "Mic noise floor {:.4}, speech threshold {:.4}",
                vad.noise_floor(),
                vad.threshold()
            );
        }

        if paused.load(Ordering::SeqCst) {
            pre_roll.clear();
            utterance.clear();
            in_utterance = false;
            continue;
        }

        let is_speech = vad.detect_speech(&frame)?;

        if !in_utterance {
            if is_speech {
                in_utterance = true;
                speech_frames = 1;
                silent_frames = 0;
                utterance = pre_roll.drain(..).flatten().collect();
                utterance.extend_from_slice(&frame);
            } else {
                if pre_roll.len() == PRE_ROLL_FRAMES {
                    pre_roll.pop_front();
                }
                pre_roll.push_back(frame);
            }
            continue;
        }

        utterance.extend_from_slice(&frame);
        if is_speech {
            speech_frames += 1;
            silent_frames = 0;
        } else {
            silent_frames += 1;
        }

        let total_frames = utterance.len() / FRAME_SAMPLES;
        if silent_frames >= END_SILENCE_FRAMES || total_frames >= MAX_UTTERANCE_FRAMES {
            in_utterance = false;
            let audio = std::mem::take(&mut utterance);
            if speech_frames >= MIN_SPEECH_FRAMES {
                debug!(
                    "Utterance captured: {:.1}s, RMS {:.4}",
                    audio.len() as f32 / SAMPLE_RATE as f32,
                    VoiceActivityDetector::rms(&audio)
                );
                if command_tx.send(AssistantCommand::ProcessAudio(audio)).is_err() {
                    return Ok(()); // assistant is shutting down
                }
            }
        }
    }
}
