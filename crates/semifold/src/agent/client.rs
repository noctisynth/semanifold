use anyhow::Context;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::config::{AgentConfig, ApiType};

#[derive(Debug, Clone, Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAIChatRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAIChatResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessageResponse,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAIMessageResponse {
    content: String,
}

pub struct AgentClient {
    http_client: Client,
    config: AgentConfig,
}

impl AgentClient {
    pub fn new(config: AgentConfig) -> Self {
        let http_client = Client::new();
        Self {
            http_client,
            config,
        }
    }

    pub async fn chat(&self, prompt: &str) -> anyhow::Result<String> {
        match self.config.api_type {
            ApiType::OpenAI => self.chat_openai(prompt).await,
            ApiType::Anthropic => self.chat_anthropic(prompt).await,
        }
    }

    async fn chat_openai(&self, prompt: &str) -> anyhow::Result<String> {
        let url = format!("{}/v1/chat/completions", self.config.base_url);

        let request = OpenAIChatRequest {
            model: self.config.model.clone(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI compatible API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("API request failed: {} - {}", status, body);
        }

        let chat_response: OpenAIChatResponse =
            response.json().await.context("Failed to parse API response")?;

        chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .context("No response content")
    }

    async fn chat_anthropic(&self, prompt: &str) -> anyhow::Result<String> {
        let url = format!("{}/v1/messages", self.config.base_url);

        #[derive(Serialize)]
        struct AnthropicRequest {
            model: String,
            messages: Vec<AnthropicMessage>,
            max_tokens: u32,
        }

        #[derive(Serialize)]
        struct AnthropicMessage {
            role: String,
            content: String,
        }

        let request = AnthropicRequest {
            model: self.config.model.clone(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            max_tokens: 2048,
        };

        let response = self
            .http_client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Anthropic compatible API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("API request failed: {} - {}", status, body);
        }

        #[derive(Deserialize)]
        struct AnthropicResponse {
            content: Vec<AnthropicContent>,
        }

        #[derive(Deserialize)]
        struct AnthropicContent {
            text: String,
        }

        let anthropic_response: AnthropicResponse =
            response.json().await.context("Failed to parse API response")?;

        anthropic_response
            .content
            .first()
            .map(|c| c.text.clone())
            .context("No response content")
    }
}
