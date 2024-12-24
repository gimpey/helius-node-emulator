use std::time::Instant;

/// # Serum Market ID Creation (Initialize Market)
/// 
/// REQUIRES REDIS: FALSE
/// REQUIRES ZMQ: TRUE
///
/// Data Deserialization: https://github.com/project-serum/serum-ts/blob/master/packages/serum/src/instructions.js#L33
/// 
/// Accounts: https://github.com/project-serum/serum-ts/blob/master/packages/serum/src/instructions.js#L195
/// 
/// https://github.com/project-serum/serum-dex/blob/master/dex/src/instruction.rs#L341

use gimpey_db_gateway::{generated::serum_market::CreateSerumMarketRequest, SerumMarketClient};
use solana_transaction_status::{parse_accounts::ParsedAccount, UiPartiallyDecodedInstruction};
use borsh::{BorshDeserialize, BorshSerialize};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};
use yansi::Paint;

use crate::{
    constants::addresses::{
        PUMP_FUN_RAYDIUM_MIGRATION, 
        WSOL_ADDRESS
    }, 
    messaging::MpscMessage,
};

pub mod spl_token {
    tonic::include_proto!("spl_token");
}

#[derive(BorshDeserialize, BorshSerialize, Debug)]
struct InitializeMarketData {
    version: u8,
    discriminator: u32,
    base_lot_size: u64,
    quote_lot_size: u64,
    fee_rate_bps: u16,
    vault_signer_nonce: u64,
    quote_dust_threshold: u64,
}

impl InitializeMarketData {
    pub fn from_base58_data(data: &String) -> Self {
        let decoded_data = bs58::decode(data).into_vec().unwrap();
        Self::try_from_slice(&decoded_data).unwrap()
    }
}


pub async fn initialize_market_handler(
    instruction: &UiPartiallyDecodedInstruction,
    accounts: &Vec<ParsedAccount>,
    _tx: UnboundedSender<MpscMessage>,
    _signature: &String,
    serum_market_client: SerumMarketClient
) {
    let start = Instant::now();

    let is_pump_fun = accounts.iter()
        .find(|account| account.pubkey == PUMP_FUN_RAYDIUM_MIGRATION)
        .map(|account| account.signer)
        .unwrap_or(false);

    let is_official: &str = if is_pump_fun { "OFFICIAL PUMP.FUN" } else { "" };

    if is_pump_fun {
        let data = InitializeMarketData::from_base58_data(&instruction.data);

        let market_id_address = instruction.accounts.get(0).unwrap();
        let request_queue_address = instruction.accounts.get(1).unwrap();
        let event_queue_address = instruction.accounts.get(2).unwrap();
        let bids_address = instruction.accounts.get(3).unwrap();
        let asks_address = instruction.accounts.get(4).unwrap();
        let base_spl_token_account_address = instruction.accounts.get(5).unwrap();
        let quote_spl_token_account_address = instruction.accounts.get(6).unwrap();
        let base_token_address = instruction.accounts.get(7).unwrap();
        let quote_token_address = instruction.accounts.get(8).unwrap();
    
        let base = if base_token_address == WSOL_ADDRESS { "WSOL" } else { base_token_address };
        let quote = if quote_token_address == WSOL_ADDRESS { "WSOL" } else { quote_token_address };

        let request = CreateSerumMarketRequest {
            market_id: market_id_address.to_string(),
            request_queue_address: request_queue_address.to_string(),
            event_queue_address: event_queue_address.to_string(),
            bids_address: bids_address.to_string(),
            asks_address: asks_address.to_string(),
            base_spl_token_account_address: base_spl_token_account_address.to_string(),
            quote_spl_token_account_address: quote_spl_token_account_address.to_string(),
            base_token_address: base_token_address.to_string(),
            quote_token_address: quote_token_address.to_string(),
            base_lot_size: data.base_lot_size,
            quote_lot_size: data.quote_lot_size,
            fee_rate_bps: data.fee_rate_bps as u32,
            vault_signer_nonce: data.vault_signer_nonce,
            quote_dust_threshold: data.quote_dust_threshold
        };

        let response = serum_market_client
            .create_serum_market(request)
            .await
            .map_err(|_| warn!("Failed to create Serum Market."));

        if let Err(error) = response {
            warn!("Failed to create Serum Market: {:?}", error);
        } else {
            info!(
                "Processing {} instruction for {} {}/{} {}", 
                Paint::magenta("INITIALIZE_MARKET"),
                Paint::cyan("SERUM_PROGRAM"),
                Paint::black(base),
                Paint::black(quote),
                Paint::red(is_official)
            );
            info!(
                "{} Market ID: {}", 
                Paint::red(">"), 
                Paint::black(market_id_address)
            );
            info!(
                "{} Time taken: {}ms",
                Paint::red(">"),
                Paint::black(start.elapsed().as_millis())
            )
        }
    }
}