
use std::{fs, future::Future, pin::Pin, sync::{Arc, Mutex}, time::Instant};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use futures::future::select_all;
use reqwest::{Error, Proxy};
use deadpool_redis::Pool;
use redis::AsyncCommands;
use serde_json::json;
use tracing::info;
use yansi::Paint;

use crate::helpers::rolling_average::RollingAverage;

#[derive(Debug, Serialize, Deserialize)]
pub struct GetLatestBlockhashResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: BlockhashResult,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlockhashResult {
    pub context: Context,
    pub value: BlockhashValue,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Context {
    #[serde(rename = "apiVersion")]
    pub api_version: Option<String>,

    pub slot: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlockhashValue {
    pub blockhash: String,

    #[serde(rename = "lastValidBlockHeight")]
    pub last_valid_block_height: u64
}

#[derive(Clone)]
pub struct SlotBlockhash {
    pub slot: u64,
    pub blockhash: String,
    pub  time: Instant,
}

#[derive(Clone)]
pub struct BlockhashProcessor {
    redis_pool: Arc<Pool>,
    proxy_index: Arc<Mutex<usize>>,
    rolling_50: RollingAverage<50>,
    rolling_150: RollingAverage<150>,

    last_slot: Option<u64>,
    last_instant: Option<Instant>,

    recent_hashes: Vec<SlotBlockhash>,
}

#[derive(Debug, Deserialize)]
struct ProxiesConfig {
    proxies: Vec<String>,
}


pub static PROXY_URLS: Lazy<Vec<String>> = Lazy::new(|| {
    let file_path = std::env::var("PROXY_FILE").expect("PROXY_FILE env var not set");
    let contents = fs::read_to_string(&file_path).expect("Could not read proxy file");
    let config: ProxiesConfig = serde_yaml::from_str(&contents).expect("Could not parse proxy file");
    config.proxies
});

const DESIRED_EXPIRATIONS: [u8; 5] = [5, 15, 30, 45, 60];

impl BlockhashProcessor {
    pub async fn new(redis_pool: Arc<Pool>) -> Result<Self, Error> {
        Ok(Self {
            redis_pool,
            proxy_index: Arc::new(Mutex::new(0)),
            rolling_50: RollingAverage::new(),
            rolling_150: RollingAverage::new(),

            last_slot: None,
            last_instant: None,

            recent_hashes: Vec::new(),
        })
    }

    fn parse_proxy_str(&self, proxy_str: &str) -> Result<String, Error> {
        let parts: Vec<&str> = proxy_str.split(':').collect();

        let host = parts[0];
        let port = parts[1];
        let user = parts[2];
        let pass = parts[3];

        let http_url = format!("http://{user}:{pass}@{host}:{port}");
        Ok(http_url)
    }

    async fn send_blockhash_request(&self, proxy_url: &str) -> Result<GetLatestBlockhashResponse, Error> {
        let rpc_url = "https://api.mainnet-beta.solana.com";

        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": [{
                "commitment": "confirmed"
            }]
        });

        let proxy_url = self.parse_proxy_str(&proxy_url)?;
        let client = reqwest::Client::builder()
            .proxy(Proxy::all(&proxy_url)?)
            .build()?;

        let response: GetLatestBlockhashResponse = client
            .post(rpc_url)
            .json(&request_body)
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }

    async fn send_parallel_blockhash_request(&self, workers: usize) -> Result<GetLatestBlockhashResponse, Error> {
        let start_idx = {
            let mut guard = self.proxy_index.lock().unwrap();
            let old = *guard;
            *guard = (old + workers) % PROXY_URLS.len();
            old
        };

        let mut futures = Vec::with_capacity(workers);
        for i in 0..workers {
            let idx = (start_idx + i) % PROXY_URLS.len();
            let proxy_str = &PROXY_URLS[idx];
            
            let this = self.clone();
    
            let fut = Box::pin(async move {
                this.send_blockhash_request(&proxy_str).await
            }) as Pin<Box<dyn Future<Output = Result<GetLatestBlockhashResponse, Error>> + Send>>;
    
            futures.push(fut);
        }

        let (firest_res, _idx, _remaining) = select_all(futures).await;

        firest_res
    }

    pub async fn start_processor(&mut self) -> Result<Self, Error> {
        loop {
            let now = Instant::now();
            let response = self.send_parallel_blockhash_request(2).await?;
            let elapsed = now.elapsed();

            if elapsed.as_millis() < 400 {
                tokio::time::sleep(std::time::Duration::from_millis(
                    (400 - elapsed.as_millis()) as u64
                ))
                .await;
            }
    

            let new_slot = response.result.context.slot;
            if let Some(old_slot) = self.last_slot {
                let slot_diff = new_slot.saturating_sub(old_slot);
                if slot_diff > 0 {
                    let delta_secs = (now - self.last_instant.unwrap()).as_secs_f64();
                    let avg_per_block = delta_secs / (slot_diff as f64);

                    for _ in 0..slot_diff {
                        self.rolling_50.push(avg_per_block);
                        self.rolling_150.push(avg_per_block);
                    }
                    
                    if new_slot % 10 == 0 {
                        info!(
                            "Confirmed slot {} in {:.2}ms, avg slot: ~{:.3}ms {}",
                            Paint::black(new_slot.clone()),
                            delta_secs * 1000.0,
                            Paint::cyan(self.rolling_150.average() * 1000.0),
                            Paint::black(response.result.value.blockhash.clone())
                        );
                    }
                }
            }

            let blockhash = response.result.value.blockhash.clone();

            self.recent_hashes.push(SlotBlockhash {
                slot: new_slot,
                blockhash: blockhash.clone(),
                time: now,
            });
            if self.recent_hashes.len() > 200 {
                let remove_count = self.recent_hashes.len().saturating_sub(200);
                self.recent_hashes.drain(0..remove_count);
            }

            {
                let _ = self.store_expiration_keys(new_slot).await;
            }
    
            // 5) Update last_* references
            self.last_slot = Some(new_slot);
            self.last_instant = Some(now);
        }
    }

    pub async fn store_expiration_keys(&self, current_slot: u64) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = self.redis_pool.get().await?;

        if let Some(latest) = self.recent_hashes.last() {
            conn.set::<&str, String, ()>("recent_blockhash", latest.blockhash.clone()).await?;
        }

        for t in &DESIRED_EXPIRATIONS {
            if let Some(candidate) = self.find_blockhash_closest_to_expiry(current_slot, *t as f64) {
                let key = format!("recent_blockhash_with_expiration:{}", t);
                conn.set::<String, String, ()>(key, candidate).await?;
            }
        }

        Ok(())
    }

    /// # Find the Blockhash Closest to Desired Expiry
    /// To note, we do not need to push mock blockhashes if one is missed given we calculate the remaining time
    /// based on the slot delta and the average block time.
    pub fn find_blockhash_closest_to_expiry(&self, current_slot: u64, target_secs: f64) -> Option<String> {
        let avg_block_time = self.rolling_50.average() as f64; 
        if avg_block_time <= 0.0 {
            return None;
        }

        let mut best_hash: Option<String> = None;
        let mut best_diff = f64::MAX;

        for entry in &self.recent_hashes {
            let slots_passed = current_slot.saturating_sub(entry.slot);
            if slots_passed >= 150 {
                continue;
            }

            let slots_left = 150 - slots_passed;
            let seconds_left = (slots_left as f64) * avg_block_time;

            let diff = (seconds_left - target_secs).abs();
            if diff < best_diff {
                best_diff = diff;
                best_hash = Some(entry.blockhash.clone());
            }
        }

        best_hash
    }
}