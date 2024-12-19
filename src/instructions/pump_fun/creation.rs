/// # Pump Fun Creation Detection
/// 
/// REQUIRES REDIS: FALSE
/// REQUIRES ZMQ: TRUE

use solana_transaction_status::{option_serializer::OptionSerializer, parse_accounts::ParsedAccount, UiPartiallyDecodedInstruction, UiTransactionStatusMeta};
use borsh::{BorshDeserialize, BorshSerialize};
use tokio::sync::mpsc::UnboundedSender;
use serde::{Deserialize, Serialize};
use prost::Message;
use tracing::info;
use yansi::Paint;
use chrono::Utc;

use crate::{constants::zmq::SPL_TOKEN_CREATION_UPDATE, messaging::MpscMessage, transaction_helpers::find_token_balance_by_address::find_token_balance_by_address};

pub mod spl_token {
    tonic::include_proto!("spl_token");
}

use spl_token::SplTokenCreationNotification;

#[derive(Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize)]
pub struct CreateDataSchema {
    discriminator: u64,
    name: String,
    symbol: String,
    uri: String,
}

#[derive(Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize, Clone)]
struct BaseMetadata {
    name: String,
    symbol: String,
    uri: String,
}

pub fn creation_handler(
    _slot: u64,
    instruction: &UiPartiallyDecodedInstruction,
    _accounts: &Vec<ParsedAccount>,
    meta: &UiTransactionStatusMeta,
    signature: &String,
    tx: UnboundedSender<MpscMessage>
) {
    let bytes = bs58::decode(&instruction.data)
        .into_vec()
        .expect("Failed to decode instruction data.");

    let metadata_result = CreateDataSchema::try_from_slice(&bytes)
        .ok()
        .map(|decoded_data| BaseMetadata {
            name: decoded_data.name,
            symbol: decoded_data.symbol,
            uri: decoded_data.uri,
        });

    let metadata = match metadata_result {
        Some(metadata) => metadata,
        None => {
            panic!("Failed to decode metadata. Instruction data: {:?}", &instruction.data);
        }
    };

    let token_account = instruction.accounts.get(0).unwrap();
    let bonding_curve = instruction.accounts.get(2).unwrap();
    let associated_bonding_curve = instruction.accounts.get(3).unwrap();
    let deployer = instruction.accounts.get(7).unwrap();

    let post_token_balances = match &meta.post_token_balances {
        OptionSerializer::Some(balances) => balances,
        _ => {
            panic!("Failed to get post token balances.");
        }
    };

    let owner_balance = match find_token_balance_by_address(post_token_balances, &deployer) {
        Some(balance) => balance.ui_token_amount.ui_amount.unwrap_or(0.0),
        None => 0.0
    };

    let creator_percentage: f64 = owner_balance / 1e9;

    let message = SplTokenCreationNotification {
        deployer: deployer.to_string(),
        token_address: token_account.to_string(),
        bonding_curve: bonding_curve.to_string(),
        associated_bonding_curve: associated_bonding_curve.to_string(),
        token_name: metadata.name,
        token_symbol: metadata.symbol.clone(),
        token_uri: metadata.uri,
        creator_buy_percentage: creator_percentage,
        timestamp: Utc::now().timestamp_millis(),
        tx_hash: signature.to_string(),
        source: "HELIUS".to_string(),
        platform: "PUMP_FUN".to_string(),
    };

    tx.send(MpscMessage {
        topic: SPL_TOKEN_CREATION_UPDATE.to_string(),
        payload: message.encode_to_vec(),
    }).expect("Failed to send MPSC Message.");

    info!(
        "Processing {} instruction for {} {} {}", 
        Paint::magenta("CREATION"), 
        Paint::cyan("PUMP_FUN_PROGRAM"), 
        Paint::black(token_account),
        Paint::black(metadata.symbol)
    );
}