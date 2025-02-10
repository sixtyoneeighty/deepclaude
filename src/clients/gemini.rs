use std::{collections::HashMap, pin::Pin};
use futures::Stream;
use google_generative_ai_rs::{
    client::Client,
    types::{GenerateContentRequest, GenerateContentResponse, Part},
};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use crate::{
    models::{ApiConfig, Message},
    error::Result,
};

/// Client for interacting with Google's Gemini AI models.
///
/// This client handles authentication, request construction, and response parsing
/// for both streaming and non-streaming interactions with the Gemini API.
///
/// # Examples
///
/// ```no_run
/// use deepclaude::clients::GeminiClient;
///
/// let client = GeminiClient::new("api_token".to_string());
/// ```
#[derive(Debug, Clone)]
pub struct GeminiClient {
    client: Client,
    model: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GeminiResponse {
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Choice {
    pub message: AssistantMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AssistantMessage {
    pub role: String,
    pub content: String,
}

// Streaming response types
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StreamChoice {
    pub delta: StreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StreamDelta {
    pub role: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StreamResponse {
    pub id: String,
    pub choices: Vec<StreamChoice>,
    pub created: u64,
    pub model: String,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PromptTokensDetails {
    pub total_tokens: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CompletionTokensDetails {
    pub total_tokens: u32,
}

impl GeminiClient {
    /// Creates a new GeminiClient with the specified API token.
    pub fn new(api_token: String) -> Self {
        Self {
            client: Client::new(api_token),
            model: "gemini-2.0-pro-exp".to_string(),
        }
    }

    /// Sends a non-streaming chat request to the Gemini API.
    ///
    /// # Arguments
    ///
    /// * `messages` - Vector of messages for the conversation
    /// * `config` - Configuration options for the request
    ///
    /// # Returns
    ///
    /// * `Result<GeminiResponse>` - The model's response on success
    pub async fn chat(
        &self,
        messages: Vec<Message>,
        config: &ApiConfig,
    ) -> Result<GeminiResponse> {
        let request = self.build_request(messages, config);
        let response = self.client.generate_content(request).await?;
        
        Ok(self.convert_response(response))
    }

    /// Sends a streaming chat request to the Gemini API.
    ///
    /// Returns a stream that yields chunks of the model's response as they arrive.
    ///
    /// # Arguments
    ///
    /// * `messages` - Vector of messages for the conversation
    /// * `config` - Configuration options for the request
    ///
    /// # Returns
    ///
    /// * `Pin<Box<dyn Stream<Item = Result<StreamResponse>> + Send>>` - A stream of response chunks
    pub fn chat_stream(
        &self,
        messages: Vec<Message>,
        config: &ApiConfig,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamResponse>> + Send>> {
        let request = self.build_request(messages, config);
        let stream = self.client.generate_content_stream(request);
        
        Box::pin(stream.map(|result| {
            result.map_err(Into::into).map(|response| self.convert_stream_response(response))
        }))
    }

    /// Builds a GenerateContentRequest for the Gemini API.
    fn build_request(&self, messages: Vec<Message>, config: &ApiConfig) -> GenerateContentRequest {
        let contents: Vec<Part> = messages.into_iter().map(|msg| {
            Part::text(msg.content)
        }).collect();

        GenerateContentRequest::new(&self.model, contents)
            .temperature(config.temperature)
            .max_output_tokens(config.max_tokens.unwrap_or(2048))
            .top_p(config.top_p)
    }

    /// Converts a Gemini response to our internal GeminiResponse format
    fn convert_response(&self, response: GenerateContentResponse) -> GeminiResponse {
        // TODO: Implement proper conversion from Gemini response format
        GeminiResponse {
            choices: vec![Choice {
                message: AssistantMessage {
                    role: "assistant".to_string(),
                    content: response.text().unwrap_or_default().to_string(),
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: None, // Gemini API currently doesn't provide detailed token usage
        }
    }

    /// Converts a Gemini streaming response to our internal StreamResponse format
    fn convert_stream_response(&self, response: GenerateContentResponse) -> StreamResponse {
        StreamResponse {
            id: "gemini".to_string(), // Gemini doesn't provide response IDs
            choices: vec![StreamChoice {
                delta: StreamDelta {
                    role: Some("assistant".to_string()),
                    content: Some(response.text().unwrap_or_default().to_string()),
                },
                finish_reason: None,
            }],
            created: chrono::Utc::now().timestamp() as u64,
            model: self.model.clone(),
            usage: None,
        }
    }
}
