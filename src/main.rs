use tokio_tungstenite::tungstenite::Error as WsError;
use tracing_subscriber::EnvFilter;
use tokio::sync::mpsc;
use dotenv::dotenv;
use tracing::info;
use std::env;

pub mod instructions;
pub mod processors;
pub mod messaging;
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

    tokio::spawn(async move {
        let context = zmq::Context::new();
        let publisher = context.socket(zmq::PUB).expect("Failed to create ZMQ socket.");
        publisher.bind("tcp://127.0.0.1:6900").expect("Failed to bind ZMQ socket.");

        while let Some(msg) = rx.recv().await {
            publisher.send(&msg.topic, zmq::SNDMORE).unwrap();
            publisher.send(&msg.payload, 0).unwrap(); 
        }
    });

    let transaction_processor = processors::transactions::TransactionProcessor::new(&api_key, url, tx).await?;

    transaction_processor.start_processor().await;

    Ok(())
}
