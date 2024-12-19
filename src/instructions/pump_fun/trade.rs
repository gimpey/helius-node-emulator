use deadpool_redis::Pool;
use prost::Message;
use redis::AsyncCommands;
use solana_transaction_status::{parse_accounts::ParsedAccount, UiPartiallyDecodedInstruction, UiTransactionStatusMeta};
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;
use std::{io, sync::Arc};
use yansi::Paint;

use crate::{messaging::MpscMessage, transaction_helpers::find_token_balance_by_address::find_token_balance_by_address};

pub mod spl_token {
    tonic::include_proto!("spl_token");
}

use spl_token::PumpFunTradeUpdate;

const INITIAL_LAMPORT_RESERVES: u64 = 30_000_000_000;
const INITIAL_VIRTUAL_TOKEN_RESERVES: u64 = 1_073_000_000_000_000;
const INITIAL_REAL_TOKEN_RESERVES: u64 = 793_100_000_000_000;
const INITIAL_TOKEN_TOTAL_SUPPLY: u64 = 1_000_000_000_000_000;

const TRACKED_TOKENS_KEY: &str = "tracked_token_addresses";

pub async fn trade_handler(
    instruction: &UiPartiallyDecodedInstruction,
    accounts: &Vec<ParsedAccount>,
    meta: &UiTransactionStatusMeta,
    redis_pool: Arc<Pool>,
    tx: UnboundedSender<MpscMessage>
) -> Result<(), WsError> {
    let token_address = instruction.accounts.get(2).unwrap().to_string();

    let mut conn = redis_pool.get().await.map_err(|e| {
        WsError::Io(io::Error::new(io::ErrorKind::Other, format!("Failed to get Redis connection: {:?}", e)));
    }).expect("Failed to get Redis connection.");

    let is_tracked: bool = conn.sismember(TRACKED_TOKENS_KEY, &token_address).await.map_err(|e| {
        WsError::Io(io::Error::new(io::ErrorKind::Other, format!("Failed to check if token is tracked: {:?}", e)));
    }).unwrap_or(false);

    if !is_tracked {
        return Ok(());
    }

    let bonding_curve = instruction.accounts.get(3).unwrap();
    let bonding_curve_index = accounts.iter().position(|x| x.pubkey == *bonding_curve).unwrap();

    let post_token_balances = meta.post_token_balances.clone().unwrap();

    let real_lamport_reserves = meta.post_balances.get(bonding_curve_index).unwrap();
    let bonding_curve_token_balance = match find_token_balance_by_address(&post_token_balances, bonding_curve) {
        Some(token_balance) => token_balance.ui_token_amount.amount.parse::<u64>().unwrap_or(0),
        None => 0
    };

    let virtual_lamport_reserves = real_lamport_reserves + INITIAL_LAMPORT_RESERVES;
    let virtual_token_reserves = bonding_curve_token_balance + (INITIAL_VIRTUAL_TOKEN_RESERVES - INITIAL_TOKEN_TOTAL_SUPPLY);
    let real_token_reserves = bonding_curve_token_balance - (INITIAL_TOKEN_TOTAL_SUPPLY - INITIAL_REAL_TOKEN_RESERVES);

    let message = PumpFunTradeUpdate {
        token_address: token_address.clone(),
        bonding_curve: bonding_curve.to_string(),
        real_lamport_reserves: *real_lamport_reserves,
        real_token_reserves,
        virtual_lamport_reserves,
        virtual_token_reserves
    };

    tx.send(MpscMessage {
        topic: "pump_fun_bonding_curve_update".to_string(),
        payload: message.encode_to_vec()
    }).expect("Failed to send MPSC Message.");

    info!(
        "Sending {} for {} {}",
        Paint::magenta("BONDING_CURVE_UPDATE"),
        Paint::cyan("PUMP_FUN_PROGRAM"),
        Paint::black(token_address)
    );

    Ok(())
}