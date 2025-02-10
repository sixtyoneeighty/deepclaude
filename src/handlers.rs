//! Request handlers for the API endpoints.
//!
//! This module contains the main request handlers and supporting functions
//! for processing chat requests, including both streaming and non-streaming
//! responses. It coordinates between different AI models and handles
//! usage tracking and cost calculations.

use crate::{
    clients::{AnthropicClient, DeepSeekClient},
    config::Config,
    error::{ApiError, Result, SseResponse},
    models::{
        ApiRequest, ApiResponse, ContentBlock, CombinedUsage, DeepSeekUsage, AnthropicUsage,
        ExternalApiResponse, Message, Role, StreamEvent,
    },
};
use axum::{
    extract::State,
    response::{sse::Event, IntoResponse},
    Json,
};
use chrono::Utc;
use futures::StreamExt;
use std::{sync::Arc, collections::HashMap};
use tokio_stream::wrappers::ReceiverStream;

/// Application state shared across request handlers.
///
/// Contains configuration that needs to be accessible
/// to all request handlers.
pub struct AppState {
    pub config: Config,
}

/// Extracts API tokens from request headers.
///
/// # Arguments
///
/// * `headers` - The HTTP headers containing the API tokens
///
/// # Returns
///
/// * `Result<(String, String)>` - A tuple of (DeepSeek token, Gemini token)
///
/// # Errors
///
/// Returns `ApiError::MissingHeader` if either token is missing
/// Returns `ApiError::BadRequest` if tokens are malformed
fn extract_api_tokens(
    headers: &axum::http::HeaderMap,
) -> Result<(String, String)> {
    let deepseek_token = headers
        .get("X-DeepSeek-API-Token")
        .ok_or_else(|| ApiError::MissingHeader { 
            header: "X-DeepSeek-API-Token".to_string() 
        })?
        .to_str()
        .map_err(|_| ApiError::BadRequest { 
            message: "Invalid DeepSeek API token".to_string() 
        })?
        .to_string();

    let gemini_token = headers
        .get("X-Gemini-API-Token")
        .ok_or_else(|| ApiError::MissingHeader { 
            header: "X-Gemini-API-Token".to_string() 
        })?
        .to_str()
        .map_err(|_| ApiError::BadRequest { 
            message: "Invalid Gemini API token".to_string() 
        })?
        .to_string();

    Ok((deepseek_token, gemini_token))
}

/// Calculates the cost of DeepSeek API usage.
///
/// # Arguments
///
/// * `input_tokens` - Number of input tokens processed
/// * `output_tokens` - Number of output tokens generated
/// * `_reasoning_tokens` - Number of tokens used for reasoning
/// * `cached_tokens` - Number of tokens retrieved from cache
/// * `config` - Configuration containing pricing information
///
/// # Returns
///
/// The total cost in dollars for the API usage
fn calculate_deepseek_cost(
    input_tokens: u32,
    output_tokens: u32,
    _reasoning_tokens: u32,
    cached_tokens: u32,
    config: &Config,
) -> f64 {
    let cache_hit_cost = (cached_tokens as f64 / 1_000_000.0) * config.pricing.deepseek.input_cache_hit_price;
    let cache_miss_cost = ((input_tokens - cached_tokens) as f64 / 1_000_000.0) * config.pricing.deepseek.input_cache_miss_price;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * config.pricing.deepseek.output_price;
    
    cache_hit_cost + cache_miss_cost + output_cost
}

/// Calculates the cost of Gemini API usage.
///
/// # Arguments
///
/// * `input_tokens` - Number of input tokens processed
/// * `output_tokens` - Number of output tokens generated
/// * `config` - Configuration containing pricing information
///
/// # Returns
///
/// The total cost in dollars for the API usage
fn calculate_gemini_cost(
    input_tokens: u32,
    output_tokens: u32,
    config: &Config,
) -> f64 {
    let pricing = &config.pricing.gemini.gemini_pro;

    let input_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_price;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_price;

    input_cost + output_cost
}

/// Formats a cost value as a dollar amount string.
///
/// # Arguments
///
/// * `cost` - The cost value to format
///
/// # Returns
///
/// A string representing the cost with 3 decimal places and $ prefix
fn format_cost(cost: f64) -> String {
    format!("${:.3}", cost)
}

/// Main handler for chat requests.
///
/// Routes requests to either streaming or non-streaming handlers
/// based on the request configuration.
///
/// # Arguments
///
/// * `state` - Application state containing configuration
/// * `headers` - HTTP request headers
/// * `request` - The parsed chat request
///
/// # Returns
///
/// * `Result<Response>` - The API response or an error
pub async fn handle_chat(
    state: State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ApiRequest>,
) -> Result<axum::response::Response> {
    if request.stream {
        let stream_response = chat_stream(state, headers, Json(request)).await?;
        Ok(stream_response.into_response())
    } else {
        let json_response = chat(state, headers, Json(request)).await?;
        Ok(json_response.into_response())
    }
}

/// Handler for non-streaming chat requests.
///
/// Processes the request through both AI models sequentially,
/// combining their responses and tracking usage.
///
/// # Arguments
///
/// * `state` - Application state containing configuration
/// * `headers` - HTTP request headers
/// * `request` - The parsed chat request
///
/// # Returns
///
/// * `Result<Json<ApiResponse>>` - The combined API response or an error
pub(crate) async fn chat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ApiRequest>,
) -> Result<Json<ApiResponse>> {
    // Validate system prompt
    if !request.validate_system_prompt() {
        return Err(ApiError::InvalidSystemPrompt);
    }

    // Extract API tokens
    let (deepseek_token, anthropic_token) = extract_api_tokens(&headers)?;

    // Initialize clients
    let deepseek_client = DeepSeekClient::new(deepseek_token);
    let google_client =Googlkelient::new(google_token);

    // Get messages with system prompt
    let messages = request.get_messages_with_system();

    // Call DeepSeek API
    let deepseek_response = deepseek_client.chat(messages.clone(), &request.deepseek_config).await?;
    
    // Store response metadata
    let deepseek_status: u16 = 200;
    let deepseek_headers = HashMap::new(); // Headers not available when using high-level chat method

    // Extract reasoning content and wrap in thinking tags
    let reasoning_content = deepseek_response
        .choices
        .first()
        .and_then(|c| c.message.reasoning_content.as_ref())
        .ok_or_else(|| ApiError::DeepSeekError { 
            message: "No reasoning content in response".to_string(),
            type_: "missing_content".to_string(),
            param: None,
            code: None
        })?;

    let thinking_content = format!("<thinking>\n{}\n</thinking>", reasoning_content);

    // Add thinking content to messages for Gemini
    let mut gemini_messages = messages;
    gemini_messages.push(Message {
        role: Role::Assistant,
        content: thinking_content.clone(),
    });

    // Call Gemini API
    let gemini_response = gemini_client.chat(
        gemini_messages,
        request.get_system_prompt().map(String::from),
        &request.gemini_config
    ).await?;
    
    // Store response metadata
    let gemini_status: u16 = 200;
    let gemini_headers = HashMap::new(); // Headers not available when using high-level chat method

    // Calculate usage costs
    let deepseek_cost = calculate_deepseek_cost(
        deepseek_response.usage.prompt_tokens,
        deepseek_response.usage.completion_tokens,
        deepseek_response.usage.completion_tokens_details.reasoning_tokens,
        deepseek_response.usage.prompt_tokens_details.cached_tokens,
        &state.config,
    );

    let gemini_cost = calculate_gemini_cost(
        gemini_response.usage.as_ref().map(|u| u.prompt_tokens).unwrap_or(0),
        gemini_response.usage.as_ref().map(|u| u.completion_tokens).unwrap_or(0),
        &state.config,
    );

    // Combine thinking content with Gemini's response
    let mut content = Vec::new();
    
    // Add thinking block first
    content.push(ContentBlock::text(thinking_content));
    
    // Add Gemini's response blocks
    content.extend(gemini_response.content.clone().into_iter()
        .map(ContentBlock::from_gemini));

    // Build response with captured headers
    let response = ApiResponse {
        created: Utc::now(),
        content,
        deepseek_response: request.verbose.then(|| ExternalApiResponse {
            status: deepseek_status,
            headers: deepseek_headers,
            body: serde_json::to_value(&deepseek_response).unwrap_or_default(),
        }),
        gemini_response: request.verbose.then(|| ExternalApiResponse {
            status: gemini_status,
            headers: gemini_headers,
            body: serde_json::to_value(&gemini_response).unwrap_or_default(),
        }),
        combined_usage: CombinedUsage {
            total_cost: format_cost(deepseek_cost + anthropic_cost),
            deepseek_usage: DeepSeekUsage {
                input_tokens: deepseek_response.usage.prompt_tokens,
                output_tokens: deepseek_response.usage.completion_tokens,
                reasoning_tokens: deepseek_response.usage.completion_tokens_details.reasoning_tokens,
                cached_input_tokens: deepseek_response.usage.prompt_tokens_details.cached_tokens,
                total_tokens: deepseek_response.usage.total_tokens,
                total_cost: format_cost(deepseek_cost),
            },
            gemini_usage: GeminiUsage {
                input_tokens: gemini_response.usage.as_ref().map(|u| u.prompt_tokens).unwrap_or(0),
                output_tokens: gemini_response.usage.as_ref().map(|u| u.completion_tokens).unwrap_or(0),
                total_tokens: gemini_response.usage.as_ref().map(|u| u.total_tokens).unwrap_or(0),
                total_cost: format_cost(gemini_cost),
            },
        },
    };

    Ok(Json(response))
}

/// Handler for streaming chat requests.
///
/// Processes the request through both AI models sequentially,
/// streaming their responses as Server-Sent Events.
///
/// # Arguments
///
/// * `state` - Application state containing configuration
/// * `headers` - HTTP request headers
/// * `request` - The parsed chat request
///
/// # Returns
///
/// * `Result<SseResponse>` - A stream of Server-Sent Events or an error
pub(crate) async fn chat_stream(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ApiRequest>,
) -> Result<SseResponse> {
    // Validate system prompt
    if !request.validate_system_prompt() {
        return Err(ApiError::InvalidSystemPrompt);
    }

    // Extract API tokens
    let (deepseek_token, gemini_token) = extract_api_tokens(&headers)?;

    // Initialize clients
    let deepseek_client = DeepSeekClient::new(deepseek_token);
    let gemini_client = GeminiClient::new(gemini_token);

    // Get messages with system prompt
    let messages = request.get_messages_with_system();

    // Create channel for stream events
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let tx = Arc::new(tx);

    // Spawn task to handle streaming
    let config = state.config.clone();
    let request_clone = request.clone();
    tokio::spawn(async move {
        let tx = tx.clone();

        // Start event
        let _ = tx
            .send(Ok(Event::default().event("start").data(
                serde_json::to_string(&StreamEvent::Start {
                    created: Utc::now(),
                })
                .unwrap_or_default(),
            )))
            .await;

        // Send initial thinking tag
        let _ = tx
            .send(Ok(Event::default().event("content").data(
                serde_json::to_string(&StreamEvent::Content {
                    content: vec![ContentBlock {
                        content_type: "text".to_string(),
                        text: "<thinking>\n".to_string(),
                    }],
                })
                .unwrap_or_default(),
            )))
            .await;

        // Stream from DeepSeek
        let mut deepseek_usage = None;
        let mut complete_reasoning = String::new();
        let mut deepseek_stream = deepseek_client.chat_stream(messages.clone(), &request_clone.deepseek_config);
        
        while let Some(chunk) = deepseek_stream.next().await {
            match chunk {
                Ok(response) => {
                    if let Some(choice) = response.choices.first() {
                        // Check if reasoning_content is null and break if it is
                        if choice.delta.reasoning_content.is_none() {
                            break;
                        }

                        // Handle delta reasoning_content for streaming
                        if let Some(reasoning) = &choice.delta.reasoning_content {
                            if !reasoning.is_empty() {
                                // Stream the reasoning content as a delta
                                let _ = tx
                                    .send(Ok(Event::default().event("content").data(
                                        serde_json::to_string(&StreamEvent::Content {
                                            content: vec![ContentBlock {
                                                content_type: "text_delta".to_string(),
                                                text: reasoning.to_string(),
                                            }],
                                        })
                                        .unwrap_or_default(),
                                    )))
                                    .await;
                                
                                // Accumulate complete reasoning for later use
                                complete_reasoning.push_str(reasoning);
                            }
                        }
                    }
                    
                    // Store usage information if present
                    if let Some(usage) = response.usage {
                        deepseek_usage = Some(usage);
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(
                            serde_json::to_string(&StreamEvent::Error {
                                message: e.to_string(),
                                code: 500,
                            })
                            .unwrap_or_default(),
                        )))
                        .await;
                    return;
                }
            }
        }

        // Send closing thinking tag
        let _ = tx
            .send(Ok(Event::default().event("content").data(
                serde_json::to_string(&StreamEvent::Content {
                    content: vec![ContentBlock {
                        content_type: "text".to_string(),
                        text: "\n</thinking>".to_string(),
                    }],
                })
                .unwrap_or_default(),
            )))
            .await;

        // Add complete thinking content to messages for Gemini
        let mut gemini_messages = messages;
        gemini_messages.push(Message {
            role: Role::Assistant,
            content: format!("<thinking>\n{}\n</thinking>", complete_reasoning),
        });

        // Stream from Gemini
        let mut gemini_stream = gemini_client.chat_stream(
            gemini_messages,
            request_clone.get_system_prompt().map(String::from),
            &request_clone.gemini_config,
        );

        while let Some(chunk) = gemini_stream.next().await {
            match chunk {
                Ok(event) => match event {
                    crate::clients::gemini::StreamEvent::MessageStart { message } => {
                        // Only send content event if there's actual content to send
                        if !message.content.is_empty() {
                            let _ = tx
                                .send(Ok(Event::default().event("content").data(
                                    serde_json::to_string(&StreamEvent::Content { 
                                        content: message.content.into_iter()
                                            .map(ContentBlock::from_gemini)
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                )))
                                .await;
                        }
                    }
                    crate::clients::gemini::StreamEvent::ContentBlockDelta { delta, .. } => {
                        // Send content update
                        let _ = tx
                            .send(Ok(Event::default().event("content").data(
                                serde_json::to_string(&StreamEvent::Content {
                                    content: vec![ContentBlock {
                                        content_type: delta.delta_type,
                                        text: delta.text,
                                    }],
                                })
                                .unwrap_or_default(),
                            )))
                            .await;
                    }
                    crate::clients::gemini::StreamEvent::MessageDelta { usage, .. } => {
                        // Send final usage stats if available
                        if let Some(usage) = usage {
                            let gemini_usage = GeminiUsage::from_gemini(&usage);
                            let gemini_cost = calculate_gemini_cost(
                                gemini_usage.input_tokens,
                                gemini_usage.output_tokens,
                                &config,
                            );

                            // Calculate DeepSeek costs if usage is available
                            let (deepseek_usage, deepseek_cost) = if let Some(usage) = deepseek_usage.as_ref() {
                                let cost = calculate_deepseek_cost(
                                    usage.prompt_tokens,
                                    usage.completion_tokens,
                                    usage.completion_tokens_details.reasoning_tokens,
                                    usage.prompt_tokens_details.cached_tokens,
                                    &config,
                                );
                                
                                (DeepSeekUsage {
                                    input_tokens: usage.prompt_tokens,
                                    output_tokens: usage.completion_tokens,
                                    reasoning_tokens: usage.completion_tokens_details.reasoning_tokens,
                                    cached_input_tokens: usage.prompt_tokens_details.cached_tokens,
                                    total_tokens: usage.total_tokens,
                                    total_cost: format_cost(cost),
                                }, cost)
                            } else {
                                (DeepSeekUsage {
                                    input_tokens: 0,
                                    output_tokens: 0,
                                    reasoning_tokens: 0,
                                    cached_input_tokens: 0,
                                    total_tokens: 0,
                                    total_cost: "$0.00".to_string(),
                                }, 0.0)
                            };

                            let _ = tx
                                .send(Ok(Event::default().event("usage").data(
                                    serde_json::to_string(&StreamEvent::Usage {
                                        usage: CombinedUsage {
                                            total_cost: format_cost(deepseek_cost + gemini_cost),
                                            deepseek_usage,
                                            gemini_usage: GeminiUsage {
                                                input_tokens: gemini_usage.input_tokens,
                                                output_tokens: gemini_usage.output_tokens,
                                                total_tokens: gemini_usage.total_tokens,
                                                total_cost: format_cost(gemini_cost),
                                            },
                                        },
                                    })
                                    .unwrap_or_default(),
                                )))
                                .await;
                        }
                    }
                    _ => {} // Handle other events if needed
                },
                Err(e) => {
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(
                            serde_json::to_string(&StreamEvent::Error {
                                message: e.to_string(),
                                code: 500,
                            })
                            .unwrap_or_default(),
                        )))
                        .await;
                    return;
                }
            }
        }

        // Send done event
        let _ = tx
            .send(Ok(Event::default().event("done").data(
                serde_json::to_string(&StreamEvent::Done)
                    .unwrap_or_default(),
            )))
            .await;
    });

    // Convert receiver into stream
    let stream = ReceiverStream::new(rx);
    Ok(SseResponse::new(stream))
}
