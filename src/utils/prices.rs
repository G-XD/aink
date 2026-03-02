//! Dynamic model pricing fetched from litellm's public price table.
//!
//! Downloads model pricing data from GitHub and caches it locally for one day.
//! If the download fails or times out (3 s), all prices fall back to zero.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use serde::Deserialize;
use tracing::{debug, warn};

use crate::collector::transcript::ModelStats;

const PRICE_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
const CACHE_FILENAME: &str = "model_prices.json";
const CACHE_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(3);

/// Per-token costs for a single model.
#[derive(Clone, Debug, Default)]
pub struct ModelPricing {
    pub input_cost_per_token: f64,
    pub output_cost_per_token: f64,
    pub cache_creation_input_token_cost: f64,
    pub cache_read_input_token_cost: f64,
}

/// Deserialization target — only the fields we care about.
#[derive(Deserialize)]
struct RawModelEntry {
    #[serde(default)]
    input_cost_per_token: Option<f64>,
    #[serde(default)]
    output_cost_per_token: Option<f64>,
    #[serde(default)]
    cache_creation_input_token_cost: Option<f64>,
    #[serde(default)]
    cache_read_input_token_cost: Option<f64>,
}

type RawPriceMap = HashMap<String, RawModelEntry>;

/// In-memory lookup table: model name → pricing.
pub struct PriceTable {
    models: HashMap<String, ModelPricing>,
}

impl PriceTable {
    fn empty() -> Self {
        Self {
            models: HashMap::new(),
        }
    }

    fn from_raw(raw: RawPriceMap) -> Self {
        let models = raw
            .into_iter()
            .filter_map(|(name, entry)| {
                if entry.input_cost_per_token.is_none() && entry.output_cost_per_token.is_none() {
                    return None;
                }
                Some((
                    name,
                    ModelPricing {
                        input_cost_per_token: entry.input_cost_per_token.unwrap_or(0.0),
                        output_cost_per_token: entry.output_cost_per_token.unwrap_or(0.0),
                        cache_creation_input_token_cost: entry
                            .cache_creation_input_token_cost
                            .unwrap_or(0.0),
                        cache_read_input_token_cost: entry
                            .cache_read_input_token_cost
                            .unwrap_or(0.0),
                    },
                ))
            })
            .collect();
        Self { models }
    }

    /// Look up pricing for a model name. Tries exact match first, then
    /// substring matching against all known model keys.
    pub fn get(&self, model: &str) -> Option<&ModelPricing> {
        if let Some(p) = self.models.get(model) {
            return Some(p);
        }

        let lower = model.to_lowercase();
        if let Some(p) = self.models.get(&lower) {
            return Some(p);
        }

        // Substring: find the longest key that is contained in the model string
        // (e.g. model "claude-sonnet-4-20250514" matches key "claude-sonnet-4-20250514").
        // If no key is contained in the model string, check if the model string
        // is contained in any key.
        let mut best: Option<(&str, &ModelPricing)> = None;
        for (key, pricing) in &self.models {
            if lower.contains(key.as_str()) && best.is_none_or(|(k, _)| key.len() > k.len()) {
                best = Some((key.as_str(), pricing));
            }
        }
        if let Some((_, p)) = best {
            return Some(p);
        }

        // Reverse: check if any key contains the model string
        let mut best_rev: Option<(&str, &ModelPricing)> = None;
        for (key, pricing) in &self.models {
            if key.contains(lower.as_str()) && best_rev.is_none_or(|(k, _)| key.len() < k.len()) {
                best_rev = Some((key.as_str(), pricing));
            }
        }
        best_rev.map(|(_, p)| p)
    }

    /// Estimate the cost (in USD) for a model's token usage.
    pub fn estimate_cost(&self, model: &str, stats: &ModelStats) -> f64 {
        let Some(p) = self.get(model) else {
            if stats.input_tokens + stats.output_tokens > 0 {
                debug!("no pricing found for model {:?}", model);
            }
            return 0.0;
        };
        stats.input_tokens as f64 * p.input_cost_per_token
            + stats.output_tokens as f64 * p.output_cost_per_token
            + stats.cache_creation_tokens as f64 * p.cache_creation_input_token_cost
            + stats.cache_read_tokens as f64 * p.cache_read_input_token_cost
    }

    /// Ensures a "cursor" fallback pricing entry exists for Cursor sessions
    /// that have no model name in the transcript. Uses a conservative
    /// average ($3/1M input, $15/1M output) so cost is non-zero.
    fn with_cursor_fallback(mut self) -> Self {
        if !self.models.contains_key("cursor") {
            self.models.insert(
                "cursor".to_string(),
                ModelPricing {
                    input_cost_per_token: 3e-6,   // $3/1M input
                    output_cost_per_token: 15e-6, // $15/1M output
                    cache_creation_input_token_cost: 0.0,
                    cache_read_input_token_cost: 0.0,
                },
            );
        }
        self
    }
}

// ── Global singleton ─────────────────────────────────────────────────────────

static PRICE_TABLE: OnceLock<PriceTable> = OnceLock::new();

/// Get (or initialize) the global price table.
///
/// On first call this will try to load from cache or download. Subsequent
/// calls return the cached reference instantly. Thread-safe.
pub fn price_table() -> &'static PriceTable {
    PRICE_TABLE.get_or_init(load_price_table)
}

/// Estimate cost for a model using the global price table. Returns 0.0 on any
/// lookup failure.
pub fn estimate_cost(model: &str, stats: &ModelStats) -> f64 {
    price_table().estimate_cost(model, stats)
}

// ── Loading logic ────────────────────────────────────────────────────────────

fn cache_path() -> PathBuf {
    crate::config::get_data_dir().join(CACHE_FILENAME)
}

fn cache_is_fresh(path: &PathBuf) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|mtime| {
            SystemTime::now()
                .duration_since(mtime)
                .unwrap_or(Duration::MAX)
                < CACHE_MAX_AGE
        })
        .unwrap_or(false)
}

fn load_price_table() -> PriceTable {
    let path = cache_path();

    if cache_is_fresh(&path)
        && let Some(table) = load_from_cache(&path)
    {
        debug!("loaded {} model prices from cache", table.models.len());
        return table.with_cursor_fallback();
    }

    if let Some(bytes) = download_blocking()
        && let Some(table) = parse_and_cache(&bytes, &path)
    {
        debug!("downloaded {} model prices", table.models.len());
        return table.with_cursor_fallback();
    }

    if path.exists()
        && let Some(table) = load_from_cache(&path)
    {
        warn!("using stale price cache ({} models)", table.models.len());
        return table.with_cursor_fallback();
    }

    warn!("no pricing data available — all costs will be zero");
    PriceTable::empty().with_cursor_fallback()
}

fn load_from_cache(path: &PathBuf) -> Option<PriceTable> {
    let bytes = std::fs::read(path).ok()?;
    let raw: RawPriceMap = serde_json::from_slice(&bytes).ok()?;
    Some(PriceTable::from_raw(raw))
}

fn parse_and_cache(bytes: &[u8], cache_path: &PathBuf) -> Option<PriceTable> {
    let raw: RawPriceMap = serde_json::from_slice(bytes).ok()?;
    let table = PriceTable::from_raw(raw);
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(cache_path, bytes) {
        warn!("failed to write price cache: {e}");
    }
    Some(table)
}

fn download_blocking() -> Option<Vec<u8>> {
    // Try to use the existing tokio runtime if we're inside one;
    // otherwise spin up a small one-shot runtime.
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        // We're inside an async context — spawn a blocking task so we don't
        // deadlock the executor.
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(async { download().await }))
                .join()
                .ok()
                .flatten()
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        rt.block_on(async { download().await })
    }
}

async fn download() -> Option<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .ok()?;
    match client.get(PRICE_URL).send().await {
        Ok(resp) if resp.status().is_success() => resp.bytes().await.ok().map(|b| b.to_vec()),
        Ok(resp) => {
            warn!("price download returned HTTP {}", resp.status());
            None
        }
        Err(e) => {
            warn!("price download failed: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_returns_zero_cost() {
        let table = PriceTable::empty();
        let stats = ModelStats {
            input_tokens: 1000,
            output_tokens: 500,
            ..Default::default()
        };
        assert_eq!(table.estimate_cost("anything", &stats), 0.0);
    }

    #[test]
    fn exact_match_lookup() {
        let mut models = HashMap::new();
        models.insert(
            "claude-sonnet-4-20250514".to_string(),
            ModelPricing {
                input_cost_per_token: 3e-6,
                output_cost_per_token: 15e-6,
                cache_creation_input_token_cost: 3.75e-6,
                cache_read_input_token_cost: 0.3e-6,
            },
        );
        let table = PriceTable { models };
        let stats = ModelStats {
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            cache_creation_tokens: 50_000,
            cache_read_tokens: 200_000,
            tool_call_count: 0,
        };
        let cost = table.estimate_cost("claude-sonnet-4-20250514", &stats);
        let expected =
            1_000_000.0 * 3e-6 + 100_000.0 * 15e-6 + 50_000.0 * 3.75e-6 + 200_000.0 * 0.3e-6;
        assert!(
            (cost - expected).abs() < 1e-6,
            "got {cost}, expected {expected}"
        );
    }

    #[test]
    fn substring_match_lookup() {
        let mut models = HashMap::new();
        models.insert(
            "claude-sonnet-4-20250514".to_string(),
            ModelPricing {
                input_cost_per_token: 3e-6,
                output_cost_per_token: 15e-6,
                ..Default::default()
            },
        );
        let table = PriceTable { models };
        assert!(table.get("claude-sonnet-4-20250514").is_some());
        assert!(table.get("Claude-Sonnet-4-20250514").is_some());
    }
}
