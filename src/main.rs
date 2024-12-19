use deadpool_redis::{Config, Runtime};
use tokio_tungstenite::tungstenite::Error as WsError;
use tracing_subscriber::EnvFilter;
use tokio::sync::mpsc;
use dotenv::dotenv;
use tracing::info;
use std::{env, sync::Arc};

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

    let url = "atlas-mainnet.helius-rpc.com";
    let api_key = env::var("HELIUS_RPC_API_KEY").expect("HELIUS_RPC_API_KEY must be set");

    let (tx, mut rx) = mpsc::unbounded_channel::<messaging::MpscMessage>();

    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
    let cfg = Config::from_url(redis_url);
    let pool = cfg.create_pool(Some(Runtime::Tokio1)).expect("Failed to create pool");
    let pool = Arc::new(pool);

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

    let transaction_processor = processors::transactions::TransactionProcessor::new(
        &api_key, 
        url, 
        tx, 
        pool
    ).await?;
    transaction_processor.start_processor().await;

    Ok(())
}
