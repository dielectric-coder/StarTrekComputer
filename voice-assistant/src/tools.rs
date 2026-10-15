//! Actions the LLM can invoke through Ollama tool calling: timers, reminders,
//! system status, volume control and the captain's log.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{info, warn};
use crate::{config::AssistantConfig, AssistantCommand};

/// A captain's log recording ends by itself after this much silence.
const LOG_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Phrases that end a captain's log recording when they finish an utterance.
/// Includes common mishearings ("and log").
const LOG_END_PHRASES: &[&str] = &[
    "end log", "and log", "end of log", "and of log", "end the log", "and the log",
    "end captain's log", "end captains log", "stop log", "stop recording", "end recording",
];

/// Result of running a tool.
pub struct ToolOutput {
    /// Text handed back to the LLM.
    pub result: String,
    /// When set, spoken as-is instead of asking the LLM to phrase the result.
    pub speech: Option<String>,
}

impl ToolOutput {
    fn speak(text: impl Into<String>) -> Self {
        let text = text.into();
        Self { result: text.clone(), speech: Some(text) }
    }

    fn data(text: impl Into<String>) -> Self {
        Self { result: text.into(), speech: None }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Timer,
    Reminder,
}

struct Scheduled {
    id: u64,
    kind: Kind,
    label: String,
    due: DateTime<Local>,
    task: JoinHandle<()>,
}

/// Reminder as persisted in `reminders.json`.
#[derive(Serialize, Deserialize)]
struct SavedReminder {
    due: DateTime<Local>,
    message: String,
}

struct LogSession {
    path: PathBuf,
    /// File length before this session's header, to undo an empty session.
    start_len: u64,
    words: usize,
    last_activity: Instant,
}

struct Inner {
    command_tx: mpsc::UnboundedSender<AssistantCommand>,
    data_dir: PathBuf,
    scheduled: Mutex<Vec<Scheduled>>,
    next_id: AtomicU64,
    log: Mutex<Option<LogSession>>,
}

pub struct ToolBox {
    inner: Arc<Inner>,
}

impl ToolBox {
    pub async fn new(config: &AssistantConfig, command_tx: mpsc::UnboundedSender<AssistantCommand>) -> Result<Self> {
        let inner = Arc::new(Inner {
            command_tx,
            data_dir: config.general.data_dir.clone(),
            scheduled: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            log: Mutex::new(None),
        });

        // Restore reminders; ones that came due while the assistant was off fire right away
        if let Ok(content) = tokio::fs::read_to_string(inner.reminders_path()).await {
            let saved: Vec<SavedReminder> = serde_json::from_str(&content).unwrap_or_default();
            info!("Restoring {} reminder(s)", saved.len());
            for reminder in saved {
                Inner::schedule(&inner, Kind::Reminder, reminder.message, reminder.due).await;
            }
        }

        Ok(Self { inner })
    }

    /// Tool definitions in Ollama's function-calling format.
    pub fn definitions() -> Value {
        json!([
            {"type": "function", "function": {
                "name": "set_timer",
                "description": "Start a countdown timer. Give the duration in any combination of hours, minutes and seconds.",
                "parameters": {"type": "object", "properties": {
                    "hours": {"type": "number"},
                    "minutes": {"type": "number"},
                    "seconds": {"type": "number"},
                    "label": {"type": "string", "description": "What the timer is for, e.g. 'tea'"}
                }}
            }},
            {"type": "function", "function": {
                "name": "set_reminder",
                "description": "Remind the user of something at a clock time, or after a number of minutes.",
                "parameters": {"type": "object", "required": ["message"], "properties": {
                    "message": {"type": "string", "description": "What to remind the user about"},
                    "time": {"type": "string", "description": "24-hour local time HH:MM"},
                    "date": {"type": "string", "description": "YYYY-MM-DD; omit for the next occurrence of the time"},
                    "in_minutes": {"type": "number", "description": "Alternative to time: minutes from now"}
                }}
            }},
            {"type": "function", "function": {
                "name": "list_timers",
                "description": "List active timers and reminders with the time remaining.",
                "parameters": {"type": "object", "properties": {}}
            }},
            {"type": "function", "function": {
                "name": "cancel_timer",
                "description": "Cancel a timer or reminder.",
                "parameters": {"type": "object", "properties": {
                    "label": {"type": "string", "description": "Timer label or words from the reminder; omit if only one is active"},
                    "all": {"type": "boolean", "description": "Cancel all timers and reminders"}
                }}
            }},
            {"type": "function", "function": {
                "name": "status_report",
                "description": "Current computer status: battery, CPU load and temperature, memory, disk usage, uptime.",
                "parameters": {"type": "object", "properties": {}}
            }},
            {"type": "function", "function": {
                "name": "set_volume",
                "description": "Change the speaker volume. Use level for an absolute value, change for louder/quieter, or mute.",
                "parameters": {"type": "object", "properties": {
                    "level": {"type": "number", "description": "Volume percent, 0 to 100"},
                    "change": {"type": "number", "description": "Relative change in percent, e.g. 10 for louder, -10 for quieter"},
                    "mute": {"type": "boolean", "description": "true to mute, false to unmute"}
                }}
            }},
            {"type": "function", "function": {
                "name": "start_captains_log",
                "description": "Start recording a captain's log entry. Everything the user says next is written to the log until they say 'end log'.",
                "parameters": {"type": "object", "properties": {}}
            }},
            {"type": "function", "function": {
                "name": "read_captains_log",
                "description": "Read back the captain's log for a day.",
                "parameters": {"type": "object", "properties": {
                    "date": {"type": "string", "description": "YYYY-MM-DD; omit for today"}
                }}
            }}
        ])
    }

    pub async fn execute(&self, name: &str, args: &Value) -> ToolOutput {
        info!("Tool call: {} {}", name, args);
        let result = match name {
            "set_timer" => self.set_timer(args).await,
            "set_reminder" => self.set_reminder(args).await,
            "list_timers" => Ok(self.list_timers().await),
            "cancel_timer" => self.cancel_timer(args).await,
            "status_report" => status_report().await,
            "set_volume" => set_volume(args).await,
            "start_captains_log" => self.start_captains_log().await,
            "read_captains_log" => read_captains_log(&self.inner.data_dir, args).await,
            _ => Err(anyhow::anyhow!("unknown tool {}", name)),
        };
        result.unwrap_or_else(|e| {
            warn!("Tool {} failed: {:#}", name, e);
            ToolOutput::data(format!("Error: {:#}", e))
        })
    }

    async fn set_timer(&self, args: &Value) -> Result<ToolOutput> {
        let seconds = num(args, "hours") * 3600.0 + num(args, "minutes") * 60.0 + num(args, "seconds");
        if seconds < 1.0 {
            bail!("timer duration must be positive");
        }
        let label = str_arg(args, "label").unwrap_or_default();
        let due = Local::now() + chrono::Duration::seconds(seconds.round() as i64);
        Inner::schedule(&self.inner, Kind::Timer, label.clone(), due).await;

        let duration = describe_duration(seconds.round() as u64);
        Ok(ToolOutput::speak(if label.is_empty() {
            format!("Timer set for {}.", duration)
        } else {
            format!("Timer set for {}: {}.", duration, label)
        }))
    }

    async fn set_reminder(&self, args: &Value) -> Result<ToolOutput> {
        let message = str_arg(args, "message").context("reminder message is required")?;
        let now = Local::now();

        let due = if let Some(minutes) = args.get("in_minutes").and_then(Value::as_f64) {
            now + chrono::Duration::seconds((minutes * 60.0).round() as i64)
        } else {
            let time_str = str_arg(args, "time").context("give either time or in_minutes")?;
            let time = NaiveTime::parse_from_str(&time_str, "%H:%M")
                .or_else(|_| NaiveTime::parse_from_str(&time_str, "%H:%M:%S"))
                .with_context(|| format!("invalid time '{}', expected HH:MM", time_str))?;
            let date = match str_arg(args, "date") {
                Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                    .with_context(|| format!("invalid date '{}', expected YYYY-MM-DD", d))?,
                None => now.date_naive(),
            };
            let mut due = Local
                .from_local_datetime(&date.and_time(time))
                .earliest()
                .context("time does not exist in the local time zone")?;
            if due <= now && str_arg(args, "date").is_none() {
                due += chrono::Duration::days(1);
            }
            if due <= now {
                bail!("that time is in the past");
            }
            due
        };

        Inner::schedule(&self.inner, Kind::Reminder, message.clone(), due).await;
        Ok(ToolOutput::speak(format!("Reminder set for {}: {}.", describe_when(due), message)))
    }

    async fn list_timers(&self) -> ToolOutput {
        let scheduled = self.inner.scheduled.lock().await;
        if scheduled.is_empty() {
            return ToolOutput::speak("No active timers or reminders.");
        }
        let now = Local::now();
        let items: Vec<String> = scheduled
            .iter()
            .map(|s| match s.kind {
                Kind::Timer => {
                    let remaining = describe_duration((s.due - now).num_seconds().max(0) as u64);
                    if s.label.is_empty() {
                        format!("Timer, {} remaining", remaining)
                    } else {
                        format!("Timer for {}, {} remaining", s.label, remaining)
                    }
                }
                Kind::Reminder => format!("Reminder {}: {}", describe_when(s.due), s.label),
            })
            .collect();
        ToolOutput::speak(format!("{}.", items.join(". ")))
    }

    async fn cancel_timer(&self, args: &Value) -> Result<ToolOutput> {
        let mut scheduled = self.inner.scheduled.lock().await;
        if scheduled.is_empty() {
            return Ok(ToolOutput::speak("No active timers or reminders."));
        }

        let cancel_all = args.get("all").and_then(Value::as_bool).unwrap_or(false);
        let label = str_arg(args, "label").map(|l| l.to_lowercase());
        let selected: Vec<u64> = if cancel_all {
            scheduled.iter().map(|s| s.id).collect()
        } else if let Some(label) = &label {
            scheduled.iter().filter(|s| s.label.to_lowercase().contains(label.as_str())).map(|s| s.id).collect()
        } else if scheduled.len() == 1 {
            vec![scheduled[0].id]
        } else {
            let labels: Vec<&str> = scheduled.iter().map(|s| s.label.as_str()).collect();
            return Ok(ToolOutput::data(format!(
                "Several are active ({}); ask the user which one to cancel.",
                labels.join(", ")
            )));
        };

        if selected.is_empty() {
            return Ok(ToolOutput::speak("No matching timer or reminder."));
        }

        let mut reminders_changed = false;
        scheduled.retain(|s| {
            if selected.contains(&s.id) {
                s.task.abort();
                reminders_changed |= s.kind == Kind::Reminder;
                false
            } else {
                true
            }
        });
        if reminders_changed {
            self.inner.save_reminders(&scheduled).await;
        }

        Ok(ToolOutput::speak(match selected.len() {
            1 => "Cancelled.".to_string(),
            n => format!("{} cancelled.", n),
        }))
    }

    async fn start_captains_log(&self) -> Result<ToolOutput> {
        let mut log = self.inner.log.lock().await;
        if log.is_some() {
            return Ok(ToolOutput::speak("Log already recording."));
        }

        let now = Local::now();
        let dir = self.inner.data_dir.join("captains_log");
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join(format!("{}.md", now.format("%Y-%m-%d")));
        let start_len = tokio::fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0);

        let mut header = String::new();
        if start_len == 0 {
            header.push_str(&format!("# Captain's log, {}\n", now.format("%A %-d %B %Y")));
        }
        header.push_str(&format!("\n## Stardate {}, {}\n\n", stardate(now), now.format("%H:%M")));
        append_to_file(&path, &header).await?;

        *log = Some(LogSession { path, start_len, words: 0, last_activity: Instant::now() });
        info!("Captain's log recording started");

        // Close a forgotten recording after a period of silence
        let inner = self.inner.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let idle = match inner.log.lock().await.as_ref() {
                    Some(session) => session.last_activity.elapsed() >= LOG_IDLE_TIMEOUT,
                    None => return,
                };
                if idle {
                    let speech = inner.end_log().await;
                    let _ = inner.command_tx.send(AssistantCommand::SpeakText(format!(
                        "Recording stopped after one minute of silence. {}",
                        speech
                    )));
                    return;
                }
            }
        });

        Ok(ToolOutput::speak(format!(
            "Captain's log, stardate {}. Recording. Say end log when finished.",
            stardate(now)
        )))
    }

    pub async fn is_logging(&self) -> bool {
        self.inner.log.lock().await.is_some()
    }

    /// Add dictated text to the captain's log. Returns what to say if the recording ended.
    pub async fn log_dictation(&self, text: &str) -> Result<Option<String>> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let normalized: Vec<String> = words
            .iter()
            .map(|w| w.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect())
            .collect();
        // Only a phrase at the end of the utterance ends the log, so dictation isn't cut short
        let end = LOG_END_PHRASES.iter().find_map(|phrase| {
            let phrase: Vec<&str> = phrase.split(' ').collect();
            let start = normalized.len().checked_sub(phrase.len())?;
            (normalized[start..] == phrase[..]).then_some(start)
        });
        let content = words[..end.unwrap_or(words.len())].join(" ");
        let content = content.trim().trim_end_matches([',', '.']).trim();

        {
            let mut log = self.inner.log.lock().await;
            let session = log.as_mut().context("captain's log is not recording")?;
            session.last_activity = Instant::now();
            if !content.is_empty() {
                let mut entry = content.to_string();
                if !entry.ends_with(['.', '!', '?']) {
                    entry.push('.');
                }
                entry.push(' ');
                append_to_file(&session.path, &entry).await?;
                session.words += content.split_whitespace().count();
            }
        }

        Ok(match end {
            Some(_) => Some(self.inner.end_log().await),
            None => None,
        })
    }
}

impl Inner {
    fn reminders_path(&self) -> PathBuf {
        self.data_dir.join("reminders.json")
    }

    async fn schedule(inner: &Arc<Inner>, kind: Kind, label: String, due: DateTime<Local>) {
        let id = inner.next_id.fetch_add(1, Ordering::SeqCst);
        // Hold the lock while spawning so the task can't fire before it's registered
        let mut scheduled = inner.scheduled.lock().await;

        let task_inner = inner.clone();
        let task_label = label.clone();
        let task = tokio::spawn(async move {
            // Re-check the wall clock regularly: monotonic timers stop while the laptop sleeps
            loop {
                let remaining = (due - Local::now()).num_milliseconds();
                if remaining <= 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(remaining.min(30_000) as u64)).await;
            }
            task_inner.fire(id, kind, &task_label).await;
        });

        scheduled.push(Scheduled { id, kind, label, due, task });
        if kind == Kind::Reminder {
            inner.save_reminders(&scheduled).await;
        }
    }

    async fn fire(&self, id: u64, kind: Kind, label: &str) {
        {
            let mut scheduled = self.scheduled.lock().await;
            scheduled.retain(|s| s.id != id);
            if kind == Kind::Reminder {
                self.save_reminders(&scheduled).await;
            }
        }

        let text = match kind {
            Kind::Timer if label.is_empty() => "Timer complete.".to_string(),
            Kind::Timer => format!("Timer complete: {}.", label),
            Kind::Reminder => format!("Reminder: {}.", label),
        };
        info!("{}", text);
        let _ = Command::new("notify-send")
            .args(["--app-name=Star Trek Computer", "Computer", &text])
            .spawn();
        let _ = self.command_tx.send(AssistantCommand::SpeakText(text));
    }

    async fn save_reminders(&self, scheduled: &[Scheduled]) {
        let reminders: Vec<SavedReminder> = scheduled
            .iter()
            .filter(|s| s.kind == Kind::Reminder)
            .map(|s| SavedReminder { due: s.due, message: s.label.clone() })
            .collect();
        let result = match serde_json::to_string_pretty(&reminders) {
            Ok(json) => tokio::fs::write(self.reminders_path(), json).await.map_err(anyhow::Error::from),
            Err(e) => Err(e.into()),
        };
        if let Err(e) = result {
            warn!("Failed to save reminders: {}", e);
        }
    }

    /// Finish the current captain's log recording and describe the result.
    async fn end_log(&self) -> String {
        let Some(session) = self.log.lock().await.take() else {
            return String::new();
        };
        info!("Captain's log recording ended ({} words)", session.words);

        if session.words == 0 {
            // Remove the empty session header (and the file if it was created for it)
            let result = if session.start_len == 0 {
                tokio::fs::remove_file(&session.path).await
            } else {
                match std::fs::OpenOptions::new().write(true).open(&session.path) {
                    Ok(file) => file.set_len(session.start_len),
                    Err(e) => Err(e),
                }
            };
            if let Err(e) = result {
                warn!("Failed to remove empty log entry: {}", e);
            }
            return "Nothing recorded.".to_string();
        }

        if let Err(e) = append_to_file(&session.path, "\n").await {
            warn!("Failed to finish log entry: {}", e);
        }
        format!("Log entry recorded, {} words.", session.words)
    }
}

async fn read_captains_log(data_dir: &std::path::Path, args: &Value) -> Result<ToolOutput> {
    let date = match str_arg(args, "date") {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .with_context(|| format!("invalid date '{}', expected YYYY-MM-DD", d))?,
        None => Local::now().date_naive(),
    };
    let path = data_dir.join("captains_log").join(format!("{}.md", date.format("%Y-%m-%d")));
    let day = date.format("%A %-d %B").to_string();

    let Ok(content) = tokio::fs::read_to_string(&path).await else {
        return Ok(ToolOutput::speak(format!("No log entries for {}.", day)));
    };

    // Turn "## Stardate 2026.287, 13:45" headings into spoken introductions
    let mut speech = String::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(heading) = line.strip_prefix("## ") {
            speech.push_str(&format!("Captain's log, {}. ", heading.replace(", ", ", at ")));
        } else if !line.is_empty() && !line.starts_with('#') {
            speech.push_str(line);
            speech.push(' ');
        }
    }
    if speech.is_empty() {
        return Ok(ToolOutput::speak(format!("No log entries for {}.", day)));
    }
    Ok(ToolOutput::speak(speech.trim().to_string()))
}

async fn status_report() -> Result<ToolOutput> {
    let mut parts = Vec::new();

    // Battery
    let mut batteries = tokio::fs::read_dir("/sys/class/power_supply").await?;
    while let Some(entry) = batteries.next_entry().await? {
        let path = entry.path();
        if !entry.file_name().to_string_lossy().starts_with("BAT") {
            continue;
        }
        let capacity = tokio::fs::read_to_string(path.join("capacity")).await.unwrap_or_default();
        let status = tokio::fs::read_to_string(path.join("status")).await.unwrap_or_default();
        parts.push(format!("Battery {}% ({})", capacity.trim(), status.trim().to_lowercase()));
    }

    // CPU load (1-minute average as a share of all cores) and temperature
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
    if let Ok(loadavg) = tokio::fs::read_to_string("/proc/loadavg").await {
        if let Some(load) = loadavg.split_whitespace().next().and_then(|l| l.parse::<f64>().ok()) {
            parts.push(format!("CPU load {:.0}%", (load / cores * 100.0).min(100.0)));
        }
    }
    if let Some(temp) = cpu_temperature().await {
        parts.push(format!("CPU temperature {:.0} degrees Celsius", temp));
    }

    // Memory
    if let Ok(meminfo) = tokio::fs::read_to_string("/proc/meminfo").await {
        let field = |name: &str| {
            meminfo
                .lines()
                .find(|l| l.starts_with(name))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<f64>().ok())
        };
        if let (Some(total), Some(available)) = (field("MemTotal:"), field("MemAvailable:")) {
            parts.push(format!(
                "Memory {:.0}% used of {:.0} GB",
                (1.0 - available / total) * 100.0,
                total / 1024.0 / 1024.0
            ));
        }
    }

    // Disks
    let df = Command::new("df").args(["-P", "/", "/home"]).output().await?;
    let mut seen = Vec::new();
    for line in String::from_utf8_lossy(&df.stdout).lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 6 && !seen.contains(&cols[0]) {
            seen.push(cols[0]);
            parts.push(format!("Disk {} {} full", cols[5], cols[4]));
        }
    }

    // Uptime
    if let Ok(uptime) = tokio::fs::read_to_string("/proc/uptime").await {
        if let Some(secs) = uptime.split_whitespace().next().and_then(|s| s.parse::<f64>().ok()) {
            parts.push(format!("Uptime {}", describe_duration((secs as u64 / 60) * 60)));
        }
    }

    Ok(ToolOutput::data(parts.join(". ")))
}

async fn cpu_temperature() -> Option<f64> {
    let mut zones = tokio::fs::read_dir("/sys/class/thermal").await.ok()?;
    while let Ok(Some(zone)) = zones.next_entry().await {
        let kind = tokio::fs::read_to_string(zone.path().join("type")).await.unwrap_or_default();
        if kind.trim() == "x86_pkg_temp" {
            let millidegrees = tokio::fs::read_to_string(zone.path().join("temp")).await.ok()?;
            return millidegrees.trim().parse::<f64>().ok().map(|t| t / 1000.0);
        }
    }
    None
}

async fn set_volume(args: &Value) -> Result<ToolOutput> {
    let level = args.get("level").and_then(Value::as_f64);
    let change = args.get("change").and_then(Value::as_f64);
    let mute = args.get("mute").and_then(Value::as_bool);
    let mixer = Mixer::detect();

    if let Some(level) = level {
        mixer.set_percent(level.clamp(0.0, 100.0)).await?;
    } else if let Some(change) = change {
        let (current, _) = mixer.get().await?;
        mixer.set_percent((current + change).clamp(0.0, 100.0)).await?;
    } else if mute.is_none() {
        bail!("give level, change or mute");
    }

    // Changing the volume implies the user wants to hear it
    let mute = mute.or(if level.is_some() || change.is_some() { Some(false) } else { None });
    if let Some(mute) = mute {
        mixer.set_mute(mute).await?;
    }

    let (percent, muted) = mixer.get().await?;
    if muted {
        return Ok(ToolOutput::speak("Audio muted."));
    }
    Ok(ToolOutput::speak(format!("Volume {} percent.", percent.round())))
}

/// Volume control for the default output: wpctl on PipeWire, pactl otherwise
/// (PulseAudio, or PipeWire without WirePlumber's CLI).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mixer {
    Wpctl,
    Pactl,
}

impl Mixer {
    fn detect() -> Self {
        if on_path("wpctl") { Mixer::Wpctl } else { Mixer::Pactl }
    }

    /// Current volume in percent and whether the output is muted
    async fn get(self) -> Result<(f64, bool)> {
        match self {
            Mixer::Wpctl => parse_wpctl_volume(&mixer_cmd("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]).await?),
            Mixer::Pactl => {
                let volume = mixer_cmd("pactl", &["get-sink-volume", "@DEFAULT_SINK@"]).await?;
                let mute = mixer_cmd("pactl", &["get-sink-mute", "@DEFAULT_SINK@"]).await?;
                Ok((parse_pactl_volume(&volume)?, mute.contains("yes")))
            }
        }
    }

    async fn set_percent(self, percent: f64) -> Result<()> {
        let percent = format!("{:.0}%", percent);
        match self {
            Mixer::Wpctl => mixer_cmd("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &percent]).await,
            Mixer::Pactl => mixer_cmd("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &percent]).await,
        }
        .map(|_| ())
    }

    async fn set_mute(self, mute: bool) -> Result<()> {
        let flag = if mute { "1" } else { "0" };
        match self {
            Mixer::Wpctl => mixer_cmd("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", flag]).await,
            Mixer::Pactl => mixer_cmd("pactl", &["set-sink-mute", "@DEFAULT_SINK@", flag]).await,
        }
        .map(|_| ())
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

async fn mixer_cmd(program: &str, args: &[&str]) -> Result<String> {
    // pactl translates its output ("Mute: oui"); keep it parseable
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .await
        .with_context(|| format!("failed to run {}", program))?;
    if !output.status.success() {
        bail!("{} {} failed: {}", program, args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// "Volume: 0.40" or "Volume: 0.40 [MUTED]"
fn parse_wpctl_volume(output: &str) -> Result<(f64, bool)> {
    let volume = output
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<f64>().ok())
        .context("could not read the volume")?;
    Ok((volume * 100.0, output.contains("MUTED")))
}

/// "Volume: front-left: 41944 /  64% / -11.63 dB,   front-right: ..." (first channel)
fn parse_pactl_volume(output: &str) -> Result<f64> {
    output
        .split_whitespace()
        .find_map(|word| word.strip_suffix('%')?.parse::<f64>().ok())
        .context("could not read the volume")
}

/// Year and day of year, in the style of the Star Trek (2009) films: 2026.287.
pub fn stardate(now: DateTime<Local>) -> String {
    format!("{}.{:03}", now.year(), now.ordinal())
}

async fn append_to_file(path: &std::path::Path, text: &str) -> Result<()> {
    let mut file = tokio::fs::OpenOptions::new().create(true).append(true).open(path).await?;
    file.write_all(text.as_bytes()).await?;
    Ok(())
}

fn num(args: &Value, key: &str) -> f64 {
    match args.get(key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn describe_duration(total_seconds: u64) -> String {
    let (h, m, s) = (total_seconds / 3600, total_seconds % 3600 / 60, total_seconds % 60);
    let unit = |n: u64, name: &str| format!("{} {}{}", n, name, if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if h > 0 { parts.push(unit(h, "hour")); }
    if m > 0 { parts.push(unit(m, "minute")); }
    if s > 0 || parts.is_empty() { parts.push(unit(s, "second")); }
    parts.join(" ")
}

fn describe_when(due: DateTime<Local>) -> String {
    let today = Local::now().date_naive();
    let day = match (due.date_naive() - today).num_days() {
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        2..=6 => due.format("%A").to_string(),
        _ => due.format("%A %-d %B").to_string(),
    };
    format!("{} {}", due.format("%H:%M"), day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wpctl_volume() {
        assert_eq!(parse_wpctl_volume("Volume: 0.40").unwrap(), (40.0, false));
        assert_eq!(parse_wpctl_volume("Volume: 0.40 [MUTED]").unwrap(), (40.0, true));
    }

    #[test]
    fn parses_pactl_volume() {
        let output = "Volume: front-left: 41944 /  64% / -11.63 dB,   front-right: 41944 /  64% / -11.63 dB\n        balance 0.00";
        assert_eq!(parse_pactl_volume(output).unwrap(), 64.0);
        assert!(parse_pactl_volume("garbage").is_err());
    }
}
