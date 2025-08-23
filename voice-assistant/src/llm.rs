use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::config::AssistantConfig;
use crate::rag::{RAGDatabase, RAGContext};
use tracing::{info, error, debug};
use uuid::Uuid;

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_ctx: usize,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
    done: bool,
}

pub struct LLMClient {
    config: AssistantConfig,
    client: Client,
    rag_db: RAGDatabase,
}

impl LLMClient {
    pub async fn new(config: &AssistantConfig) -> Result<Self> {
        info!("Initializing LLM client for model: {}", config.llm.model);
        
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        
        // Initialize RAG database
        let rag_db = RAGDatabase::new(config).await?;
            
        Ok(Self {
            config: config.clone(),
            client,
            rag_db,
        })
    }

    pub async fn generate_response(&self, prompt: &str, context: Option<&str>) -> Result<String> {
        // Get RAG context for personalized and informed responses
        let rag_context = self.rag_db.get_rag_context(prompt).await?;
        debug!("Retrieved RAG context: {} entries", rag_context.knowledge_entries.len());
        
        // Build enhanced system prompt with user context
        let mut system_prompt = "You are the Starship Enterprise computer. Speak in a clipped, precise, neutral tone. Provide only the factual answer, using the fewest words possible, as in the original Star Trek series. Be direct and concise. Do not use conversational pleasantries. Always maintain a professional, analytical demeanor. Begin responses with 'Affirmative' or 'Negative' when appropriate. State facts clearly and economically.".to_string();
        
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
        
        let full_prompt = format!("{}\n\n{}\nUser: {}", system_prompt, full_context, prompt);
        
        let request = OllamaRequest {
            model: self.config.llm.model.clone(),
            prompt: full_prompt,
            stream: false,
            options: OllamaOptions {
                temperature: 0.3, // Lower temperature for more consistent, factual responses
                num_ctx: self.config.llm.context_length,
            },
        };

        let response = self.client
            .post(&format!("{}/api/generate", self.config.llm.base_url))
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            let ollama_response: OllamaResponse = response.json().await?;
            let response_text = ollama_response.response.trim().to_string();
            
            // Store the conversation in RAG database for learning
            let conversation_id = self.rag_db.store_conversation(
                prompt, 
                &response_text, 
                Some(&rag_context.context_summary)
            ).await?;
            
            debug!("Stored conversation with ID: {}", conversation_id);
            
            Ok(response_text)
        } else {
            error!("LLM request failed with status: {}", response.status());
            Err(anyhow::anyhow!("LLM request failed"))
        }
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
    
    pub async fn rate_last_response(&self, rating: i32) -> Result<()> {
        // This would need to track the last conversation ID
        // For now, this is a placeholder for the rating functionality
        info!("Response rated: {}/5", rating);
        Ok(())
    }
    
    pub async fn get_memory_stats(&self) -> Result<std::collections::HashMap<String, i64>> {
        self.rag_db.get_statistics().await
    }
}
