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
use std::sync::Arc;

use crate::instructions::pump_fun;
use crate::messaging::MpscMessage;
use crate::programs::pump_fun::PumpFunFunction;
use crate::programs::ProgramId;

#[derive(Clone)]
pub struct TransactionProcessor {
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

impl TransactionProcessor {
    pub async fn new(api_key: &str, url: &str, tx: UnboundedSender<MpscMessage>) -> Result<Self, WsError> {
        let ws_url = format!("wss://{}/?api-key={}", url, api_key);
        info!("Connecting to: wss://{}/?api-key=*", url);

        let (ws, _response) = connect_async(ws_url).await?;
        info!("WebSocket connection established!");

        Ok(Self {
            ws: Arc::new(Mutex::new(Some(ws))),
            tx
        })
    }
    
    pub async fn start_processor(&self) {
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
                                        PumpFunFunction::Buy => pump_fun::buy::buy_handler(),
                                        PumpFunFunction::Sell => pump_fun::sell::sell_handler(),
                                    }
                                }
                            }
                        }
                    }
                },
            };
        }

        // todo: I think all of the transactions are parsed or partially decoded
        // for instruction in compiled_instructions.iter() {
        //     let program_id_index = instruction.program_id_index as usize;
        //     let program_address = &accounts.get(program_id_index).unwrap().pubkey;

        //     if let Some(program_id) = ProgramId::from_str(&program_address) {
        //         match program_id {
        //             ProgramId::PumpFun => {
        //                 if let Some(instruction_type) = PumpFunFunction::from_data(&instruction.data) {
        //                     match instruction_type {
        //                         PumpFunFunction::Creation => pump_fun::creation::creation_handler(),
        //                         PumpFunFunction::Buy => pump_fun::buy::buy_handler(),
        //                         PumpFunFunction::Sell => pump_fun::sell::sell_handler(),
        //                     }
        //                 }
        //             }
        //         }
        //     }
        // }

        // todo: compile instructions
        // todo: compile inner instructions
        // todo: iterate through
        // todo: derive program id
        // todo: derive instruction discriminator

        // ! FOR DEBUGGING
        // use std::io::Write;
        // use std::fs::File;
        // let json = serde_json::to_string_pretty(&notification).expect("Failed to serialize notification");
        // let mut file = File::create(format!("./tx-files/transaction-{}.json", &notification.params.result.signature)).expect("Failed to create file");
        // file.write_all(json.as_bytes()).expect("Failed to write to file");
    }
}