use solana_transaction_status::{parse_accounts::ParsedAccount, UiPartiallyDecodedInstruction};
use tracing::info;
use yansi::Paint;

use crate::constants::addresses::PUMP_FUN_RAYDIUM_MIGRATION;

pub fn initialize_two_handler(
    instruction: &UiPartiallyDecodedInstruction,
    accounts: &Vec<ParsedAccount>,
    _signature: &String
) {
    let is_pump_fun = accounts.iter()
        .find(|account| account.pubkey == PUMP_FUN_RAYDIUM_MIGRATION)
        .map(|account| account.signer)
        .unwrap_or(false);

    let is_official: &str = if is_pump_fun { "OFFICIAL PUMP.FUN" } else { "" };

    // instruction.accounts.get(9) is the mint
    // instruction.accounts.get(16) is the market id
    // with the market id you can get the serum information, either through cache/db/on-chain data
    let token_address = instruction.accounts.get(9).unwrap();
    let market_id_address = instruction.accounts.get(16).unwrap();

    info!(
        "Processing {} instruction for {} {} {}", 
        Paint::magenta("INITIALIZE_2"), 
        Paint::cyan("RAYDIUM_PROGRAM"),
        Paint::black(token_address.to_string()),
        Paint::red(is_official)
    );
    info!("{} Market ID: {}", 
        Paint::red(">"),
        Paint::black(market_id_address.to_string())
    );
    info!("{}", Paint::red(_signature));
}