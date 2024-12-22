use solana_transaction_status::{
    EncodedTransaction, 
    EncodedTransactionWithStatusMeta, 
    UiCompiledInstruction, 
    UiInstruction, 
    UiMessage, 
    UiParsedInstruction
};
use tokio_tungstenite::{
    connect_async, 
    MaybeTlsStream, 
    WebSocketStream, 
    tungstenite::{Error as WsError, Message as WsMessage}
};
use solana_transaction_status::option_serializer::OptionSerializer;
use std::{collections::HashSet, fs::{self, File}, io};
use tokio::{net::TcpStream, sync::Mutex};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc::UnboundedSender;
use serde::{Serialize, Deserialize};
use serde_json::{json, Value};
use futures::TryStreamExt;
use tracing::{info, warn};
use redis::AsyncCommands;
use deadpool_redis::Pool;
use std::path::Path;
use std::io::Write;
use std::sync::Arc;
use prost::Message;
use yansi::Paint;

use crate::{
    constants::{
        redis::TRACKED_USER_ADDRESSES, 
        zmq::{LAMPORTS_BALANCE_UPDATE, SPL_TOKEN_BALANCE_UPDATE}
    }, 
    instructions::raydium::initialize_two::initialize_two_handler, 
    transaction_helpers::compile_balance_updates::compile_balance_updates
};
use crate::instructions::serum::initialize_market::initialize_market_handler;
use crate::programs::daos_fund_deployer::DaosFundDeployerFunction;
use crate::instructions::{daos_fund, pump_fun};
use crate::programs::pump_fun::PumpFunFunction;
use crate::programs::raydium::RaydiumFunction;
use crate::programs::serum::SerumFunction;
use crate::messaging::MpscMessage;
use crate::programs::ProgramId;

pub mod spl_token {
    tonic::include_proto!("spl_token");
}

pub mod system {
    tonic::include_proto!("system");
}

use system::LamportsBalanceUpdate;
use spl_token::SplBalanceUpdate;

#[derive(Clone)]
pub struct TransactionProcessor {
    api_key: String,
    url: String,
    ws: Arc<Mutex<Option<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    tx: UnboundedSender<MpscMessage>,
    redis_pool: Arc<Pool>
}

/// https://github.com/helius-labs/helius-rust-sdk/blob/dev/src/types/enhanced_websocket.rs#L96
#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionNotificationResult {
    transaction: EncodedTransactionWithStatusMeta,
    signature: String,
    slot: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionNotificationParams {
    subscription: u64,
    result: TransactionNotificationResult
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionNotification {
    jsonrpc: String,
    method: String,
    params: TransactionNotificationParams
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SubscriptionSuccessfulNotification {
    id: String,
    jsonrpc: String,
    result: u64
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
pub struct SubscriptionErrorParams {
    subscription: u64,
    error: String
} 

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SubscriptionErrorNotification {
    jsonrpc: String,
    method: String,
    params: SubscriptionErrorParams
}

impl TransactionProcessor {
    pub async fn new(api_key: &str, url: &str, tx: UnboundedSender<MpscMessage>, redis_pool: Arc<Pool>) -> Result<Self, WsError> {
        Ok(Self {
            api_key: api_key.to_string(),
            url: url.to_string(),
            ws: Arc::new(Mutex::new(None)),
            tx,
            redis_pool
        })
    }

    /// Runs the three step process to start the transaction processor.
    pub async fn start_processor(&self) {
        loop {
            if let Err(err) = self.start_connection().await {
                warn!("Failed to establish connection to WebSocket: {}. Retrying in 1 second(s)...", err);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }

            if let Err(err) = self.subscribe_to_transactions().await {
                warn!("Failed to subscribe to transactions: {}. Retrying in 1 second(s)...", err);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }

            match self.process_messages().await {
                Ok(_) => {
                    info!("Message processing ended gracefully (unexpected). Stopping...");
                    break;
                }
                Err(err) => {
                    warn!("Error while processing messages: {}. Will retry in 1 second(s)...", err);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    pub async fn start_connection(&self) -> Result<(), WsError> {
        let ws_url = format!("wss://{}/?api-key={}", self.url, self.api_key);
        info!("Connecting to: wss://{}/?api-key=*", self.url);

        let (ws, _response) = connect_async(ws_url).await?;
        info!("WebSocket connection established!");

        *self.ws.lock().await = Some(ws);

        Ok(())
    }
    
    pub async fn subscribe_to_transactions(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "transactionSubscribe",
            "params": [
                {
                    "vote": false,
                    "failed": false,
                    "accountInclude": [],
                    "accountRequired": [],
                    "accountExclude": [],
                },
                {
                    "commitment": "processed",
                    "encoding": "jsonParsed",
                    "transaction_details": "full",
                    "showRewards": true,
                    "maxSupportedTransactionVersion": 0,
                }
            ]
        });

        let request_string = request.to_string();

        info!("Unlocking WebSocket mutex...");

        let mut ws_guard = self.ws.lock().await;

        info!("WebSocket mutex unlocked.");

        if let Some(ws) = ws_guard.as_mut() {
            ws.send(WsMessage::Text(request_string)).await?;
            info!("Subscription to transactions successfully sent.");
            Ok(())
        } else {
            Err("WebSocket is not connected!".into())
        }
    }

    pub async fn process_messages(&self) -> Result<(), WsError> {
        let mut ws_guard = self.ws.lock().await;
        let ws = ws_guard.take().ok_or_else(|| {
            WsError::Io(io::Error::new(io::ErrorKind::Other, "WebSocket is not connected!"))
        })?;

        let (write, read) = ws.split();

        let write_arc = Arc::new(Mutex::new(write));

        let write_clone = write_arc.clone();
        let heartbeat = tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                if let Err(err) = write_clone.lock().await.send(WsMessage::Ping(vec![])).await {
                    eprintln!("Failed to send ping: {:?}", err);
                    break;
                }
            }
        });

        // todo: see below
        // ! It's possible that this could lead to issues in the event that a later message
        // ! is processed first compared to an earlier message. If the later message contains
        // ! an account that is also updated in the former message, and the former message
        // ! is processed after, then you'd technically have stale data.
        let result = read.try_for_each_concurrent(16, |msg| {
            let this = self.clone();
            async move { this.handle_message(msg).await }
        }).await;

        heartbeat.abort();

        match result {
            Ok(()) => {
                // If we reach here, it means the stream ended gracefully (connection closed).
                warn!("WebSocket connection closed.");
                Ok(())
            }
            Err(e) => {
                // If we encounter an error (like a subscription error that couldn't be fixed 
                // or a read error), this will trigger the logic in `start_processor` to 
                // reconnect and/or resubscribe.
                warn!("Error processing messages: {}", e);
                Err(e)
            }
        }
    }

    async fn handle_message(&self, message: WsMessage) -> Result<(), WsError> {
        match message {
            WsMessage::Text(text) => {
                let json: Value = serde_json::from_str(&text)
                    .map_err(|e| WsError::Io(io::Error::new(io::ErrorKind::Other, format!("JSON parse error: {}", e))))?;
    
                if let Ok(notification) = serde_json::from_value::<TransactionNotification>(json.clone()) {
                    let _ = self.handle_transaction_notification(notification).await;
                    return Ok(());
                }
    
                if let Ok(notification) = serde_json::from_value::<SubscriptionSuccessfulNotification>(json.clone()) {
                    info!("Subscription successful: {:?}", notification);
                    return Ok(());
                }

                if let Ok(notification) = serde_json::from_value::<SubscriptionErrorNotification>(json.clone()) {
                    warn!("Subscription error: {:?}", notification);
                    warn!("Sending re-subscription request...");
    
                    self.subscribe_to_transactions().await.map_err(|e| {
                        WsError::Io(io::Error::new(io::ErrorKind::Other, format!("Failed to re-subscribe after error: {}", e)))
                    })?;
    
                    info!("Re-subscription successful after error.");
                    return Ok(());
                }

                warn!("Unknown JSON message: {}", text);
                Ok(())
            }
            WsMessage::Pong(_data) => {
                info!("WebSocket server responded with Pong.");
                Ok(())
            }
            other => {
                println!("Unknown message: {:?}", other);
                Ok(())
            }
        }
    }

    async fn handle_transaction_notification(&self, notification: TransactionNotification) -> Result<(), WsError> {
        let meta = match &notification.params.result.transaction.meta {
            Some(meta) => meta,
            None => {
                panic!("Transaction meta is missing!");
            }
        };

        let transaction = match &notification.params.result.transaction.transaction {
            EncodedTransaction::Json(ui_transaction) => ui_transaction,
            _ => {
                panic!("Binary transactions are not supported!");
            }
        };

        let message = match &transaction.message {
            UiMessage::Parsed(ui_message) => ui_message,
            _ => {
                panic!("Binary messages are not supported!");
            }
        };

        let accounts = &message.account_keys;

        let mut compiled_instructions: Vec<UiCompiledInstruction> = Vec::new();
        let mut parsed_instructions: Vec<UiParsedInstruction> = Vec::new();

        for instruction in message.instructions.iter() {
            match instruction {
                UiInstruction::Parsed(ui_instruction) => parsed_instructions.push(ui_instruction.clone()),
                UiInstruction::Compiled(ui_instruction) => compiled_instructions.push(ui_instruction.clone())
            }
        }

        if let OptionSerializer::Some(inner_instructions) = &meta.inner_instructions {
            for ui_inner in inner_instructions {
                for inner_instruction in ui_inner.instructions.iter() {
                    match inner_instruction {
                        UiInstruction::Parsed(ui_instruction) => parsed_instructions.push(ui_instruction.clone()),
                        UiInstruction::Compiled(ui_instruction) => compiled_instructions.push(ui_instruction.clone())
                    }
                }
            }
        }

        for instruction in parsed_instructions.iter() {
            match instruction {
                UiParsedInstruction::Parsed(_ui_instruction) => {},
                UiParsedInstruction::PartiallyDecoded(ui_instruction) => {
                    let program_address = &ui_instruction.program_id;

                    if let Some(program_id) = ProgramId::from_str(&program_address) {
                        match program_id {
                            ProgramId::PumpFun => {
                                if let Some(instruction_type) = PumpFunFunction::from_data(&ui_instruction.data) {
                                    match instruction_type {
                                        PumpFunFunction::Creation => pump_fun::creation::creation_handler(
                                            notification.params.result.slot,
                                            &ui_instruction,
                                            accounts,
                                            &meta,
                                            &notification.params.result.signature,
                                            self.tx.clone()
                                        ),
                                        PumpFunFunction::Buy => pump_fun::trade::trade_handler(
                                            &ui_instruction,
                                            accounts,
                                            &meta,
                                            self.redis_pool.clone(),
                                            self.tx.clone()
                                        ).await?,
                                        PumpFunFunction::Sell => pump_fun::trade::trade_handler(
                                            &ui_instruction,
                                            accounts,
                                            &meta,
                                            self.redis_pool.clone(),
                                            self.tx.clone()
                                        ).await?,
                                    }
                                }
                            },
                            ProgramId::DaosFundDeployer => {
                                if let Some(instruction_type) = DaosFundDeployerFunction::from_data(&ui_instruction.data) {
                                    match instruction_type {
                                        DaosFundDeployerFunction::InitializeCurve => {
                                            daos_fund::initialize_curve::initialize_curve_handler(
                                                notification.params.result.slot, 
                                                &ui_instruction, 
                                                accounts, 
                                                &meta, 
                                                &notification.params.result.signature, 
                                                self.tx.clone()
                                            );
                                            info!("Daos Fund Deployer InitializeCurve");
                                            let dir_path = format!("./unknown-txs/{}", program_address);
                                            fs::create_dir_all(&dir_path).expect("Failed to create directories");

                                            let json = serde_json::to_string_pretty(&notification).expect("Failed to serialize notification");

                                            let file_path = format!("{}/{}.json", dir_path, "initializeCurve");

                                            if !Path::new(&file_path).exists() {
                                                let mut file = File::create(&file_path).expect("Failed to create file");
                                                file.write_all(json.as_bytes()).expect("Failed to write to file");
                                            }
                                        }
                                    }
                                }
                            },
                            ProgramId::Serum => {
                                if let Some(instruction_type) = SerumFunction::from_data(&ui_instruction.data) {
                                    match instruction_type {
                                        SerumFunction::InitializeMarket => initialize_market_handler(
                                            &ui_instruction, 
                                            accounts, 
                                            self.tx.clone(),
                                            &notification.params.result.signature
                                        ),
                                    }
                                }
                            },
                            ProgramId::Raydium => {
                                if let Some(instruction_type) = RaydiumFunction::from_data(&ui_instruction.data) {
                                    match instruction_type {
                                        RaydiumFunction::Initialize => info!("Raydium Initialize"),
                                        RaydiumFunction::Initialize2 => initialize_two_handler(
                                            &ui_instruction,
                                            accounts,
                                            &notification.params.result.signature
                                        )
                                    }
                                }
                            }
                        }
                    }
                },
            };
        }

        let mut conn = self.redis_pool.get().await.map_err(|e| {
            WsError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("Redis pool error: {}", e)))
        })?;

        let tracked_addresses: HashSet<String> = conn.smembers(TRACKED_USER_ADDRESSES).await.map_err(|e| {
            WsError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("Redis SMEMBERS error: {}", e)))
        })?;

        let pre_balances = &meta.pre_balances;
        let post_balances = &meta.post_balances;
        for (i, account) in accounts.iter().enumerate() {
            let pre_balance = *pre_balances.get(i).unwrap_or(&0);
            let post_balance = *post_balances.get(i).unwrap_or(&0);
            
            if pre_balance != post_balance && tracked_addresses.contains(&account.pubkey) {
                let mpsc_message = LamportsBalanceUpdate {
                    address: account.pubkey.to_string(),
                    pre_balance,
                    post_balance,
                };

                self.tx.send(MpscMessage {
                    topic: LAMPORTS_BALANCE_UPDATE.to_string(),
                    payload: mpsc_message.encode_to_vec(),
                }).expect("Failed to send MPSC Message.");

                info!(
                    "Sending {} for {}", 
                    Paint::magenta("BALANCE_UPDATE"), 
                    Paint::cyan(account.pubkey.to_string())
                );
            }
        }

        let pre_token_balances = meta.pre_token_balances.clone().unwrap();
        let post_token_balances = meta.post_token_balances.clone().unwrap();
        let token_balance_updates = compile_balance_updates(&pre_token_balances, &post_token_balances);
        for (key, update) in token_balance_updates {
            // By definition of the transaction pre- and post- token balances,
            // the pre-balance and post-balances will always be different. This means
            // we do not need to perform an additional check like we do for the SOL amounts.
            if tracked_addresses.contains(&key.owner) {
                let mpsc_message = SplBalanceUpdate {
                    address: key.owner.to_string(),
                    mint: key.mint,
                    pre_balance: update.pre,
                    post_balance: update.post,
                    decimals: update.decimals as u32,
                };

                self.tx.send(MpscMessage {
                    topic: SPL_TOKEN_BALANCE_UPDATE.to_string(),
                    payload: mpsc_message.encode_to_vec(),
                }).expect("Failed to send MPSC Message.");

                info!(
                    "Sending {} for {}",
                    Paint::magenta("SPL_BALANCE_UPDATE"),
                    Paint::cyan(key.owner)
                );
            }
        }

        Ok(())
    }
}