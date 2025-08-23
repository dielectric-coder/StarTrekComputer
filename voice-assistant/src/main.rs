use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, error, warn};

mod audio;
mod config;
mod llm;
mod rag;
mod search;
mod stt;
mod tts;
mod vad;
mod wake_word;

use audio::{AudioManager, AudioCommand};
use config::AssistantConfig;
use llm::LLMClient;
use search::{WebSearcher, SearchResults};
use stt::SpeechToText;
use tts::TextToSpeech;
use vad::VoiceActivityDetector;
use wake_word::WakeWordDetector;

#[derive(Debug, Clone)]
pub enum AssistantCommand {
    StartListening,
    StopListening,
    ProcessAudio(Vec<f32>),
    SpeakText(String),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum AssistantEvent {
    WakeWordDetected,
    SpeechDetected(String),
    LLMResponse(String),
    SearchResults(String),
    AudioPlaybackComplete,
    Error(String),
}

pub struct VoiceAssistant {
    config: AssistantConfig,
    audio_manager: Arc<AudioManager>,
    stt: Arc<SpeechToText>,
    tts: Arc<TextToSpeech>,
    llm: Arc<LLMClient>,
    searcher: Arc<WebSearcher>,
    vad: Arc<VoiceActivityDetector>,
    wake_word: Arc<Mutex<WakeWordDetector>>,
    command_tx: mpsc::UnboundedSender<AssistantCommand>,
    command_rx: Arc<Mutex<mpsc::UnboundedReceiver<AssistantCommand>>>,
    event_tx: mpsc::UnboundedSender<AssistantEvent>,
    event_rx: Arc<Mutex<mpsc::UnboundedReceiver<AssistantEvent>>>,
    is_listening: Arc<Mutex<bool>>,
    conversation_context: Arc<Mutex<Vec<String>>>,
}

impl VoiceAssistant {
    pub async fn new(config: AssistantConfig) -> Result<Self> {
        info!("Initializing Voice Assistant...");

        let audio_manager = Arc::new(AudioManager::new(&config).await?);
        let stt = Arc::new(SpeechToText::new(&config).await?);
        let tts = Arc::new(TextToSpeech::new(&config).await?);
        let llm = Arc::new(LLMClient::new(&config).await?);
        let searcher = Arc::new(WebSearcher::new(&config));
        let vad = Arc::new(VoiceActivityDetector::new(&config)?);
        let wake_word = Arc::new(Mutex::new(WakeWordDetector::new(&config).await?));

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            audio_manager,
            stt,
            tts,
            llm,
            searcher,
            vad,
            wake_word,
            command_tx,
            command_rx: Arc::new(Mutex::new(command_rx)),
            event_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            is_listening: Arc::new(Mutex::new(false)),
            conversation_context: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting Voice Assistant...");

        // Start audio capture
        self.audio_manager.start_capture(self.command_tx.clone()).await?;

        // Start wake word detection
        self.wake_word.lock().await.start(self.event_tx.clone()).await?;

        // Start the main event loop
        self.event_loop().await?;

        Ok(())
    }

    async fn event_loop(&self) -> Result<()> {
        let mut command_rx = self.command_rx.lock().await;
        let mut event_rx = self.event_rx.lock().await;

        loop {
            tokio::select! {
                Some(command) = command_rx.recv() => {
                    if let Err(e) = self.handle_command(command).await {
                        error!("Error handling command: {}", e);
                    }
                }
                Some(event) = event_rx.recv() => {
                    if let Err(e) = self.handle_event(event).await {
                        error!("Error handling event: {}", e);
                    }
                }
                else => break,
            }
        }

        Ok(())
    }

    async fn handle_command(&self, command: AssistantCommand) -> Result<()> {
        match command {
            AssistantCommand::StartListening => {
                info!("Starting to listen for voice commands");
                *self.is_listening.lock().await = true;
                
                // Play acknowledgment sound
                self.tts.speak("I'm listening").await?;
            }
            
            AssistantCommand::StopListening => {
                info!("Stopping voice command listening");
                *self.is_listening.lock().await = false;
            }
            
            AssistantCommand::ProcessAudio(audio_data) => {
                if *self.is_listening.lock().await {
                    self.process_audio_data(audio_data).await?;
                } else {
                    // Check for wake word
                    if self.wake_word.lock().await.detect(&audio_data).await? {
                        self.event_tx.send(AssistantEvent::WakeWordDetected)?;
                    }
                }
            }
            
            AssistantCommand::SpeakText(text) => {
                self.tts.speak(&text).await?;
                self.event_tx.send(AssistantEvent::AudioPlaybackComplete)?;
            }
            
            AssistantCommand::Shutdown => {
                info!("Shutting down Voice Assistant");
                self.audio_manager.stop().await?;
                return Ok(());
            }
        }

        Ok(())
    }

    async fn handle_event(&self, event: AssistantEvent) -> Result<()> {
        match event {
            AssistantEvent::WakeWordDetected => {
                info!("Wake word detected!");
                self.command_tx.send(AssistantCommand::StartListening)?;
            }
            
            AssistantEvent::SpeechDetected(text) => {
                info!("Speech detected: {}", text);
                self.process_user_input(text).await?;
            }
            
            AssistantEvent::LLMResponse(response) => {
                info!("LLM response: {}", response);
                self.command_tx.send(AssistantCommand::SpeakText(response))?;
            }
            
            AssistantEvent::SearchResults(results) => {
                info!("Search results: {}", results);
                self.command_tx.send(AssistantCommand::SpeakText(results))?;
            }
            
            AssistantEvent::AudioPlaybackComplete => {
                // Return to wake word detection mode
                self.command_tx.send(AssistantCommand::StopListening)?;
            }
            
            AssistantEvent::Error(error) => {
                error!("Assistant error: {}", error);
                self.command_tx.send(
                    AssistantCommand::SpeakText("Sorry, I encountered an error.".to_string())
                )?;
            }
        }

        Ok(())
    }

    async fn process_audio_data(&self, audio_data: Vec<f32>) -> Result<()> {
        // Check for voice activity
        if self.vad.detect_speech(&audio_data)? {
            // Convert speech to text
            if let Some(text) = self.stt.transcribe(&audio_data).await? {
                self.event_tx.send(AssistantEvent::SpeechDetected(text))?;
            }
        }

        Ok(())
    }

    async fn process_user_input(&self, input: String) -> Result<()> {
        // Strip the wake word "computer" from the beginning of the input
        let cleaned_input = self.strip_wake_word(&input);
        let input_lower = cleaned_input.to_lowercase();

        // Voice confirmation: echo back what was heard
        let confirmation = format!("You have requested: {}", cleaned_input);
        self.tts.speak(&confirmation).await?;
        
        // Processing notification
        self.tts.speak("Processing...").await?;

        // Check if it's a search request
        if input_lower.contains("search") || input_lower.contains("look up") || 
           input_lower.contains("find information") {
            self.handle_search_request(cleaned_input).await?;
        } else {
            // Process with local LLM
            self.handle_llm_request(cleaned_input).await?;
        }

        Ok(())
    }

    async fn handle_search_request(&self, query: String) -> Result<()> {
        info!("Processing search request: {}", query);
        
        // Extract the actual search terms
        let search_query = self.extract_search_terms(&query);
        
        match self.searcher.search(&search_query).await {
            Ok(search_results) => {
                let summary = self.format_search_results_with_fact_check(search_results);
                self.event_tx.send(AssistantEvent::SearchResults(summary))?;
            }
            Err(e) => {
                warn!("Search failed: {}, falling back to LLM", e);
                self.handle_llm_request(query).await?;
            }
        }

        Ok(())
    }

    async fn handle_llm_request(&self, input: String) -> Result<()> {
        info!("Processing LLM request: {}", input);
        
        // Add context from conversation history
        let mut context = self.conversation_context.lock().await;
        context.push(format!("User: {}", input));
        
        // Keep only last 10 exchanges to manage memory
        if context.len() > 20 {
            context.drain(0..10);
        }
        
        let context_str = context.join("\n");
        
        match self.llm.generate_response(&input, Some(&context_str)).await {
            Ok(response) => {
                context.push(format!("Assistant: {}", response));
                self.event_tx.send(AssistantEvent::LLMResponse(response))?;
            }
            Err(e) => {
                error!("LLM request failed: {}", e);
                self.event_tx.send(AssistantEvent::Error(
                    "I'm having trouble processing that request.".to_string()
                ))?;
            }
        }

        Ok(())
    }

    fn extract_search_terms(&self, query: &str) -> String {
        // Simple extraction - remove common search trigger words
        let query = query.to_lowercase();
        let query = query.replace("search for", "")
                        .replace("search", "")
                        .replace("look up", "")
                        .replace("find information about", "")
                        .replace("tell me about", "");
        
        query.trim().to_string()
    }

    fn format_search_results_with_fact_check(&self, search_results: SearchResults) -> String {
        let mut response = String::new();
        
        // Format primary search results
        if !search_results.primary_results.is_empty() {
            response.push_str("Based on search results: ");
            
            for (i, result) in search_results.primary_results.iter().take(2).enumerate() {
                if let Some(snippet) = result.get("snippet").and_then(|s| s.as_str()) {
                    response.push_str(&format!("{}. {}", i + 1, snippet));
                    if i == 0 { response.push_str(" "); }
                }
            }
        }
        
        // Add fact-checking information if available
        if !search_results.fact_check_results.is_empty() {
            response.push_str(" Verification from fact-checking sources: ");
            
            for result in search_results.fact_check_results.iter().take(1) {
                if let Some(snippet) = result.get("snippet").and_then(|s| s.as_str()) {
                    response.push_str(snippet);
                    break;
                }
            }
        }
        
        if response.is_empty() {
            "Unable to locate relevant information.".to_string()
        } else {
            response
        }
    }

    fn strip_wake_word(&self, input: &str) -> String {
        let input_lower = input.to_lowercase();
        
        // Remove "computer" from the beginning of the command
        if input_lower.starts_with("computer ") {
            input[9..].trim().to_string()
        } else if input_lower.starts_with("computer,") {
            input[9..].trim().to_string()
        } else if input_lower.starts_with("computer") {
            input[8..].trim().to_string()
        } else {
            input.trim().to_string()
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("voice_assistant=info")
        .init();

    info!("Starting Voice Assistant...");

    // Load configuration
    let config = AssistantConfig::load().await?;
    
    // Create and run the assistant
    let assistant = VoiceAssistant::new(config).await?;
    
    // Set up signal handlers for graceful shutdown
    let command_tx = assistant.command_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        info!("Received shutdown signal");
        let _ = command_tx.send(AssistantCommand::Shutdown);
    });
    
    // Run the assistant
    if let Err(e) = assistant.run().await {
        error!("Assistant failed: {}", e);
        std::process::exit(1);
    }

    info!("Voice Assistant stopped.");
    Ok(())
}
