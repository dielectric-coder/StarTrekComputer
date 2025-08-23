-- Initial RAG Database Schema
-- This file contains the database schema for the RAG system

-- Conversations table - stores all user interactions
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    user_input TEXT NOT NULL,
    assistant_response TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    context_used TEXT,
    user_satisfaction INTEGER CHECK (user_satisfaction >= 1 AND user_satisfaction <= 5),
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- User preferences table - learns user behavior patterns
CREATE TABLE IF NOT EXISTS user_preferences (
    id TEXT PRIMARY KEY,
    preference_type TEXT NOT NULL, -- 'response_style', 'topic_interest', 'complexity_level'
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    last_updated DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(preference_type, key)
);

-- Knowledge entries table - stores learned information
CREATE TABLE IF NOT EXISTS knowledge_entries (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    source TEXT NOT NULL, -- 'conversation', 'search', 'correction'
    topic TEXT NOT NULL,
    embedding BLOB NOT NULL, -- Vector embedding for similarity search
    relevance_score REAL NOT NULL CHECK (relevance_score >= 0.0),
    created_at DATETIME NOT NULL,
    last_accessed DATETIME NOT NULL,
    access_count INTEGER DEFAULT 0
);

-- Performance indexes
CREATE INDEX IF NOT EXISTS idx_conversations_timestamp ON conversations(timestamp);
CREATE INDEX IF NOT EXISTS idx_conversations_satisfaction ON conversations(user_satisfaction);
CREATE INDEX IF NOT EXISTS idx_preferences_type ON user_preferences(preference_type);
CREATE INDEX IF NOT EXISTS idx_preferences_updated ON user_preferences(last_updated);
CREATE INDEX IF NOT EXISTS idx_knowledge_topic ON knowledge_entries(topic);
CREATE INDEX IF NOT EXISTS idx_knowledge_source ON knowledge_entries(source);
CREATE INDEX IF NOT EXISTS idx_knowledge_relevance ON knowledge_entries(relevance_score);
CREATE INDEX IF NOT EXISTS idx_knowledge_accessed ON knowledge_entries(last_accessed);

-- Views for common queries
CREATE VIEW IF NOT EXISTS recent_conversations AS
SELECT 
    id,
    user_input,
    assistant_response,
    timestamp,
    user_satisfaction
FROM conversations 
ORDER BY timestamp DESC 
LIMIT 50;

CREATE VIEW IF NOT EXISTS top_preferences AS
SELECT 
    preference_type,
    key,
    value,
    confidence,
    last_updated
FROM user_preferences 
WHERE confidence > 0.5
ORDER BY confidence DESC, last_updated DESC;

CREATE VIEW IF NOT EXISTS knowledge_by_topic AS
SELECT 
    topic,
    COUNT(*) as entry_count,
    AVG(relevance_score) as avg_relevance,
    MAX(last_accessed) as last_accessed
FROM knowledge_entries 
GROUP BY topic
ORDER BY entry_count DESC;