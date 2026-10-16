use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::config::AssistantConfig;
use crate::rag::{RAGDatabase, RAGContext};
use crate::tools::ToolBox;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, error, debug};

/// Tool-calling rounds allowed per request before giving up.
const MAX_TOOL_ROUNDS: usize = 3;

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    keep_alive: &'static str,
    think: bool,
    options: OllamaOptions,
}

/// Keep the model loaded between questions; a cold load can take 15+ seconds.
const MODEL_KEEP_ALIVE: &str = "30m";

#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_ctx: usize,
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Value],
    tools: Value,
    stream: bool,
    keep_alive: &'static str,
    think: bool,
    options: OllamaOptions,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ToolCall {
    function: ToolFunction,
}

#[derive(Debug, Deserialize, Serialize)]
struct ToolFunction {
    name: String,
    #[serde(default)]
    arguments: Value,
}

pub struct LLMClient {
    config: AssistantConfig,
    client: Client,
    rag_db: RAGDatabase,
    tools: Arc<ToolBox>,
}

impl LLMClient {
    pub async fn new(config: &AssistantConfig, tools: Arc<ToolBox>) -> Result<Self> {
        info!("Initializing LLM client for model: {}", config.llm.model);
        
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;
        
        // Initialize RAG database
        let rag_db = RAGDatabase::new(config).await?;

        Self::preload_model(&client, config);

        Ok(Self {
            config: config.clone(),
            client,
            rag_db,
            tools,
        })
    }

    /// Load the model into Ollama in the background so the first question is answered quickly.
    fn preload_model(client: &Client, config: &AssistantConfig) {
        let client = client.clone();
        let url = format!("{}/api/generate", config.llm.base_url);
        let request = OllamaRequest {
            model: config.llm.model.clone(),
            prompt: String::new(), // an empty prompt only loads the model
            stream: false,
            keep_alive: MODEL_KEEP_ALIVE,
            think: config.llm.think,
            options: OllamaOptions {
                temperature: config.llm.temperature,
                num_ctx: config.llm.context_length,
            },
        };

        tokio::spawn(async move {
            match client.post(&url).json(&request).send().await {
                Ok(response) if response.status().is_success() => {
                    info!("LLM model {} loaded", request.model)
                }
                Ok(response) => error!(
                    "Failed to preload LLM model {}: {} (is it pulled in Ollama?)",
                    request.model,
                    response.status()
                ),
                Err(e) => error!("Failed to preload LLM model {}: {}", request.model, e),
            }
        });
    }

    pub async fn generate_response(&self, prompt: &str, context: Option<&str>) -> Result<String> {
        // Get RAG context for personalized and informed responses
        let rag_context = self.rag_db.get_rag_context(prompt).await?;
        debug!("Retrieved RAG context: {} entries", rag_context.knowledge_entries.len());
        
        // Build enhanced system prompt with user context
        let mut system_prompt = "You are the Starship Enterprise computer. Speak in a clipped, precise, neutral tone. Provide only the factual answer, using the fewest words possible, as in the original Star Trek series. Be direct and concise. Do not use conversational pleasantries. Always maintain a professional, analytical demeanor. Begin responses with 'Affirmative' or 'Negative' when appropriate. State facts clearly and economically.".to_string();
        
        // The model has no clock of its own; without this it invents dates
        system_prompt.push_str(&format!(
            " The current local date and time is {}. Use it when asked about the date, day or time.",
            chrono::Local::now().format("%A, %-d %B %Y, %H:%M")
        ));
        system_prompt.push_str(
            " Use the provided tools when the user asks for a timer, reminder, volume change, \
             system status or the captain's log. Your replies are spoken aloud: plain sentences only, \
             no markdown, lists or symbols.",
        );

        // Add user preference adaptations
        system_prompt = self.adapt_to_user_preferences(&system_prompt, &rag_context);
        
        // Build context with RAG information
        let mut full_context = String::new();
        
        // Add previous context if provided
        if let Some(ctx) = context {
            full_context.push_str(ctx);
            full_context.push_str("\n\n");
        }
        
        // Add RAG context summary
        if !rag_context.context_summary.is_empty() {
            full_context.push_str("Context: ");
            full_context.push_str(&rag_context.context_summary);
            full_context.push_str("\n\n");
        }
        
        // Add relevant knowledge
        if !rag_context.knowledge_entries.is_empty() {
            full_context.push_str("Relevant information: ");
            for (i, entry) in rag_context.knowledge_entries.iter().take(3).enumerate() {
                if i > 0 { full_context.push_str("; "); }
                full_context.push_str(&entry.content.chars().take(200).collect::<String>());
                if entry.content.len() > 200 { full_context.push_str("..."); }
            }
            full_context.push_str("\n\n");
        }
        
        let mut messages = vec![
            json!({"role": "system", "content": format!("{}\n\n{}", system_prompt, full_context.trim())}),
            json!({"role": "user", "content": prompt}),
        ];

        let mut response_text = None;
        let mut used_tools = false;
        for _ in 0..MAX_TOOL_ROUNDS {
            let message = self.chat(&messages).await?;
            if message.tool_calls.is_empty() {
                response_text = Some(message.content.trim().to_string());
                break;
            }

            used_tools = true;
            messages.push(json!({"role": "assistant", "content": message.content, "tool_calls": message.tool_calls}));

            let mut speeches = Vec::new();
            let mut all_spoken = true;
            for call in &message.tool_calls {
                let output = self.tools.execute(&call.function.name, &call.function.arguments).await;
                match output.speech {
                    Some(speech) => speeches.push(speech),
                    None => all_spoken = false,
                }
                messages.push(json!({"role": "tool", "tool_name": call.function.name, "content": output.result}));
            }

            // Actions with a fixed confirmation don't need another LLM round
            if all_spoken {
                response_text = Some(speeches.join(" "));
                break;
            }
        }
        let response_text = response_text.context("too many tool-calling rounds")?;

        // Store the conversation in RAG database for learning; tool results aren't knowledge
        let conversation_id = self.rag_db.store_conversation(
            prompt,
            &response_text,
            Some(&rag_context.context_summary),
            !used_tools,
        ).await?;
        debug!("Stored conversation with ID: {}", conversation_id);

        Ok(response_text)
    }

    async fn chat(&self, messages: &[Value]) -> Result<ChatMessage> {
        let request = ChatRequest {
            model: &self.config.llm.model,
            messages,
            tools: ToolBox::definitions(),
            stream: false,
            keep_alive: MODEL_KEEP_ALIVE,
            think: self.config.llm.think,
            options: OllamaOptions {
                temperature: 0.3, // Lower temperature for more consistent, factual responses
                num_ctx: self.config.llm.context_length,
            },
        };

        let response = self.client
            .post(&format!("{}/api/chat", self.config.llm.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            error!("LLM request failed with status: {} {}", status, response.text().await.unwrap_or_default());
            anyhow::bail!("LLM request failed");
        }
        Ok(response.json::<ChatResponse>().await?.message)
    }

    fn adapt_to_user_preferences(&self, system_prompt: &str, rag_context: &RAGContext) -> String {
        let mut adapted_prompt = system_prompt.to_string();
        
        // Adapt based on user preferences
        for preference in &rag_context.user_preferences {
            match (preference.preference_type.as_str(), preference.key.as_str()) {
                ("response_style", "length") => {
                    match preference.value.as_str() {
                        "brief" => adapted_prompt.push_str(" Be especially concise and brief in responses."),
                        "detailed" => adapted_prompt.push_str(" Provide more comprehensive explanations while maintaining computer precision."),
                        _ => {}
                    }
                },
                ("response_style", "formality") => {
                    if preference.value == "polite" && preference.confidence > 0.5 {
                        adapted_prompt.push_str(" Acknowledge requests appropriately while maintaining computer protocol.");
                    }
                },
                ("topic_interest", topic) => {
                    if preference.value == "high" && preference.confidence > 0.7 {
                        adapted_prompt.push_str(&format!(" User shows high interest in {}.", topic));
                    }
                },
                _ => {}
            }
        }
        
        adapted_prompt
    }
}
