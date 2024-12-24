use deadpool_redis::{Config, Runtime};
use gimpey_db_gateway::SerumMarketClient;
use processors::blockhashes::BlockhashProcessor;
use tokio_tungstenite::tungstenite::Error as WsError;
use tracing_subscriber::EnvFilter;
use tokio::sync::mpsc;
use dotenv::dotenv;
use tracing::info;
use std::{env, io, sync::Arc};

pub mod transaction_helpers;
pub mod instructions;
pub mod processors;
pub mod messaging;
pub mod constants;
pub mod programs;
pub mod helpers;

#[tokio::main]
async fn main() -> Result<(), WsError> {
    dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .with_thread_names(false)
        .init();

    info!("Starting the Helius Node Emulator microservice...");

    let api_key = env::var("HELIUS_RPC_API_KEY").expect("HELIUS_RPC_API_KEY must be set");

    let (tx, mut rx) = mpsc::unbounded_channel::<messaging::MpscMessage>();

    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
    let cfg = Config::from_url(redis_url);
    let pool = cfg.create_pool(Some(Runtime::Tokio1)).expect("Failed to create pool");
    let pool = Arc::new(pool);

    let db_gateway_api_key = env::var("DB_GATEWAY_API_KEY").expect("DB_GATEWAY_API_KEY must be set");
    let serum_market_client = SerumMarketClient::connect(
        "ny.db-gateway.gimpey.com",
        db_gateway_api_key.to_string(),
        true
    ).await.expect("Failed to conect to Serum Market Client.");

    // This task is responsible for receiving messages from each of the individual processors and
    // handling them in their respective manners. For now, each is simply sent across a ZMQ socket
    // to downstream programs - however, another application would be to store these messages in a
    // database for later analysis.
    tokio::spawn(async move {
        let context = zmq::Context::new();
        let publisher = context.socket(zmq::PUB).expect("Failed to create ZMQ socket.");
        publisher.bind("tcp://127.0.0.1:6900").expect("Failed to bind ZMQ socket.");

        while let Some(msg) = rx.recv().await {
            publisher.send(&msg.topic, zmq::SNDMORE).unwrap();
            publisher.send(&msg.payload, 0).unwrap(); 
        }
    });

    let mut blockhash_processor = BlockhashProcessor::new(
        pool.clone()
    ).await.map_err(|e: reqwest::Error| {
        WsError::Io(io::Error::new(io::ErrorKind::Other, e.to_string()))
    })?;

    let transaction_processor = processors::transactions::TransactionProcessor::new(
        &api_key, 
        "atlas-mainnet.helius-rpc.com",
        tx, 
        pool,
        serum_market_client.clone()
    ).await?;

    let blockhash_processor_task = tokio::spawn(async move {
        let _ = blockhash_processor.start_processor().await;
    });

    let transaction_processor_task = tokio::spawn(async move {
        transaction_processor.start_processor().await;
    });

    let _ = tokio::join!(blockhash_processor_task, transaction_processor_task);

    Ok(())
}
