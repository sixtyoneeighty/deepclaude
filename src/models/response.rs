//! Response models for the API endpoints.
//!
//! This module defines the structures used to represent API responses,
//! including chat completions, usage statistics, and streaming events.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Primary response structure for chat API endpoints.
///
/// Contains the complete response from both AI models, including
/// content blocks, usage statistics, and optional raw API responses.
#[derive(Debug, Serialize, Clone)]
pub struct ApiResponse {
    pub created: DateTime<Utc>,
    pub content: Vec<ContentBlock>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deepseek_response: Option<ExternalApiResponse>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gemini_response: Option<ExternalApiResponse>,
    
    pub combined_usage: CombinedUsage,
}

/// A block of content in a response.
///
/// Represents a single piece of content in the response,
/// with its type and actual text content.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// Raw response from an external API.
///
/// Contains the complete response details from an external API
/// call, including status code, headers, and response body.
#[derive(Debug, Serialize, Clone)]
pub struct ExternalApiResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: serde_json::Value,
}

/// Combined usage statistics from both AI models.
///
/// Aggregates token usage and cost information from both
/// DeepSeek and Google API calls.
#[derive(Debug, Serialize, Clone)]
pub struct CombinedUsage {
    pub total_cost: String,
    pub deepseek_usage: DeepSeekUsage,
    pub gemini_usage: GeminiUsage,
}

/// Usage statistics for DeepSeek API calls.
///
/// Tracks token consumption and costs specific to
/// DeepSeek model usage.
#[derive(Debug, Serialize, Clone)]
pub struct DeepSeekUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub reasoning_tokens: u32,
    pub cached_input_tokens: u32,
    pub total_tokens: u32,
    pub total_cost: String,
}

/// Usage statistics for Gemini API calls.
///
/// Tracks token consumption and costs specific to
/// Gemini model usage.
#[derive(Debug, Serialize, Clone)]
pub struct GeminiUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub total_cost: String,
}

// Streaming event types
/// Events emitted during streaming responses.
///
/// Represents different types of events that can occur
/// during a streaming response, including content updates
/// and usage statistics.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "start")]
    Start {
        created: DateTime<Utc>,
    },
    
    #[serde(rename = "content")]
    Content {
        content: Vec<ContentBlock>,
    },
    
    #[serde(rename = "usage")]
    Usage {
        usage: CombinedUsage,
    },
    
    #[serde(rename = "done")]
    Done,
    
    #[serde(rename = "error")]
    Error {
        message: String,
        code: u16,
    },
}

impl ContentBlock {
    /// Creates a new text content block.
    ///
    /// # Arguments
    ///
    /// * `text` - The text content to include in the block
    ///
    /// # Returns
    ///
    /// A new `ContentBlock` with the type set to "text"
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content_type: "text".to_string(),
            text: text.into(),
        }
    }

    /// Converts an Google content block to a generic content block.
    ///
    /// # Arguments
    ///
    /// * `block` - The Google-specific content block to convert
    ///
    /// # Returns
    ///
    /// A new `ContentBlock` with the same content type and text
    pub fn from_gemini(block: crate::clients::gemini::ContentBlock) -> Self {
        Self {
            content_type: block.content_type,
            text: block.text,
        }
    }
}

impl ApiResponse {
    /// Creates a new API response with simple text content.
    ///
    /// # Arguments
    ///
    /// * `content` - The text content for the response
    ///
    /// # Returns
    ///
    /// A new `ApiResponse` with default values and the provided content
    #[allow(dead_code)]
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            created: Utc::now(),
            content: vec![ContentBlock::text(content)],
            deepseek_response: None,
            anthropic_response: None,
            combined_usage: CombinedUsage {
                total_cost: "$0.00".to_string(),
                deepseek_usage: DeepSeekUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    reasoning_tokens: 0,
                    cached_input_tokens: 0,
                    total_tokens: 0,
                    total_cost: "$0.00".to_string(),
                },
                gemini_usage: GeminiUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    total_cost: "$0.00".to_string(),
                },
            },
        }
    }
}

impl GeminiUsage {
    /// Converts Gemini usage statistics to the generic usage format.
    ///
    /// # Arguments
    ///
    /// * `response` - The Gemini response containing usage information
    ///
    /// # Returns
    ///
    /// A new `GeminiUsage` with values from the Gemini response
    pub fn from_gemini(response: &crate::clients::gemini::GeminiResponse) -> Self {
        Self {
            input_tokens: response.usage.as_ref().map(|u| u.prompt_tokens).unwrap_or(0),
            output_tokens: response.usage.as_ref().map(|u| u.completion_tokens).unwrap_or(0),
            total_tokens: response.usage.as_ref().map(|u| u.total_tokens).unwrap_or(0),
            total_cost: "$0.00".to_string(), // Cost will be calculated later
        }
    }
}
