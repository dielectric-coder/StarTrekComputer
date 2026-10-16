use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::{SqliteConnectOptions, SqlitePool}, Row};
use std::collections::HashMap;
use tracing::{info, debug};
use uuid::Uuid;
use crate::config::AssistantConfig;

// Vector embedding size (using sentence transformers typical size)
const EMBEDDING_SIZE: usize = 384;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Conversation {
    pub id: Uuid,
    pub user_input: String,
    pub assistant_response: String,
    pub timestamp: DateTime<Utc>,
    pub context_used: Option<String>,
    pub user_satisfaction: Option<i32>, // 1-5 rating if provided
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserPreference {
    pub id: Uuid,
    pub preference_type: String, // "response_style", "topic_interest", "complexity_level"
    pub key: String,
    pub value: String,
    pub confidence: f32, // 0.0-1.0 confidence in this preference
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KnowledgeEntry {
    pub id: Uuid,
    pub content: String,
    pub source: String, // "conversation", "search", "correction"
    pub topic: String,
    pub embedding: Vec<f32>,
    pub relevance_score: f32,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: i32,
}

#[derive(Debug, Clone)]
pub struct RAGContext {
    pub relevant_conversations: Vec<Conversation>,
    pub user_preferences: Vec<UserPreference>,
    pub knowledge_entries: Vec<KnowledgeEntry>,
    pub context_summary: String,
}

pub struct RAGDatabase {
    pool: SqlitePool,
    embedding_model: Option<EmbeddingModel>,
}

// Simplified embedding model for now - in production would use actual sentence transformers
struct EmbeddingModel {
    // Placeholder for embedding model
}

impl EmbeddingModel {
    fn new() -> Result<Self> {
        Ok(Self {})
    }

    fn encode(&self, text: &str) -> Result<Vec<f32>> {
        // Simplified embedding - in real implementation would use proper model
        // For now, create a basic hash-based embedding for demonstration
        let mut embedding = vec![0.0; EMBEDDING_SIZE];
        let bytes = text.as_bytes();
        
        for (i, &byte) in bytes.iter().enumerate() {
            let idx = (byte as usize + i) % EMBEDDING_SIZE;
            embedding[idx] += (byte as f32) / 255.0;
        }
        
        // Normalize
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for val in &mut embedding {
                *val /= magnitude;
            }
        }
        
        Ok(embedding)
    }
}

impl RAGDatabase {
    pub async fn new(config: &AssistantConfig) -> Result<Self> {
        let database_path = config.general.data_dir.join("rag_memory.db");
        
        info!("Initializing RAG database at: {:?}", database_path);
        
        tokio::fs::create_dir_all(&config.general.data_dir).await?;
        let connect_options = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(connect_options).await?;
        
        let rag_db = Self {
            pool,
            embedding_model: Some(EmbeddingModel::new()?),
        };
        
        rag_db.initialize_schema().await?;
        
        info!("RAG database initialized successfully");
        Ok(rag_db)
    }

    async fn initialize_schema(&self) -> Result<()> {
        // Conversations table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                user_input TEXT NOT NULL,
                assistant_response TEXT NOT NULL,
                timestamp DATETIME NOT NULL,
                context_used TEXT,
                user_satisfaction INTEGER,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // User preferences table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_preferences (
                id TEXT PRIMARY KEY,
                preference_type TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                confidence REAL NOT NULL,
                last_updated DATETIME NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(preference_type, key)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Knowledge entries table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS knowledge_entries (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                source TEXT NOT NULL,
                topic TEXT NOT NULL,
                embedding BLOB NOT NULL,
                relevance_score REAL NOT NULL,
                created_at DATETIME NOT NULL,
                last_accessed DATETIME NOT NULL,
                access_count INTEGER DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_conversations_timestamp ON conversations(timestamp)")
            .execute(&self.pool).await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_preferences_type ON user_preferences(preference_type)")
            .execute(&self.pool).await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_knowledge_topic ON knowledge_entries(topic)")
            .execute(&self.pool).await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_knowledge_source ON knowledge_entries(source)")
            .execute(&self.pool).await?;

        Ok(())
    }

    pub async fn store_conversation(&self, user_input: &str, assistant_response: &str, context_used: Option<&str>, learn_knowledge: bool) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let timestamp = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO conversations (id, user_input, assistant_response, timestamp, context_used)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(id.to_string())
        .bind(user_input)
        .bind(assistant_response)
        .bind(timestamp)
        .bind(context_used)
        .execute(&self.pool)
        .await?;

        debug!("Stored conversation: {} -> {}", user_input.chars().take(50).collect::<String>(), 
               assistant_response.chars().take(50).collect::<String>());

        // Learn from this conversation
        self.learn_user_preferences(user_input).await?;
        if learn_knowledge {
            self.extract_knowledge(user_input, assistant_response).await?;
        }

        Ok(id)
    }

    async fn learn_user_preferences(&self, user_input: &str) -> Result<()> {
        // Analyze user input patterns to learn preferences
        
        // Response length preference
        if user_input.contains("brief") || user_input.contains("short") || user_input.contains("quickly") {
            self.update_preference("response_style", "length", "brief", 0.8).await?;
        } else if user_input.contains("detail") || user_input.contains("explain") || user_input.contains("elaborate") {
            self.update_preference("response_style", "length", "detailed", 0.8).await?;
        }

        // Formality preference
        if user_input.contains("please") || user_input.contains("thank you") {
            self.update_preference("response_style", "formality", "polite", 0.6).await?;
        }

        // Topic interests (simplified)
        let topics = vec![
            ("science", vec!["physics", "chemistry", "biology", "astronomy"]),
            ("technology", vec!["computer", "software", "programming", "AI"]),
            ("history", vec!["historical", "past", "ancient", "war"]),
            ("entertainment", vec!["movie", "music", "game", "book"]),
        ];

        for (topic, keywords) in topics {
            if keywords.iter().any(|&keyword| user_input.to_lowercase().contains(keyword)) {
                self.update_preference("topic_interest", topic, "high", 0.7).await?;
            }
        }

        Ok(())
    }

    async fn update_preference(&self, pref_type: &str, key: &str, value: &str, confidence: f32) -> Result<()> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        // Use UPSERT to update existing preference or insert new one
        sqlx::query(
            r#"
            INSERT INTO user_preferences (id, preference_type, key, value, confidence, last_updated)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(preference_type, key) DO UPDATE SET
                value = ?4,
                confidence = MAX(confidence, ?5),
                last_updated = ?6
            "#,
        )
        .bind(id.to_string())
        .bind(pref_type)
        .bind(key)
        .bind(value)
        .bind(confidence)
        .bind(now)
        .execute(&self.pool)
        .await?;

        debug!("Updated preference: {} -> {} = {} (confidence: {})", pref_type, key, value, confidence);
        Ok(())
    }

    async fn extract_knowledge(&self, user_input: &str, assistant_response: &str) -> Result<()> {
        // Extract factual information from conversations
        
        // If user asked a question and got a factual response, store as knowledge
        let input_lower = user_input.to_lowercase();
        let is_question = ["what", "who", "when", "where", "how", "why"]
            .iter()
            .any(|w| input_lower.contains(w));

        if is_question && !is_time_sensitive(&input_lower) {
            let topic = self.extract_topic(user_input);
            // Keep the question with the answer; a bare "Affirmative. Paris." is meaningless later
            let content = format!("Q: {} A: {}", user_input.trim(), assistant_response.trim());

            if let Some(embedding_model) = &self.embedding_model {
                let embedding = embedding_model.encode(&content)?;
                let embedding_bytes = bincode::serialize(&embedding)?;

                let id = Uuid::new_v4();
                let now = Utc::now();

                sqlx::query(
                    r#"
                    INSERT INTO knowledge_entries (id, content, source, topic, embedding, relevance_score, created_at, last_accessed)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                    "#,
                )
                .bind(id.to_string())
                .bind(&content)
                .bind("conversation")
                .bind(&topic)
                .bind(&embedding_bytes)
                .bind(1.0) // Initial relevance score
                .bind(now)
                .bind(now)
                .execute(&self.pool)
                .await?;

                debug!("Stored knowledge entry for topic: {}", topic);
            }
        }

        Ok(())
    }

    fn extract_topic(&self, input: &str) -> String {
        let input_lower = input.to_lowercase();
        
        // Simple keyword-based topic extraction
        let topics = vec![
            ("science", vec!["physics", "chemistry", "biology", "astronomy", "space", "planet", "star"]),
            ("technology", vec!["computer", "software", "programming", "AI", "robot", "internet"]),
            ("history", vec!["historical", "past", "ancient", "war", "battle", "empire", "civilization"]),
            ("geography", vec!["country", "city", "mountain", "river", "ocean", "continent"]),
            ("health", vec!["medical", "health", "disease", "medicine", "doctor", "hospital"]),
            ("general", vec![]), // fallback
        ];

        for (topic, keywords) in topics {
            if topic == "general" || keywords.iter().any(|&keyword| input_lower.contains(keyword)) {
                return topic.to_string();
            }
        }

        "general".to_string()
    }

    pub async fn get_rag_context(&self, user_input: &str) -> Result<RAGContext> {
        let mut context = RAGContext {
            relevant_conversations: Vec::new(),
            user_preferences: Vec::new(),
            knowledge_entries: Vec::new(),
            context_summary: String::new(),
        };

        // Get recent relevant conversations
        context.relevant_conversations = self.get_relevant_conversations(user_input, 3).await?;

        // Get user preferences
        context.user_preferences = self.get_user_preferences().await?;

        // Get relevant knowledge entries
        context.knowledge_entries = self.get_relevant_knowledge(user_input, 5).await?;

        // Generate context summary
        context.context_summary = self.generate_context_summary(&context).await;

        Ok(context)
    }

    async fn get_relevant_conversations(&self, input: &str, limit: i32) -> Result<Vec<Conversation>> {
        // Simple similarity search based on keyword matching
        // In production, would use vector similarity
        
        let rows = sqlx::query(
            r#"
            SELECT id, user_input, assistant_response, timestamp, context_used, user_satisfaction
            FROM conversations
            WHERE user_input LIKE '%' || ?1 || '%'
            OR assistant_response LIKE '%' || ?1 || '%'
            ORDER BY timestamp DESC
            LIMIT ?2
            "#,
        )
        .bind(input)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut conversations = Vec::new();
        for row in rows {
            conversations.push(Conversation {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                user_input: row.get("user_input"),
                assistant_response: row.get("assistant_response"),
                timestamp: row.get("timestamp"),
                context_used: row.get("context_used"),
                user_satisfaction: row.get("user_satisfaction"),
            });
        }

        Ok(conversations)
    }

    async fn get_user_preferences(&self) -> Result<Vec<UserPreference>> {
        let rows = sqlx::query(
            r#"
            SELECT id, preference_type, key, value, confidence, last_updated
            FROM user_preferences
            ORDER BY confidence DESC, last_updated DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut preferences = Vec::new();
        for row in rows {
            preferences.push(UserPreference {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                preference_type: row.get("preference_type"),
                key: row.get("key"),
                value: row.get("value"),
                confidence: row.get("confidence"),
                last_updated: row.get("last_updated"),
            });
        }

        Ok(preferences)
    }

    async fn get_relevant_knowledge(&self, input: &str, limit: i32) -> Result<Vec<KnowledgeEntry>> {
        // Simple keyword-based retrieval for now. "general" is the catch-all topic,
        // so matching on it would attach unrelated entries to almost every question.
        let topic = self.extract_topic(input);
        let topic_filter = if topic == "general" { "" } else { topic.as_str() };
        
        let rows = sqlx::query(
            r#"
            SELECT id, content, source, topic, embedding, relevance_score, created_at, last_accessed, access_count
            FROM knowledge_entries
            WHERE topic = ?1 OR content LIKE '%' || ?2 || '%'
            ORDER BY relevance_score DESC, access_count DESC
            LIMIT ?3
            "#,
        )
        .bind(topic_filter)
        .bind(input)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut entries = Vec::new();
        for row in rows {
            let embedding_bytes: Vec<u8> = row.get("embedding");
            let embedding: Vec<f32> = bincode::deserialize(&embedding_bytes)?;
            
            entries.push(KnowledgeEntry {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                content: row.get("content"),
                source: row.get("source"),
                topic: row.get("topic"),
                embedding,
                relevance_score: row.get("relevance_score"),
                created_at: row.get("created_at"),
                last_accessed: row.get("last_accessed"),
                access_count: row.get("access_count"),
            });
        }

        // Update access count for retrieved entries
        for entry in &entries {
            self.update_knowledge_access(&entry.id).await?;
        }

        Ok(entries)
    }

    async fn update_knowledge_access(&self, entry_id: &Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE knowledge_entries 
            SET access_count = access_count + 1, last_accessed = ?2
            WHERE id = ?1
            "#,
        )
        .bind(entry_id.to_string())
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn generate_context_summary(&self, context: &RAGContext) -> String {
        let mut summary = String::new();

        // Summarize user preferences
        if !context.user_preferences.is_empty() {
            summary.push_str("User preferences: ");
            let mut prefs = HashMap::new();
            
            for pref in &context.user_preferences {
                prefs.insert(format!("{}_{}", pref.preference_type, pref.key), &pref.value);
            }
            
            if let Some(length) = prefs.get("response_style_length") {
                summary.push_str(&format!("prefers {} responses, ", length));
            }
            
            if let Some(formality) = prefs.get("response_style_formality") {
                summary.push_str(&format!("communication style: {}, ", formality));
            }

            // Add topic interests
            let interests: Vec<String> = context.user_preferences.iter()
                .filter(|p| p.preference_type == "topic_interest" && p.value == "high")
                .map(|p| p.key.clone())
                .collect();
            
            if !interests.is_empty() {
                summary.push_str(&format!("interested in: {}. ", interests.join(", ")));
            }
        }

        // Add relevant knowledge context
        if !context.knowledge_entries.is_empty() {
            summary.push_str("Previous knowledge: ");
            for (i, entry) in context.knowledge_entries.iter().take(2).enumerate() {
                if i > 0 { summary.push_str("; "); }
                summary.push_str(&entry.content.chars().take(100).collect::<String>());
                if entry.content.len() > 100 { summary.push_str("..."); }
            }
            summary.push_str(". ");
        }

        // Add conversation context
        if !context.relevant_conversations.is_empty() {
            summary.push_str("Recent context: ");
            if let Some(recent) = context.relevant_conversations.first() {
                summary.push_str(&format!("User previously asked: '{}'. ", 
                    recent.user_input.chars().take(80).collect::<String>()));
            }
        }

        if summary.is_empty() {
            summary = "First interaction with user.".to_string();
        }

        summary
    }

    pub async fn rate_response(&self, conversation_id: &Uuid, rating: i32) -> Result<()> {
        sqlx::query(
            "UPDATE conversations SET user_satisfaction = ?1 WHERE id = ?2"
        )
        .bind(rating)
        .bind(conversation_id.to_string())
        .execute(&self.pool)
        .await?;

        info!("Updated conversation rating: {} -> {}", conversation_id, rating);
        Ok(())
    }

    pub async fn get_statistics(&self) -> Result<HashMap<String, i64>> {
        let mut stats = HashMap::new();

        // Conversation count
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations")
            .fetch_one(&self.pool).await?;
        stats.insert("total_conversations".to_string(), count);

        // Knowledge entries count  
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_entries")
            .fetch_one(&self.pool).await?;
        stats.insert("knowledge_entries".to_string(), count);

        // Preferences count
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_preferences")
            .fetch_one(&self.pool).await?;
        stats.insert("user_preferences".to_string(), count);

        // Average satisfaction rating
        let avg_rating: Option<f64> = sqlx::query_scalar(
            "SELECT AVG(user_satisfaction) FROM conversations WHERE user_satisfaction IS NOT NULL"
        ).fetch_one(&self.pool).await?;
        
        if let Some(rating) = avg_rating {
            stats.insert("average_satisfaction".to_string(), rating as i64);
        }

        Ok(stats)
    }

    pub async fn cleanup_old_data(&self, days_to_keep: i64) -> Result<()> {
        let cutoff_date = Utc::now() - chrono::Duration::days(days_to_keep);

        // Clean old conversations (keep highly rated ones)
        let deleted = sqlx::query(
            r#"
            DELETE FROM conversations 
            WHERE timestamp < ?1 
            AND (user_satisfaction IS NULL OR user_satisfaction < 4)
            "#,
        )
        .bind(cutoff_date)
        .execute(&self.pool)
        .await?
        .rows_affected();

        info!("Cleaned up {} old conversations", deleted);

        // Clean low-relevance knowledge entries
        let deleted = sqlx::query(
            r#"
            DELETE FROM knowledge_entries 
            WHERE created_at < ?1 
            AND relevance_score < 0.3 
            AND access_count < 2
            "#,
        )
        .bind(cutoff_date)
        .execute(&self.pool)
        .await?
        .rows_affected();

        info!("Cleaned up {} low-relevance knowledge entries", deleted);

        Ok(())
    }
}
/// Answers to these go stale (or were guessed), so they must not be stored as knowledge.
fn is_time_sensitive(input_lower: &str) -> bool {
    const WORDS: &[&str] = &[
        "date", "day", "time", "today", "tonight", "tomorrow", "yesterday", "now",
        "current", "currently", "latest", "news", "weather", "moon", "stardate",
    ];
    input_lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| WORDS.contains(&word))
}
