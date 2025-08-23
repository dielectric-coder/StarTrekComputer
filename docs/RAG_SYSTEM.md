# 🧠 Star Trek Computer RAG (Retrieval-Augmented Generation) System

The Star Trek Computer features an advanced RAG system that enables it to remember interactions, learn user preferences, and become more personalized over time - just like the Enterprise computer would adapt to its crew.

## 🎯 Overview

The RAG system transforms the voice assistant from a stateless interaction model into a learning, adaptive companion that:

- **Remembers conversations** and builds context over time
- **Learns user preferences** for response style, topics, and complexity
- **Builds a knowledge base** from interactions and searches
- **Adapts personality** based on user behavior patterns
- **Provides contextual responses** using relevant historical information

## 🏗️ Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   User Input    │───▶│  RAG Retrieval   │───▶│ Context Builder │
│                 │    │    System        │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                │                        │
                                ▼                        ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   SQLite DB     │◀───│  Learning Engine │    │ Enhanced LLM    │
│ - Conversations │    │                  │    │    Prompt       │
│ - Preferences   │    │ • Preference     │    └─────────────────┘
│ - Knowledge     │    │   Detection      │             │
└─────────────────┘    │ • Topic Extract  │             ▼
                       │ • Embedding Gen  │    ┌─────────────────┐
                       └──────────────────┘    │   Response +    │
                                │              │ Memory Storage  │
                                ▼              └─────────────────┘
                       ┌──────────────────┐
                       │ Continuous Learn │
                       └──────────────────┘
```

## 📊 Database Schema

### Conversations Table
Stores all user interactions for context building:

```sql
- id: Unique conversation identifier  
- user_input: What the user said
- assistant_response: How the computer responded
- timestamp: When the interaction occurred
- context_used: RAG context that influenced the response
- user_satisfaction: Optional 1-5 rating
```

### User Preferences Table  
Learns and stores user behavior patterns:

```sql
- preference_type: Category (response_style, topic_interest, complexity_level)
- key: Specific preference (length, formality, science, etc.)
- value: Preference value (brief, detailed, high, low)
- confidence: How certain we are (0.0-1.0)
- last_updated: When this preference was last observed
```

### Knowledge Entries Table
Builds searchable knowledge base:

```sql
- content: The actual knowledge/information
- source: Where it came from (conversation, search, correction)
- topic: Categorized topic (science, history, etc.)
- embedding: Vector embedding for similarity search
- relevance_score: How useful this knowledge is
- access_count: How often it's been retrieved
```

## 🤖 Learning Mechanisms

### 1. Preference Learning

**Response Style Detection:**
- Detects keywords like "brief", "detailed", "explain", "quickly"
- Learns formality preferences from "please", "thank you"
- Adapts computer personality accordingly

**Topic Interest Tracking:**
- Identifies subject areas user asks about frequently
- Weights topics by frequency and recency
- Prioritizes relevant knowledge in responses

**Complexity Preference:**
- Learns if user prefers technical details or simple explanations
- Adapts Enterprise computer responses to appropriate level

### 2. Knowledge Extraction

**From Conversations:**
- Extracts factual information from Q&A exchanges
- Stores successful explanations for reuse
- Builds topic-categorized knowledge base

**From Search Results:**  
- Stores verified information from fact-checking
- Associates search results with user queries
- Builds reliable knowledge repository

**From Corrections:**
- Learns when user corrects or clarifies information
- Updates knowledge base with verified facts
- Improves future response accuracy

### 3. Context Retrieval

**Similarity Search:**
- Uses vector embeddings to find relevant past conversations
- Retrieves similar knowledge entries for current queries
- Provides contextual information to LLM

**Temporal Context:**
- Considers recent conversation flow
- Maintains thread of ongoing discussions
- References previous interactions appropriately

## 🔧 Configuration Options

```toml
[rag]
enabled = true                    # Enable/disable RAG system
max_conversations = 1000         # Conversation history limit
max_knowledge_entries = 5000     # Knowledge base size limit  
cleanup_days = 30                # Auto-cleanup period
similarity_threshold = 0.7       # Relevance threshold for retrieval
learning_rate = 0.1              # How quickly preferences adapt
```

## 🛠️ Management Tools

### RAG Manager CLI

```bash
# View database statistics
rag-manager stats

# Show recent conversations
rag-manager conversations --limit 20

# View learned preferences  
rag-manager preferences

# Browse knowledge entries
rag-manager knowledge --topic science

# Search knowledge base
rag-manager search "photon torpedo specifications"

# Clean old data
rag-manager cleanup --days 30

# Export data
rag-manager export --format json --output backup.json
```

### Example Usage

```bash
# Check system memory stats
./target/release/rag-manager stats
# Output:
# TOTAL CONVERSATIONS: 247
# KNOWLEDGE ENTRIES: 892  
# USER PREFERENCES: 15
# AVERAGE SATISFACTION: 4

# Search for Star Trek knowledge
./target/release/rag-manager search "warp drive"
# Output:  
# 1. [science] Warp drive enables faster-than-light travel by distorting spacetime...
#    Source: conversation | Relevance: 0.95 | Accessed: 23 times
```

## 🚀 Integration Examples

### User Preference Adaptation

**Before RAG:**
```
User: "Computer, explain quantum mechanics"
Computer: "Quantum mechanics is the fundamental theory in physics..."
```

**After RAG (user prefers brief responses):**
```
User: "Computer, explain quantum mechanics"  
Computer: "Quantum mechanics: physical theory describing atomic-scale behavior. Wave-particle duality and uncertainty principle are key concepts."
```

### Knowledge Base Integration

**Before RAG:**
```
User: "Computer, what did we discuss about warp core efficiency yesterday?"
Computer: "I do not have access to previous conversations."
```

**After RAG:**
```
User: "Computer, what did we discuss about warp core efficiency yesterday?"
Computer: "Previous discussion: warp core efficiency improved 12% with dilithium matrix realignment. Recommended maintenance schedule every 72 hours."
```

### Contextual Responses

**Before RAG:**
```
User: "Computer, continue the previous calculation"
Computer: "Please specify which calculation you are referencing."
```

**After RAG:**
```  
User: "Computer, continue the previous calculation"
Computer: "Continuing trajectory calculation for Mars orbital insertion. Velocity adjustment: 247 meters per second required."
```

## 📈 Performance Metrics

### Learning Effectiveness
- **Preference Accuracy**: How well the system learns user style
- **Context Relevance**: Quality of retrieved information  
- **Response Satisfaction**: User ratings improve over time
- **Knowledge Reuse**: Frequency of knowledge base utilization

### Database Performance
- **Query Speed**: Sub-100ms retrieval for most operations
- **Storage Efficiency**: Automatic cleanup and optimization
- **Embedding Quality**: Vector similarity accuracy
- **Memory Usage**: Optimized for Jetson Nano constraints

## 🔒 Privacy & Security

### Data Protection
- **Local Storage**: All data remains on device
- **No Cloud Sync**: Complete privacy preservation  
- **Encryption**: Sensitive preferences can be encrypted
- **Cleanup**: Automatic removal of old data

### User Control
- **Disable Learning**: Can turn off preference learning
- **Data Export**: Full data export capabilities
- **Selective Deletion**: Remove specific conversations or topics
- **Reset Option**: Complete memory wipe if desired

## 🧪 Development & Testing

### Adding New Learning Patterns

```rust
// Example: Adding new preference detection
async fn learn_complexity_preference(&self, input: &str, response: &str) -> Result<()> {
    if input.contains("simple") || input.contains("basic") {
        self.update_preference("complexity_level", "technical", "low", 0.8).await?;
    } else if input.contains("detailed") || input.contains("technical") {
        self.update_preference("complexity_level", "technical", "high", 0.8).await?;
    }
    Ok(())
}
```

### Custom Knowledge Sources

```rust
// Example: Adding external knowledge integration
async fn import_technical_manual(&self, file_path: &str) -> Result<()> {
    let content = tokio::fs::read_to_string(file_path).await?;
    let entries = parse_technical_manual(&content);
    
    for entry in entries {
        self.store_knowledge_entry(
            &entry.content,
            "technical_manual",  
            &entry.topic,
            1.0 // High relevance for official sources
        ).await?;
    }
    Ok(())
}
```

## 🎯 Future Enhancements

### Planned Features
- **Voice Pattern Recognition**: Learn user's speaking patterns
- **Emotional Adaptation**: Respond to user mood and context
- **Multi-User Support**: Individual profiles for different crew members
- **Federation Database**: Optional connection to Star Trek knowledge base
- **Hologram Integration**: Visual memory association capabilities

### Advanced Learning
- **Reinforcement Learning**: Improve based on user feedback
- **Few-Shot Learning**: Quick adaptation to new user patterns  
- **Federated Learning**: Share knowledge while preserving privacy
- **Causal Inference**: Understanding cause-effect relationships in user behavior

The RAG system transforms the Star Trek Computer from a simple question-answering system into a true artificial intelligence companion that grows and adapts with its user, embodying the advanced computer systems imagined in the Star Trek universe.

---

*"The computer will adapt its responses based on crew preferences and operational requirements, Captain."*