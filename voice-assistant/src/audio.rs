use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use std::time::Duration;
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use crate::{config::AssistantConfig, vad::VoiceActivityDetector, AssistantCommand};

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
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

pub struct AudioManager {
    config: AssistantConfig,
    capture_task: Mutex<Option<JoinHandle<()>>>,
    gate: Arc<Gate>,
}

/// What the capture loop does with the microphone while the assistant makes sound.
#[derive(Default)]
struct Gate {
    /// Discard input and drop any half-heard utterance (used while speaking).
    paused: AtomicBool,
    /// Keep recording but let nothing start or end an utterance (used for the
    /// short wake chirp, so words spoken over it aren't lost).
    held: AtomicBool,
}

impl AudioManager {
    pub async fn new(config: &AssistantConfig) -> Result<Self> {
        info!("Audio manager initialized");

        Ok(Self {
            config: config.clone(),
            capture_task: Mutex::new(None),
            gate: Arc::new(Gate::default()),
        })
    }

    pub async fn start_capture(&self, command_tx: mpsc::UnboundedSender<AssistantCommand>) -> Result<()> {
        let mut capture_task = self.capture_task.lock().await;

        if capture_task.is_some() {
            warn!("Audio capture already started");
            return Ok(());
        }

        let device = self.config.audio.input_device.clone();
        let (child, stdout) = spawn_recorder(device.as_deref())?;
        let mut vad = VoiceActivityDetector::new(&self.config)?;
        let gate = self.gate.clone();

        info!(
            "Audio capture started from {}",
            device.as_deref().unwrap_or("default input device")
        );

        *capture_task = Some(tokio::spawn(async move {
            // Owning the child here means aborting the task kills parecord.
            let mut child = child;
            let mut stdout = stdout;
            loop {
                match capture_loop(&mut stdout, &mut vad, &gate, &command_tx).await {
                    Ok(()) => return, // assistant is shutting down
                    // At login the session can start us before the microphone node exists,
                    // and devices come and go (USB, Bluetooth), so reconnect instead of going deaf.
                    Err(e) => warn!("Microphone stream lost ({}), reconnecting", e),
                }
                let _ = child.kill().await;
                loop {
                    tokio::time::sleep(RECONNECT_DELAY).await;
                    match spawn_recorder(device.as_deref()) {
                        Ok((c, s)) => {
                            child = c;
                            stdout = s;
                            break;
                        }
                        Err(e) => error!("Could not restart microphone capture: {:#}", e),
                    }
                }
            }
        }));

        Ok(())
    }

    /// Ignore microphone input (e.g. while the assistant is speaking).
    pub fn pause_capture(&self) {
        self.gate.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume_capture(&self) {
        self.gate.paused.store(false, Ordering::SeqCst);
    }

    /// Keep listening through a short sound without letting it cut an utterance short.
    pub fn hold_capture(&self) {
        self.gate.held.store(true, Ordering::SeqCst);
    }

    pub fn release_capture(&self) {
        self.gate.held.store(false, Ordering::SeqCst);
    }

    pub async fn stop(&self) -> Result<()> {
        if let Some(task) = self.capture_task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }

        info!("Audio stopped");
        Ok(())
    }

}

fn spawn_recorder(device: Option<&str>) -> Result<(Child, ChildStdout)> {
    let mut cmd = Command::new("parecord");
    cmd.arg("--raw")
        .arg(format!("--rate={}", SAMPLE_RATE))
        .arg("--channels=1")
        .arg("--format=s16le")
        .arg("--latency-msec=30");
    if let Some(device) = device {
        cmd.arg(format!("--device={}", device));
    }
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("failed to start `parecord` for microphone capture (is pipewire-pulse or pulseaudio installed?)")?;
    let stdout = child.stdout.take().context("parecord has no stdout")?;
    Ok((child, stdout))
}

/// Read raw PCM from the recorder and send each complete utterance as one chunk.
async fn capture_loop(
    stdout: &mut ChildStdout,
    vad: &mut VoiceActivityDetector,
    gate: &Gate,
    command_tx: &mpsc::UnboundedSender<AssistantCommand>,
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

        if gate.paused.load(Ordering::SeqCst) {
            pre_roll.clear();
            utterance.clear();
            in_utterance = false;
            continue;
        }

        if gate.held.load(Ordering::SeqCst) {
            // The chirp would fool the detector (and its noise floor), so just keep the audio:
            // an utterance in progress carries on, otherwise it rolls through the pre-roll.
            if in_utterance {
                utterance.extend_from_slice(&frame);
            } else {
                if pre_roll.len() == PRE_ROLL_FRAMES {
                    pre_roll.pop_front();
                }
                pre_roll.push_back(frame);
            }
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
