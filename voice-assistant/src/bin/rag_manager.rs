#!/usr/bin/env rust
//! RAG Database Management Tool
//! 
//! This tool provides command-line management of the RAG (Retrieval-Augmented Generation)
//! database used by the Star Trek Computer voice assistant.

use anyhow::Result;
use clap::{Arg, Command as ClapCommand, ArgMatches};
use tracing::Level;
use tracing_subscriber;

// We need to include the main crate modules
use voice_assistant::{
    config::AssistantConfig,
    rag::RAGDatabase,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    let app = ClapCommand::new("rag-manager")
        .version("0.1.0")
        .author("Star Trek Computer System")
        .about("Manages the RAG database for the voice assistant")
        .subcommand(
            ClapCommand::new("stats")
                .about("Show database statistics")
        )
        .subcommand(
            ClapCommand::new("conversations")
                .about("List recent conversations")
                .arg(
                    Arg::new("limit")
                        .short('l')
                        .long("limit")
                        .value_name("NUMBER")
                        .help("Number of conversations to show")
                        .default_value("10")
                )
        )
        .subcommand(
            ClapCommand::new("preferences")
                .about("Show user preferences")
        )
        .subcommand(
            ClapCommand::new("knowledge")
                .about("Show knowledge entries")
                .arg(
                    Arg::new("topic")
                        .short('t')
                        .long("topic")
                        .value_name("TOPIC")
                        .help("Filter by topic")
                )
        )
        .subcommand(
            ClapCommand::new("cleanup")
                .about("Clean up old data")
                .arg(
                    Arg::new("days")
                        .short('d')
                        .long("days")
                        .value_name("DAYS")
                        .help("Number of days of data to keep")
                        .default_value("30")
                )
        )
        .subcommand(
            ClapCommand::new("search")
                .about("Search knowledge base")
                .arg(
                    Arg::new("query")
                        .value_name("QUERY")
                        .help("Search query")
                        .required(true)
                        .index(1)
                )
        )
        .subcommand(
            ClapCommand::new("export")
                .about("Export data")
                .arg(
                    Arg::new("format")
                        .short('f')
                        .long("format")
                        .value_name("FORMAT")
                        .help("Export format (json, csv)")
                        .default_value("json")
                )
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_name("FILE")
                        .help("Output file")
                        .default_value("rag_export.json")
                )
        );

    let matches = app.get_matches();

    // Load configuration
    let config = AssistantConfig::load().await?;
    
    // Initialize RAG database
    let rag_db = RAGDatabase::new(&config).await?;

    // Handle subcommands
    match matches.subcommand() {
        Some(("stats", _)) => show_stats(&rag_db).await?,
        Some(("conversations", sub_matches)) => show_conversations(&rag_db, sub_matches).await?,
        Some(("preferences", _)) => show_preferences(&rag_db).await?,
        Some(("knowledge", sub_matches)) => show_knowledge(&rag_db, sub_matches).await?,
        Some(("cleanup", sub_matches)) => cleanup_data(&rag_db, sub_matches).await?,
        Some(("search", sub_matches)) => search_knowledge(&rag_db, sub_matches).await?,
        Some(("export", sub_matches)) => export_data(&rag_db, sub_matches).await?,
        _ => {
            println!("No subcommand provided. Use --help for usage information.");
        }
    }

    Ok(())
}

async fn show_stats(rag_db: &RAGDatabase) -> Result<()> {
    println!("📊 RAG Database Statistics");
    println!("{}", "=".repeat(50));
    
    let stats = rag_db.get_statistics().await?;
    
    for (key, value) in stats {
        let display_key = key.replace("_", " ").to_uppercase();
        println!("{}: {}", display_key, value);
    }
    
    Ok(())
}

async fn show_conversations(_rag_db: &RAGDatabase, matches: &ArgMatches) -> Result<()> {
    let limit = matches.get_one::<String>("limit")
        .unwrap()
        .parse::<i32>()
        .unwrap_or(10);
    
    println!("💬 Recent Conversations (limit: {})", limit);
    println!("{}", "=".repeat(50));
    
    // This would require adding a method to RAGDatabase to get recent conversations
    println!("Feature not yet implemented - would show {} recent conversations", limit);
    
    Ok(())
}

async fn show_preferences(_rag_db: &RAGDatabase) -> Result<()> {
    println!("⚙️  User Preferences");
    println!("{}", "=".repeat(50));
    
    // This would require adding a method to RAGDatabase to get preferences
    println!("Feature not yet implemented - would show user preferences");
    
    Ok(())
}

async fn show_knowledge(_rag_db: &RAGDatabase, matches: &ArgMatches) -> Result<()> {
    let topic_filter = matches.get_one::<String>("topic");
    
    if let Some(topic) = topic_filter {
        println!("📚 Knowledge Entries (topic: {})", topic);
    } else {
        println!("📚 Knowledge Entries (all topics)");
    }
    println!("{}", "=".repeat(50));
    
    // This would require adding a method to RAGDatabase to get knowledge entries
    println!("Feature not yet implemented - would show knowledge entries");
    
    Ok(())
}

async fn cleanup_data(rag_db: &RAGDatabase, matches: &ArgMatches) -> Result<()> {
    let days = matches.get_one::<String>("days")
        .unwrap()
        .parse::<i64>()
        .unwrap_or(30);
    
    println!("🧹 Cleaning up data older than {} days", days);
    
    rag_db.cleanup_old_data(days).await?;
    
    println!("✅ Cleanup completed");
    Ok(())
}

async fn search_knowledge(rag_db: &RAGDatabase, matches: &ArgMatches) -> Result<()> {
    let query = matches.get_one::<String>("query").unwrap();
    
    println!("🔍 Searching for: '{}'", query);
    println!("{}", "=".repeat(50));
    
    let context = rag_db.get_rag_context(query).await?;
    
    if context.knowledge_entries.is_empty() {
        println!("No relevant knowledge entries found.");
    } else {
        for (i, entry) in context.knowledge_entries.iter().enumerate() {
            println!("{}. [{}] {}", 
                i + 1, 
                entry.topic, 
                entry.content.chars().take(100).collect::<String>());
            if entry.content.len() > 100 {
                println!("   ...");
            }
            println!("   Source: {} | Relevance: {:.2} | Accessed: {} times",
                entry.source, entry.relevance_score, entry.access_count);
            println!();
        }
    }
    
    Ok(())
}

async fn export_data(rag_db: &RAGDatabase, matches: &ArgMatches) -> Result<()> {
    let format = matches.get_one::<String>("format").unwrap();
    let output = matches.get_one::<String>("output").unwrap();
    
    println!("📤 Exporting data to {} (format: {})", output, format);
    
    // Get statistics as a simple export
    let stats = rag_db.get_statistics().await?;
    
    match format.as_str() {
        "json" => {
            let json_data = serde_json::to_string_pretty(&stats)?;
            tokio::fs::write(output, json_data).await?;
        }
        "csv" => {
            let mut csv_data = String::from("metric,value\n");
            for (key, value) in stats {
                csv_data.push_str(&format!("{},{}\n", key, value));
            }
            tokio::fs::write(output, csv_data).await?;
        }
        _ => {
            return Err(anyhow::anyhow!("Unsupported format: {}", format));
        }
    }
    
    println!("✅ Export completed: {}", output);
    Ok(())
}