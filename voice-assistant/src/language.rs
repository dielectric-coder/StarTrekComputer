//! The language the computer works in: English or French, switched by voice
//! ("Computer, work in French" / "Computer, travaille en anglais") and kept across restarts.
//!
//! It picks the speech recognition model, the voice, the language of the LLM's answers,
//! the fixed phrases and which Home Assistant aliases are used.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;
use tracing::{info, warn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);
static STATE_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Restore the language chosen before the last restart.
pub fn init(data_dir: &Path) {
    let path = data_dir.join("language");
    let saved = std::fs::read_to_string(&path).unwrap_or_default();
    let lang = if saved.trim() == "fr" { Lang::Fr } else { Lang::En };
    CURRENT.store(lang as u8, Ordering::Relaxed);
    let _ = STATE_FILE.set(path);
    info!("Working in {}", name(lang));
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::Fr as u8 { Lang::Fr } else { Lang::En }
}

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
    info!("Now working in {}", name(lang));
    if let Some(path) = STATE_FILE.get() {
        let code = if lang == Lang::Fr { "fr" } else { "en" };
        if let Err(e) = std::fs::write(path, code) {
            warn!("Could not save the language choice to {}: {}", path.display(), e);
        }
    }
}

fn name(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "English",
        Lang::Fr => "French",
    }
}

/// The English or French version of a phrase, for the current language.
pub fn tr<'a>(en: &'a str, fr: &'a str) -> &'a str {
    match current() {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

/// Lowercase without accents.
pub fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'œ' => 'o',
            other => other,
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// "Work in French", "speak English", "travaille en anglais", "passe en français"...
/// A short request naming a language and asking to use it.
pub fn switch_command(text: &str) -> Option<Lang> {
    let words = words(text);
    let words: Vec<&str> = words.iter().map(String::as_str).skip_while(|w| *w == "computer").collect();
    if words.is_empty() || words.len() > 7 {
        return None;
    }
    let lang = if words.iter().any(|w| matches!(*w, "french" | "francais")) {
        Lang::Fr
    } else if words.iter().any(|w| matches!(*w, "english" | "anglais")) {
        Lang::En
    } else {
        return None;
    };
    const ASKING: &[&str] = &[
        "work", "speak", "talk", "switch", "use", "change", "mode", "language", "in", "to",
        "travaille", "travailler", "parle", "parler", "passe", "passer", "bascule", "utilise",
        "langue", "en",
    ];
    // "What is 'chair' in French?" is a question, not a switch
    const QUESTION: &[&str] = &["what", "how", "say", "translate", "word", "comment", "dit", "traduis", "traduire", "mot"];
    if words.iter().any(|w| QUESTION.contains(w)) {
        return None;
    }
    let only_language = words.len() == 1;
    (only_language || words.iter().any(|w| ASKING.contains(w))).then_some(lang)
}

/// Words that only appear in French device names and aliases.
const FRENCH_WORDS: &[&str] = &[
    "de", "du", "des", "la", "le", "les", "l", "d", "au", "aux", "et", "sur", "pour", "avec", "ce",
    "cette", "ces", "dernier", "derniere", "lumiere", "lumieres", "lampe", "lampes", "chauffage",
    "prise", "pluie", "porte", "fenetre", "cuisine", "salon", "chambre", "serre", "sol", "vent",
    "foudre", "rafale", "pression", "moyenne", "taux", "mois", "annee", "aujourd", "hui", "heure",
    "coup", "radiation", "indice", "solaire", "detection", "mouvement", "allee", "laboratoire",
    "stationnement", "centrale", "chevet", "lecture", "pied", "garde", "robe", "lavage", "salle",
    "diner", "electrique", "exterieur", "exterieure", "interieur", "interieure", "humidite",
    "rosee", "ressentie", "facteur", "refroidissement", "precipitation",
    "bonne", "nuit", "nord", "sud", "ouest", "barometrique",
];

/// Which language a Home Assistant alias is written in. Aliases carry no language, so it
/// is guessed: accents or French words make it French, anything else counts as English.
pub fn alias_language(alias: &str) -> Lang {
    let accented = alias.chars().any(|c| "àâäéèêëîïôöùûüçœ".contains(c.to_lowercase().next().unwrap_or(c)));
    if accented || words(alias).iter().any(|w| FRENCH_WORDS.contains(&w.as_str())) {
        Lang::Fr
    } else {
        Lang::En
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_language_switches() {
        for (text, lang) in [
            ("Work in French.", Lang::Fr),
            ("computer, work in english", Lang::En),
            ("Speak French", Lang::Fr),
            ("Travaille en anglais.", Lang::En),
            ("Passe en français", Lang::Fr),
            ("Parle anglais", Lang::En),
            ("Français.", Lang::Fr),
            ("Switch to English mode", Lang::En),
        ] {
            assert_eq!(switch_command(text), Some(lang), "{text}");
        }
    }

    #[test]
    fn ordinary_requests_are_not_switches() {
        for text in [
            "What is chair in French?",
            "Comment dit-on chaise en anglais ?",
            "Turn on the kitchen light",
            "Tell me about the history of the English language and how it borrowed so many French words",
            "",
        ] {
            assert_eq!(switch_command(text), None, "{text}");
        }
    }

    #[test]
    fn guesses_alias_language() {
        for alias in ["Lumière cuisine", "Lampe de lecture", "Pluie cette semaine", "Chauffage A", "Point de rosée", "Prise électrique salon A"] {
            assert_eq!(alias_language(alias), Lang::Fr, "{alias}");
        }
        for alias in ["kitchen light", "reading lamp", "rain this week", "heating A", "dew point", "living room plug"] {
            assert_eq!(alias_language(alias), Lang::En, "{alias}");
        }
    }
}
