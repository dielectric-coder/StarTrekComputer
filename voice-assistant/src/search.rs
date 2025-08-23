use anyhow::Result;
use serde_json::Value;
use tokio::process::Command;
use crate::config::AssistantConfig;
use tracing::{info, error, warn};

#[derive(Debug)]
pub struct SearchResults {
    pub primary_results: Vec<Value>,
    pub fact_check_results: Vec<Value>,
    pub query: String,
}

pub struct WebSearcher {
    config: AssistantConfig,
}

impl WebSearcher {
    pub fn new(config: &AssistantConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub async fn search(&self, query: &str) -> Result<SearchResults> {
        if !self.config.search.enabled {
            return Err(anyhow::anyhow!("Search is disabled"));
        }

        info!("Searching with fact-checking for: {}", query);

        let output = Command::new("python3")
            .arg(&self.config.search.search_script)
            .arg(query)
            .output()
            .await?;

        if output.status.success() {
            let result_text = String::from_utf8_lossy(&output.stdout);
            
            // Parse the enhanced search results
            let parsed_results: Value = serde_json::from_str(&result_text)?;
            
            let primary_results = parsed_results.get("primary_search")
                .and_then(|v| v.as_array())
                .unwrap_or(&Vec::new())
                .clone();
                
            let fact_check_results = parsed_results.get("fact_check")
                .and_then(|v| v.as_array())
                .unwrap_or(&Vec::new())
                .clone();
            
            Ok(SearchResults {
                primary_results,
                fact_check_results,
                query: query.to_string(),
            })
        } else {
            error!("Search script failed: {}", String::from_utf8_lossy(&output.stderr));
            Err(anyhow::anyhow!("Search failed"))
        }
    }

    pub async fn fact_check_only(&self, query: &str) -> Result<Vec<Value>> {
        if !self.config.search.enabled {
            return Err(anyhow::anyhow!("Search is disabled"));
        }

        info!("Fact-checking: {}", query);

        let output = Command::new("python3")
            .arg(&self.config.search.search_script)
            .arg("--fact-check")
            .arg(query)
            .output()
            .await?;

        if output.status.success() {
            let result_text = String::from_utf8_lossy(&output.stdout);
            let results: Vec<Value> = serde_json::from_str(&result_text)?;
            Ok(results)
        } else {
            warn!("Fact-check failed: {}", String::from_utf8_lossy(&output.stderr));
            Ok(Vec::new())
        }
    }
}

