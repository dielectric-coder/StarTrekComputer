//! Read-only access to Home Assistant through its REST API.
//!
//! Several addresses can be configured (e.g. the home network, then Tailscale for when the
//! laptop is away); they are tried in order and the one that last answered is tried first.
//!
//! The entities the user exposed to voice assistants in Home Assistant (Settings > Voice
//! assistants > Expose), with their aliases, are the only ones the computer looks at.

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use chrono::Datelike;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};
use crate::config::{AssistantConfig, HomeAssistantConfig};

/// How many entities a single answer mentions at most.
const MAX_MATCHES: usize = 6;
/// How long the list of exposed entities is trusted before it is fetched again.
const EXPOSED_TTL: Duration = Duration::from_secs(300);
/// Longest catalogue of entity names put into the LLM's tool description.
const MAX_CATALOGUE_CHARS: usize = 3000;

/// Entity kinds that never hold a reading worth speaking.
const IGNORED_DOMAINS: &[&str] = &[
    "automation", "button", "conversation", "event", "image", "input_button", "scene",
    "script", "stt", "tag", "tts", "update", "wake_word", "zone",
];

/// Words in a question that say nothing about which entity is meant.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "at", "current", "currently", "does", "for", "give", "how", "in",
    "is", "it", "me", "much", "my", "now", "of", "on", "please", "reading", "right", "say",
    "show", "state", "status", "tell", "the", "there", "to", "value", "what", "whats", "which",
    "from", "sensor", "network", "data", "level", "have", "you", "access", "get", "do",
];

/// Different words people and integrations use for the same thing.
const SYNONYMS: &[&[&str]] = &[
    &["temperature", "temp"],
    &["humidity", "humid", "humidite"],
    &["pressure", "pression", "barometric", "baro"],
    &["door", "porte"],
    &["window", "fenetre"],
    &["outside", "outdoor", "exterior"],
    &["inside", "indoor", "interior"],
    &["power", "consumption", "watt"],
    &["light", "lamp"],
];

#[derive(Debug, Clone, Deserialize)]
pub struct EntityState {
    pub entity_id: String,
    pub state: String,
    #[serde(default)]
    pub attributes: Value,
}

impl EntityState {
    fn domain(&self) -> &str {
        self.entity_id.split('.').next().unwrap_or("")
    }

    fn name(&self) -> String {
        self.attributes["friendly_name"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| self.entity_id.split('.').nth(1).unwrap_or("").replace('_', " "))
    }

    /// The reading as it should be spoken, e.g. "21.5 degrees Celsius" or "open".
    fn describe(&self) -> String {
        let state = self.state.as_str();
        if matches!(state, "unavailable" | "unknown") {
            return state.to_string();
        }
        let attr = |key: &str| self.attributes.get(key).filter(|v| !v.is_null());
        let class = self.attributes["device_class"].as_str().unwrap_or("");

        match self.domain() {
            "binary_sensor" => binary_state(class, state == "on").to_string(),
            "climate" => {
                let mut text = state.to_string();
                if let Some(current) = attr("current_temperature") {
                    text.push_str(&format!(", currently {} degrees", plain(current)));
                }
                if let Some(target) = attr("temperature") {
                    text.push_str(&format!(", set to {} degrees", plain(target)));
                }
                text
            }
            "weather" => {
                let mut text = state.replace("partlycloudy", "partly cloudy").replace('-', " ");
                if let Some(temp) = attr("temperature") {
                    let unit = self.attributes["temperature_unit"].as_str().unwrap_or("");
                    text.push_str(&format!(", {} {}", plain(temp), speak_unit(unit)).trim_end().to_string());
                }
                if let Some(humidity) = attr("humidity") {
                    text.push_str(&format!(", humidity {} percent", plain(humidity)));
                }
                text
            }
            _ if class == "timestamp" => speak_time(state),
            _ => {
                let unit = self.attributes["unit_of_measurement"].as_str().unwrap_or("");
                let value = state.parse::<f64>().map(round_reading).unwrap_or_else(|_| state.replace('_', " "));
                format!("{} {}", value, speak_unit(unit)).trim_end().to_string()
            }
        }
    }
}

/// "today at 14:05", "yesterday at 09:30" or "8 October at 20:31", in local time.
fn speak_time(state: &str) -> String {
    let Ok(when) = chrono::DateTime::parse_from_rfc3339(state) else {
        return state.to_string();
    };
    let when = when.with_timezone(&chrono::Local);
    let today = chrono::Local::now().date_naive();
    let day = match (today - when.date_naive()).num_days() {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        -1 => "tomorrow".to_string(),
        _ if when.date_naive().year_ce() == today.year_ce() => when.format("%-d %B").to_string(),
        _ => when.format("%-d %B %Y").to_string(),
    };
    format!("{} at {}", day, when.format("%H:%M"))
}

fn plain(value: &Value) -> String {
    match value {
        Value::Number(n) => n.as_f64().map(round_reading).unwrap_or_else(|| n.to_string()),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Sensors often report more decimals than anyone wants to hear.
fn round_reading(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{:.1}", rounded)
    }
}

fn speak_unit(unit: &str) -> &str {
    match unit {
        "°C" => "degrees Celsius",
        "°F" => "degrees Fahrenheit",
        "°" => "degrees",
        "W/m²" => "watts per square metre",
        "mm/h" => "millimetres per hour",
        "%" => "percent",
        "W" => "watts",
        "kW" => "kilowatts",
        "kWh" => "kilowatt hours",
        "V" => "volts",
        "A" => "amps",
        "hPa" => "hectopascals",
        "lx" => "lux",
        "km/h" => "kilometres per hour",
        "mm" => "millimetres",
        "ppm" => "parts per million",
        "µg/m³" => "micrograms per cubic metre",
        other => other,
    }
}

fn binary_state(device_class: &str, on: bool) -> &'static str {
    match (device_class, on) {
        ("door" | "garage_door" | "window" | "opening", true) => "open",
        ("door" | "garage_door" | "window" | "opening", false) => "closed",
        ("lock", true) => "unlocked",
        ("lock", false) => "locked",
        ("motion" | "occupancy" | "presence" | "smoke" | "gas" | "carbon_monoxide" | "sound" | "vibration", true) => "detected",
        ("motion" | "occupancy" | "presence" | "smoke" | "gas" | "carbon_monoxide" | "sound" | "vibration", false) => "clear",
        ("moisture", true) => "wet",
        ("moisture", false) => "dry",
        ("battery", true) => "low",
        ("battery", false) => "normal",
        ("connectivity", true) => "connected",
        ("connectivity", false) => "disconnected",
        ("plug" | "power", true) => "plugged in",
        ("plug" | "power", false) => "unplugged",
        (_, true) => "on",
        (_, false) => "off",
    }
}

/// Lowercase without accents, so French names ("Température") match English questions.
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    fold(text)
        .replace('\'', "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            // Plural and singular should match ("doors" asks about a door)
            // ("temps" is French for time, not a plural of temp)
            if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") && w != "temps" { w[..w.len() - 1].to_string() } else { w.to_string() }
        })
        .collect()
}

/// Whisper's usual mishearings of sensor words.
fn heard_right(query: &str) -> String {
    let mut query = fold(query);
    for (heard, meant) in [("do point", "dew point"), ("due point", "dew point"), ("jew point", "dew point")] {
        query = query.replace(heard, meant);
    }
    query
}

fn same_word(a: &str, b: &str) -> bool {
    a == b || SYNONYMS.iter().any(|group| group.contains(&a) && group.contains(&b))
}

/// The entities a spoken question is most likely about, best first.
/// `aliases` holds the alternative names given in Home Assistant, by entity id.
pub fn best_matches<'a>(query: &str, states: &'a [EntityState], aliases: &HashMap<String, Vec<String>>) -> Vec<&'a EntityState> {
    let spoken = words(&heard_right(query)).join(" ");
    let wanted: Vec<String> = words(&heard_right(query)).into_iter().filter(|w| !STOP_WORDS.contains(&w.as_str())).collect();
    if wanted.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<(usize, usize, &EntityState)> = states
        .iter()
        .filter(|s| !IGNORED_DOMAINS.contains(&s.domain()))
        .filter_map(|s| {
            let mut name_words = words(&s.name());
            name_words.extend(words(&s.entity_id.replace('_', " ")));
            let own_aliases = aliases.get(&s.entity_id).map(Vec::as_slice).unwrap_or_default();
            for alias in own_aliases {
                name_words.extend(words(alias));
            }
            // Saying an alias in full is the clearest way to name an entity
            let alias_bonus: usize = own_aliases
                .iter()
                .map(|a| words(a).join(" "))
                .filter(|a| !a.is_empty() && format!(" {} ", spoken).contains(&format!(" {} ", a)))
                .map(|a| 10 * a.split(' ').count())
                .max()
                .unwrap_or(0);
            let class_words = words(s.attributes["device_class"].as_str().unwrap_or(""));
            let in_name = |w: &str| name_words.iter().any(|n| same_word(w, n));
            let in_class = |w: &str| class_words.iter().any(|n| same_word(w, n));
            // A word in the name counts more than one only in the device class: "temperature"
            // means the temperature sensors, not the dew point that is measured in degrees too
            let mut score: Vec<usize> = wanted
                .iter()
                .map(|w| if in_name(w) { 2 } else if in_class(w) { 1 } else { 0 })
                .collect();
            // Spoken words can be one word in the name: "dew point" is "Dewpoint"
            for i in 1..wanted.len() {
                if in_name(&format!("{}{}", wanted[i - 1], wanted[i])) {
                    score[i - 1] = 2;
                    score[i] = 2;
                }
            }
            let hits: usize = score.iter().sum::<usize>() + alias_bonus;
            (hits > 0).then(|| (hits, words(&s.name()).len(), s))
        })
        .collect();

    let Some(top) = scored.iter().map(|(hits, _, _)| *hits).max() else {
        return Vec::new();
    };
    scored.retain(|(hits, _, _)| *hits == top);
    // Among the best matches, a dead sensor shouldn't crowd out working ones
    let working = |s: &EntityState| !matches!(s.state.as_str(), "unavailable" | "unknown");
    if scored.iter().any(|(_, _, s)| working(s)) {
        scored.retain(|(_, _, s)| working(s));
    }
    // Among equally good matches, readings first, then the most specific (shortest) names
    scored.sort_by_key(|(_, len, s)| (!matches!(s.domain(), "sensor" | "binary_sensor" | "climate" | "weather"), *len));
    scored.into_iter().take(MAX_MATCHES).map(|(_, _, s)| s).collect()
}

pub struct HomeAssistant {
    client: reqwest::Client,
    urls: Vec<String>,
    token: String,
    /// Index into `urls` of the address that answered last.
    preferred: AtomicUsize,
    exposed: tokio::sync::Mutex<Option<(Instant, Arc<Exposed>)>>,
    /// Names of the exposed entities, for the LLM's tool description.
    catalogue: std::sync::Mutex<String>,
}

/// What the user chose to show voice assistants in Home Assistant.
#[derive(Default)]
pub struct Exposed {
    /// Alternative names, by entity id; every exposed entity has an entry.
    aliases: HashMap<String, Vec<String>>,
    /// Short names ("Weekly Rain", not "Station Météo Mike's Lab Weekly Rain"), by entity id.
    names: HashMap<String, String>,
}

impl Exposed {
    /// Exposed entities grouped by device, e.g. "Station Météo: Dewpoint (Point de rosée),
    /// Wind Speed; Table de chevet Michel: Relais Sonoff B R2". `states` gives the full
    /// names, whose start is the device name; short names alone are often ambiguous.
    fn catalogue(&self, states: &[EntityState]) -> String {
        let mut devices: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for state in states {
            let Some(short) = self.names.get(&state.entity_id) else { continue };
            if IGNORED_DOMAINS.contains(&state.domain()) {
                continue;
            }
            let full = state.name();
            let device = full
                .strip_suffix(short.as_str())
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .unwrap_or("")
                .to_string();
            let entry = match self.aliases.get(&state.entity_id).filter(|a| !a.is_empty()) {
                Some(aliases) => format!("{} ({})", short, aliases.join(", ")),
                None => short.clone(),
            };
            devices.entry(device).or_default().push(entry);
        }

        let mut catalogue = String::new();
        for (device, mut entries) in devices {
            entries.sort();
            let group = if device.is_empty() {
                entries.join(", ")
            } else {
                format!("{}: {}", device, entries.join(", "))
            };
            if catalogue.len() + group.len() > MAX_CATALOGUE_CHARS {
                catalogue.push_str("; and more");
                break;
            }
            if !catalogue.is_empty() {
                catalogue.push_str("; ");
            }
            catalogue.push_str(&group);
        }
        catalogue
    }
}

impl HomeAssistant {
    /// None when Home Assistant isn't configured.
    pub fn from_config(config: &AssistantConfig) -> Result<Option<Self>> {
        let Some(ha) = &config.home_assistant else {
            return Ok(None);
        };
        Self::new(ha, config).map(Some)
    }

    fn new(ha: &HomeAssistantConfig, config: &AssistantConfig) -> Result<Self> {
        if ha.urls.is_empty() {
            bail!("home_assistant.urls is empty");
        }
        let token = match std::env::var("HASS_TOKEN") {
            Ok(token) if !token.trim().is_empty() => token,
            _ => {
                let path = ha
                    .token_file
                    .clone()
                    .unwrap_or_else(|| config.general.data_dir.join("home_assistant_token"));
                std::fs::read_to_string(&path).with_context(|| {
                    format!("could not read the Home Assistant token from {}", path.display())
                })?
            }
        };
        let client = reqwest::Client::builder()
            // An address that isn't reachable from here (home LAN while away) must fail fast
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(8))
            .build()?;
        info!("Home Assistant configured at {}", ha.urls.join(", "));
        Ok(Self {
            client,
            urls: ha.urls.iter().map(|u| u.trim_end_matches('/').to_string()).collect(),
            token: token.trim().to_string(),
            preferred: AtomicUsize::new(0),
            exposed: tokio::sync::Mutex::new(None),
            catalogue: std::sync::Mutex::new(String::new()),
        })
    }

    /// Find a reachable address and read the exposed entities ahead of the first question.
    pub async fn warm_up(&self) -> Result<()> {
        let states = self.states().await?;
        if let Some(exposed) = self.exposed().await {
            self.update_catalogue(&exposed, &states);
        }
        Ok(())
    }

    fn update_catalogue(&self, exposed: &Exposed, states: &[EntityState]) {
        if let Ok(mut catalogue) = self.catalogue.lock() {
            *catalogue = exposed.catalogue(states);
        }
    }

    /// Names of the exposed entities, empty until they have been fetched.
    pub fn catalogue(&self) -> String {
        self.catalogue.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// The exposed entities, fetched again when the cached list is old.
    /// None when they can't be read (then every entity is considered).
    pub async fn exposed(&self) -> Option<Arc<Exposed>> {
        let mut cached = self.exposed.lock().await;
        if let Some((fetched, exposed)) = cached.as_ref() {
            if fetched.elapsed() < EXPOSED_TTL {
                return Some(exposed.clone());
            }
        }
        match self.fetch_exposed().await {
            Ok(exposed) => {
                info!("Home Assistant: {} entities exposed to voice assistants", exposed.names.len());
                let exposed = Arc::new(exposed);
                *cached = Some((Instant::now(), exposed.clone()));
                Some(exposed)
            }
            Err(e) => {
                warn!("Could not read the exposed Home Assistant entities: {:#}", e);
                // An older list is better than none
                cached.as_ref().map(|(_, exposed)| exposed.clone())
            }
        }
    }

    async fn fetch_exposed(&self) -> Result<Exposed> {
        let base = &self.urls[self.preferred.load(Ordering::Relaxed)];
        let ws_url = format!("{}/api/websocket", base.replacen("http", "ws", 1));
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(&ws_url))
            .await
            .context("timed out connecting")??;

        let mut next_id = 0;
        let mut call = |message: Value| {
            next_id += 1;
            let mut message = message;
            message["id"] = json!(next_id);
            (next_id, message)
        };

        expect_message(&mut socket, "auth_required").await?;
        socket.send(Message::text(json!({"type": "auth", "access_token": self.token}).to_string())).await?;
        let auth = expect_message(&mut socket, "auth_ok").await.context("access token rejected")?;
        debug!("Home Assistant WebSocket: {}", auth);

        let (id, request) = call(json!({"type": "homeassistant/expose_entity/list"}));
        let listed = request_result(&mut socket, id, request).await?;
        let exposed_ids: Vec<String> = listed["exposed_entities"]
            .as_object()
            .context("no exposed_entities in the reply")?
            .iter()
            .filter(|(_, settings)| settings["conversation"].as_bool() == Some(true))
            .map(|(id, _)| id.clone())
            .collect();

        let (id, request) = call(json!({"type": "config/entity_registry/get_entries", "entity_ids": exposed_ids}));
        let entries = request_result(&mut socket, id, request).await?;
        let _ = socket.close(None).await;

        let mut exposed = Exposed::default();
        for entity_id in exposed_ids {
            let entry = &entries[&entity_id];
            let aliases: Vec<String> = entry["aliases"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).map(str::to_string).collect())
                .unwrap_or_default();
            let name = entry["name"]
                .as_str()
                .or_else(|| entry["original_name"].as_str())
                .map(str::to_string)
                .unwrap_or_else(|| entity_id.split('.').nth(1).unwrap_or("").replace('_', " "));
            exposed.names.insert(entity_id.clone(), name);
            exposed.aliases.insert(entity_id, aliases);
        }
        Ok(exposed)
    }

    async fn states(&self) -> Result<Vec<EntityState>> {
        let first = self.preferred.load(Ordering::Relaxed);
        let order = std::iter::once(first).chain((0..self.urls.len()).filter(|&i| i != first));
        let mut last_error = None;

        for index in order {
            let url = &self.urls[index];
            let response = match self.client.get(format!("{}/api/states", url)).bearer_auth(&self.token).send().await {
                Ok(response) => response,
                Err(e) => {
                    // Not reachable from this network; try the next address
                    warn!("Home Assistant not reachable at {}: {}", url, e);
                    last_error = Some(e);
                    continue;
                }
            };
            match response.status() {
                reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                    bail!("Home Assistant at {} rejected the access token", url)
                }
                status if !status.is_success() => bail!("Home Assistant at {} answered {}", url, status),
                _ => {}
            }
            self.preferred.store(index, Ordering::Relaxed);
            return response.json().await.context("unexpected reply from Home Assistant");
        }

        match last_error {
            Some(e) => Err(anyhow::Error::new(e).context("Home Assistant is not reachable from this network")),
            None => bail!("Home Assistant is not reachable"),
        }
    }

    /// Readings for the entities a question is about, for the LLM to phrase.
    pub async fn status(&self, query: &str) -> Result<String> {
        let mut states = self.states().await?;
        let no_aliases = HashMap::new();
        let exposed = self.exposed().await;
        if let Some(exposed) = &exposed {
            self.update_catalogue(exposed, &states);
        }
        let aliases = match &exposed {
            Some(exposed) if !exposed.aliases.is_empty() => {
                states.retain(|s| exposed.aliases.contains_key(&s.entity_id));
                &exposed.aliases
            }
            // Nothing exposed (or the list couldn't be read): look at everything
            _ => &no_aliases,
        };
        let matches = best_matches(query, &states, aliases);
        if matches.is_empty() {
            return Ok(format!("No Home Assistant entity matches '{}'.", query));
        }
        Ok(matches
            .iter()
            .map(|s| format!("{}: {}", s.name(), s.describe()))
            .collect::<Vec<_>>()
            .join(". "))
    }
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Next JSON message from Home Assistant, giving up after a few seconds.
async fn next_message(socket: &mut Socket) -> Result<Value> {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(8), socket.next())
            .await
            .context("Home Assistant stopped answering")?
            .context("Home Assistant closed the connection")??;
        if let Message::Text(text) = message {
            return Ok(serde_json::from_str(&text)?);
        }
    }
}

async fn expect_message(socket: &mut Socket, kind: &str) -> Result<Value> {
    let message = next_message(socket).await?;
    if message["type"] != kind {
        bail!("expected {} from Home Assistant, got {}", kind, message["type"]);
    }
    Ok(message)
}

async fn request_result(socket: &mut Socket, id: u64, request: Value) -> Result<Value> {
    socket.send(Message::text(request.to_string())).await?;
    loop {
        let message = next_message(socket).await?;
        if message["id"] != id {
            continue;
        }
        if message["success"] != true {
            bail!("Home Assistant refused {}: {}", request["type"], message["error"]);
        }
        return Ok(message["result"].clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entity(id: &str, state: &str, attributes: Value) -> EntityState {
        EntityState { entity_id: id.to_string(), state: state.to_string(), attributes }
    }

    fn sample() -> Vec<EntityState> {
        vec![
            entity("sensor.basement_temperature", "18.43", json!({"friendly_name": "Basement Temperature", "unit_of_measurement": "°C", "device_class": "temperature"})),
            entity("sensor.basement_humidity", "61", json!({"friendly_name": "Basement Humidity", "unit_of_measurement": "%", "device_class": "humidity"})),
            entity("sensor.outdoor_temp", "-3.2", json!({"friendly_name": "Outdoor Temp", "unit_of_measurement": "°C"})),
            entity("binary_sensor.garage_door", "on", json!({"friendly_name": "Garage Door", "device_class": "garage_door"})),
            entity("update.basement_sensor_firmware", "off", json!({"friendly_name": "Basement sensor firmware"})),
            entity("light.kitchen", "off", json!({"friendly_name": "Kitchen Lights"})),
            entity("sensor.hp2553_dewpoint", "5.4", json!({"friendly_name": "HP2553 Dewpoint", "unit_of_measurement": "°C", "device_class": "temperature"})),
            entity("sensor.humidite", "unavailable", json!({"friendly_name": "Esp32Sensor1 Humidité", "device_class": "humidity"})),
            entity("sensor.salon_temperature", "unavailable", json!({"friendly_name": "Salon Température", "device_class": "temperature"})),
        ]
    }

    fn names(query: &str, states: &[EntityState]) -> Vec<String> {
        best_matches(query, states, &HashMap::new()).iter().map(|s| s.entity_id.clone()).collect()
    }

    #[test]
    fn finds_the_entity_a_question_is_about() {
        let states = sample();
        assert_eq!(names("basement temperature", &states), ["sensor.basement_temperature"]);
        assert_eq!(names("What's the temperature outside?", &states), ["sensor.outdoor_temp"]);
        assert_eq!(names("is the garage door open", &states), ["binary_sensor.garage_door"]);
        assert_eq!(names("kitchen lamp", &states), ["light.kitchen"]);
        assert_eq!(names("basement", &states), ["sensor.basement_temperature", "sensor.basement_humidity"]);
        assert!(names("swimming pool", &states).is_empty());
        assert_eq!(names("dew point", &states), ["sensor.hp2553_dewpoint"]);
        assert_eq!(names("do point", &states), ["sensor.hp2553_dewpoint"]);
        assert!(!names("temperature", &states).contains(&"sensor.hp2553_dewpoint".to_string()));
        let french = [entity("number.porte_temps", "15", json!({"friendly_name": "Porte Temps de réponse"}))];
        assert!(names("temperature", &french).is_empty());
        assert_eq!(names("Do you have access to the dew point from the home sensor network?", &states), ["sensor.hp2553_dewpoint"]);
        // Working sensors win over dead ones; a dead one is still reported when it's all there is
        assert!(!names("humidity", &states).contains(&"sensor.humidite".to_string()));
        assert_eq!(names("salon temperature", &states), ["sensor.salon_temperature"]);
        assert!(names("what is the", &states).is_empty());
    }

    #[test]
    fn aliases_name_entities() {
        let states = sample();
        let aliases = HashMap::from([
            ("sensor.hp2553_dewpoint".to_string(), vec!["point de rosée".to_string()]),
            ("sensor.outdoor_temp".to_string(), vec!["how cold".to_string()]),
        ]);
        let names = |q: &str| -> Vec<String> { best_matches(q, &states, &aliases).iter().map(|s| s.entity_id.clone()).collect() };
        assert_eq!(names("point de rosée"), ["sensor.hp2553_dewpoint"]);
        assert_eq!(names("how cold is it"), ["sensor.outdoor_temp"]);
    }

    #[test]
    fn catalogue_groups_entities_by_device() {
        let exposed = Exposed {
            aliases: HashMap::from([
                ("sensor.a".into(), vec!["pluie semaine".into()]),
                ("sensor.b".into(), vec![]),
                ("switch.c".into(), vec![]),
                ("switch.d".into(), vec![]),
            ]),
            names: HashMap::from([
                ("sensor.a".into(), "Weekly Rain".into()),
                ("sensor.b".into(), "Dewpoint".into()),
                ("switch.c".into(), "Relais".into()),
                ("switch.d".into(), "Relais".into()),
            ]),
        };
        let states = [
            entity("sensor.a", "2", json!({"friendly_name": "Station Weekly Rain"})),
            entity("sensor.b", "5", json!({"friendly_name": "Station Dewpoint"})),
            entity("switch.c", "on", json!({"friendly_name": "Chevet Michel Relais"})),
            entity("switch.d", "off", json!({"friendly_name": "Relais"})),
            entity("sensor.hidden", "1", json!({"friendly_name": "Not exposed"})),
        ];
        assert_eq!(
            exposed.catalogue(&states),
            "Relais; Chevet Michel: Relais; Station: Dewpoint, Weekly Rain (pluie semaine)"
        );
    }

    #[test]
    fn timestamps_are_spoken_as_local_times() {
        let now = chrono::Local::now();
        assert_eq!(speak_time(&now.to_rfc3339()), format!("today at {}", now.format("%H:%M")));
        assert_eq!(speak_time("not a time"), "not a time");
        let lightning = entity("sensor.last_lightning", "2020-10-09T00:31:27+00:00", json!({"device_class": "timestamp"}));
        assert!(lightning.describe().contains("October 2020 at"), "{}", lightning.describe());
    }

    #[test]
    fn readings_are_spoken_plainly() {
        let states = sample();
        assert_eq!(states[0].describe(), "18.4 degrees Celsius");
        assert_eq!(states[1].describe(), "61 percent");
        assert_eq!(states[2].describe(), "-3.2 degrees Celsius");
        assert_eq!(states[3].describe(), "open");
        assert_eq!(entity("sensor.x", "unavailable", json!({})).describe(), "unavailable");
        let weather = entity("weather.home", "partlycloudy", json!({"temperature": 4.0, "temperature_unit": "°C", "humidity": 80}));
        assert_eq!(weather.describe(), "partly cloudy, 4 degrees Celsius, humidity 80 percent");
    }
}
