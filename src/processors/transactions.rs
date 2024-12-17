use solana_transaction_status::{EncodedTransaction, EncodedTransactionWithStatusMeta, UiCompiledInstruction, UiInstruction, UiMessage, UiParsedInstruction};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use solana_transaction_status::option_serializer::OptionSerializer;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio::{net::TcpStream, sync::Mutex};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc::UnboundedSender;
use serde::{Serialize, Deserialize};
use serde_json::{json, Value};
use tracing::{info, warn};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use crate::instructions::pump_fun;
use crate::messaging::MpscMessage;
use crate::programs::pump_fun::PumpFunFunction;
use crate::programs::raydium::RaydiumFunction;
use crate::programs::serum::SerumFunction;
use crate::programs::ProgramId;

#[derive(Clone)]
pub struct TransactionProcessor {
    api_key: String,
    url: String,
    ws: Arc<Mutex<Option<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    tx: UnboundedSender<MpscMessage>,
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

// todo: if we get the below we need to resubscribe
// Unknown JSON message: 
// {
//     "jsonrpc":"2.0",
//     "method":"transactionSubscribe",
//     "params":{
//         "subscription":879524311758297,
//         "error":"Status { code: Internal, message: \"h2 protocol error: error reading a body from connection\", source: Some(hyper::Error(Body, Error { kind: Reset(StreamId(1), INTERNAL_ERROR, Remote) })) }"
//     }
// }

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
    pub async fn new(api_key: &str, url: &str, tx: UnboundedSender<MpscMessage>) -> Result<Self, WsError> {
        Ok(Self {
            api_key: api_key.to_string(),
            url: url.to_string(),
            ws: Arc::new(Mutex::new(None)),
            tx
        })
    }

    pub async fn start_processor(&self) {
        let mut retry_attempts = 0;

        loop {
            if retry_attempts > 0 {
                let delay = std::time::Duration::from_secs(2u64.pow((retry_attempts - 1).min(5)));
                warn!(
                    "Reconnecting WebSocket after {} seconds... (attempt #{})",
                    delay.as_secs(),
                    retry_attempts
                );
                tokio::time::sleep(delay).await;
            }

            match self.start_connection().await {
                Ok(_) => {
                    info!("WebSocket connected successfully!");
                    retry_attempts = 0;
                    self.subscribe_and_process().await;
                }
                Err(err) => {
                    warn!("Failed to connect WebSocket: {}. Retrying...", err);
                    retry_attempts += 1;
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
    
    pub async fn subscribe_and_process(&self) {
        self.subscribe_to_transactions().await;

        let mut ws_guard = self.ws.lock().await;

        if let Some(ws) = ws_guard.take() {
            let (write, mut read_stream) = ws.split();
            let write_arc = Arc::new(Mutex::new(write));

            let write_clone = write_arc.clone();
            let heartbeat = tokio::spawn(async move {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                    if let Err(err) = write_clone.lock().await.send(Message::Ping(vec![])).await {
                        eprintln!("Failed to send ping: {:?}", err);
                        break;
                    }
                }
            });
            
            while let Some(result) = read_stream.next().await {
                match result {
                    Ok(msg) => {    
                        let processor_clone = self.clone();
                        tokio::spawn(async move {
                            processor_clone.handle_message(msg);
                        });
                    }
                    Err(err) => {
                        warn!("Failed to read message: {}", err);
                        break;
                    }
                }
            }

            heartbeat.abort();
            *ws_guard = None;

            warn!("WebSocket connection was closed peacefully.")
        } else {
            println!("WebSocket is not connected!");
        }
    }

    pub async fn subscribe_to_transactions(&self) {
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

        let mut ws_guard = self.ws.lock().await;

        if let Some(ws) = ws_guard.as_mut() {
            if let Err(err) = ws.send(Message::Text(request_string)).await {
                eprintln!("Failed to send subscription request: {}", err);
            } else {
                println!("Subscription request sent successfully.");
            }
        } else {
            eprintln!("WebSocket is not connected!");
        }
    }

    pub fn handle_message(&self, message: Message) {
        match message {
            Message::Text(text) => {
                let json: Value = serde_json::from_str(&text).unwrap();
    
                if let Ok(notification) = serde_json::from_value::<TransactionNotification>(json.clone()) {
                    self.handle_transaction_notification(notification);
                    return;
                }
    
                if let Ok(notification) = serde_json::from_value::<SubscriptionSuccessfulNotification>(json.clone()) {
                    info!("Subscription successful: {:?}", notification);
                    return;
                }

                if let Ok(notification) = serde_json::from_value::<SubscriptionErrorNotification>(json.clone()) {
                    warn!("Subscription error: {:?}", notification);
                    
                    let tx_clone = self.clone();
                    tokio::spawn(async move {
                        tx_clone.subscribe_to_transactions().await;
                    });

                    return;
                }

                println!("Unknown JSON message: {}", text);
            }
            Message::Pong(_data) => info!("WebSocket server responded with Pong."),
            other => println!("Unknown message: {:?}", other)
        }
    }

    fn handle_transaction_notification(&self, notification: TransactionNotification) {
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

        // todo: need to implement something on a per `program_id` basis where if the instruction type is not known we write the instruction to a file
        for instruction in parsed_instructions.iter() {
            match instruction {
                UiParsedInstruction::Parsed(_ui_instruction) => {
                    // use std::fs::{self, File};
                    // use std::path::Path;
                    // use std::io::Write;

                    // let program_address = &ui_instruction.program_id;

                    // if let Value::Object(parsed) = &ui_instruction.parsed {
                    //     let instruction_type = parsed
                    //         .get("type")
                    //         .and_then(|t| t.as_str())
                    //         .expect("Parsed instruction does not contain a `type` field or it is not a string!");
        
                    //     let json = serde_json::to_string_pretty(&ui_instruction).expect("Failed to serialize notification");
        
                    //     let dir_path = format!("./unknown-txs/{}", program_address);
                    //     fs::create_dir_all(&dir_path).expect("Failed to create directories");
        
                    //     let file_path = format!("{}/{}.json", dir_path, instruction_type);
        
                    //     if !Path::new(&file_path).exists() {
                    //         let mut file = File::create(&file_path).expect("Failed to create file");
                    //         file.write_all(json.as_bytes()).expect("Failed to write to file");
                    //     }
                    // } else {
                    //     continue;
                    // }
                },
                UiParsedInstruction::PartiallyDecoded(ui_instruction) => {
                    let program_address = &ui_instruction.program_id;

                    if let Some(program_id) = ProgramId::from_str(&program_address) {
                        match program_id {
                            // https://github.com/project-serum/serum-ts/blob/master/packages/serum/src/instructions.js#L33
                            // https://github.com/project-serum/serum-ts/blob/master/packages/serum/src/instructions.js#L195
                            ProgramId::Serum => {
                                if let Some(instruction_type) = SerumFunction::from_data(&ui_instruction.data) {
                                    match instruction_type {
                                        SerumFunction::InitializeMarket => {
                                            info!("Serum Initialize Market");
                                        }
                                    }
                                }
                            },
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
                                        PumpFunFunction::Buy => pump_fun::buy::buy_handler(),
                                        PumpFunFunction::Sell => pump_fun::sell::sell_handler(),
                                    }
                                }
                            },
                            ProgramId::Raydium => {
                                if let Some(instruction_type) = RaydiumFunction::from_data(&ui_instruction.data) {
                                    match instruction_type {
                                        RaydiumFunction::Initialize => {
                                            info!("Raydium Initialize");
                                        },
                                        RaydiumFunction::Initialize2 => {
                                            info!("Raydium Initialize2");
                                            let dir_path = format!("./unknown-txs/{}", program_address);
                                            fs::create_dir_all(&dir_path).expect("Failed to create directories");

                                            let json = serde_json::to_string_pretty(&notification).expect("Failed to serialize notification");

                                            let file_path = format!("{}/{}.json", dir_path, "initialize2");

                                            if !Path::new(&file_path).exists() {
                                                let mut file = File::create(&file_path).expect("Failed to create file");
                                                file.write_all(json.as_bytes()).expect("Failed to write to file");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            };
        }

        // todo: iterate through accounts and send an account update if the sol balance updates
        // Likely fastest way to do this:
        // 1. Create a map between pre and post sol balances
        // 2. If the balance changes, then we check if the address is a tracked address
        //    Option A. first get the redis set of tracked addresses (think this is faster)
        //    Option B. simply check if the address exists in the set 
        // 3. If the address is tracked, then we send a balance update message
        // 4. If the address is not tracked, then we continue

        // ! FOR DEBUGGING
        // let json = serde_json::to_string_pretty(&notification).expect("Failed to serialize notification");
        // let mut file = File::create(format!("./tx-files/transaction-{}.json", &notification.params.result.signature)).expect("Failed to create file");
        // file.write_all(json.as_bytes()).expect("Failed to write to file");
    }
}