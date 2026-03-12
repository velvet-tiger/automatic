use serde::{Deserialize, Serialize};
use std::fs;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A single model's token estimate with cost information.
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenEstimate {
    /// Display name of the model (e.g. "GPT-4o").
    pub model: String,
    /// Provider name (e.g. "OpenAI", "Anthropic").
    pub provider: String,
    /// Estimated token count.
    pub tokens: usize,
    /// Cost per 1 million input tokens in USD.
    pub cost_per_million_input: f64,
    /// Cost per 1 million output tokens in USD.
    pub cost_per_million_output: f64,
    /// Estimated input cost in USD for this text.
    pub estimated_input_cost: f64,
    /// Method used: "exact" (tiktoken BPE) or "approximate" (char ratio).
    pub method: String,
}

/// Request payload for token estimation.
#[derive(Debug, Deserialize)]
pub struct EstimateTokensRequest {
    /// Raw text content to estimate. Mutually exclusive with `file_path`.
    pub text: Option<String>,
    /// Absolute file path to read and estimate. Mutually exclusive with `text`.
    pub file_path: Option<String>,
}

// ── Encoding helpers ──────────────────────────────────────────────────────────

/// Count tokens using tiktoken's cl100k_base encoding (GPT-4, GPT-3.5-turbo,
/// text-embedding-ada-002). Returns None on encoding failure.
fn count_cl100k(text: &str) -> Option<usize> {
    use tiktoken_rs::cl100k_base;
    let bpe = cl100k_base().ok()?;
    Some(bpe.encode_with_special_tokens(text).len())
}

/// Count tokens using tiktoken's o200k_base encoding (GPT-4o, o1, o3).
fn count_o200k(text: &str) -> Option<usize> {
    use tiktoken_rs::o200k_base;
    let bpe = o200k_base().ok()?;
    Some(bpe.encode_with_special_tokens(text).len())
}

/// Approximate token count using a character-ratio heuristic.
///
/// Ratios are derived from empirical testing against each provider's tokeniser:
/// - Claude: ~3.8 chars/token (Anthropic's BPE variant)
/// - Gemini: ~4.0 chars/token (SentencePiece)
fn approximate_tokens(text: &str, chars_per_token: f64) -> usize {
    let char_count = text.chars().count();
    ((char_count as f64) / chars_per_token).ceil() as usize
}

// ── Command ───────────────────────────────────────────────────────────────────

/// Estimate token counts for a given text or file across multiple LLM providers.
///
/// Uses exact BPE counting (tiktoken) for OpenAI models and character-ratio
/// approximations for providers without public tokenisers (Anthropic, Google).
#[tauri::command]
pub fn estimate_tokens(request: EstimateTokensRequest) -> Result<Vec<TokenEstimate>, String> {
    // Resolve the input text from either the inline `text` field or a file path.
    let text = match (request.text, request.file_path) {
        (Some(t), _) => t,
        (None, Some(path)) => fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read file '{}': {}", path, e))?,
        (None, None) => {
            return Err("Either 'text' or 'file_path' must be provided.".to_string());
        }
    };

    if text.is_empty() {
        return Ok(vec![]);
    }

    // Exact OpenAI counts via tiktoken.
    let cl100k_tokens = count_cl100k(&text).unwrap_or_else(|| approximate_tokens(&text, 4.0));
    let o200k_tokens = count_o200k(&text).unwrap_or_else(|| approximate_tokens(&text, 4.0));
    let cl100k_exact = count_cl100k(&text).is_some();
    let o200k_exact = count_o200k(&text).is_some();

    // Approximate counts for providers without public tokenisers.
    let claude_tokens = approximate_tokens(&text, 3.8);
    let gemini_tokens = approximate_tokens(&text, 4.0);

    let estimates = vec![
        // ── Anthropic ────────────────────────────────────────────────────────
        TokenEstimate {
            model: "Claude 3.5 Sonnet / Haiku".to_string(),
            provider: "Anthropic".to_string(),
            tokens: claude_tokens,
            cost_per_million_input: 3.00,
            cost_per_million_output: 15.00,
            estimated_input_cost: (claude_tokens as f64 / 1_000_000.0) * 3.00,
            method: "approximate".to_string(),
        },
        TokenEstimate {
            model: "Claude 3 Opus".to_string(),
            provider: "Anthropic".to_string(),
            tokens: claude_tokens,
            cost_per_million_input: 15.00,
            cost_per_million_output: 75.00,
            estimated_input_cost: (claude_tokens as f64 / 1_000_000.0) * 15.00,
            method: "approximate".to_string(),
        },
        // ── OpenAI ────────────────────────────────────────────────────────────
        TokenEstimate {
            model: "GPT-4o / GPT-4o mini".to_string(),
            provider: "OpenAI".to_string(),
            tokens: o200k_tokens,
            cost_per_million_input: 2.50,
            cost_per_million_output: 10.00,
            estimated_input_cost: (o200k_tokens as f64 / 1_000_000.0) * 2.50,
            method: if o200k_exact {
                "exact".to_string()
            } else {
                "approximate".to_string()
            },
        },
        TokenEstimate {
            model: "GPT-4 Turbo".to_string(),
            provider: "OpenAI".to_string(),
            tokens: cl100k_tokens,
            cost_per_million_input: 10.00,
            cost_per_million_output: 30.00,
            estimated_input_cost: (cl100k_tokens as f64 / 1_000_000.0) * 10.00,
            method: if cl100k_exact {
                "exact".to_string()
            } else {
                "approximate".to_string()
            },
        },
        TokenEstimate {
            model: "GPT-3.5 Turbo".to_string(),
            provider: "OpenAI".to_string(),
            tokens: cl100k_tokens,
            cost_per_million_input: 0.50,
            cost_per_million_output: 1.50,
            estimated_input_cost: (cl100k_tokens as f64 / 1_000_000.0) * 0.50,
            method: if cl100k_exact {
                "exact".to_string()
            } else {
                "approximate".to_string()
            },
        },
        // ── Google ────────────────────────────────────────────────────────────
        TokenEstimate {
            model: "Gemini 1.5 Pro".to_string(),
            provider: "Google".to_string(),
            tokens: gemini_tokens,
            cost_per_million_input: 1.25,
            cost_per_million_output: 5.00,
            estimated_input_cost: (gemini_tokens as f64 / 1_000_000.0) * 1.25,
            method: "approximate".to_string(),
        },
        TokenEstimate {
            model: "Gemini 1.5 Flash".to_string(),
            provider: "Google".to_string(),
            tokens: gemini_tokens,
            cost_per_million_input: 0.075,
            cost_per_million_output: 0.30,
            estimated_input_cost: (gemini_tokens as f64 / 1_000_000.0) * 0.075,
            method: "approximate".to_string(),
        },
    ];

    Ok(estimates)
}
