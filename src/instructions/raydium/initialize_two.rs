use solana_transaction_status::UiPartiallyDecodedInstruction;
use tracing::info;
use yansi::Paint;

pub fn initialize_two_handler(
    instruction: &UiPartiallyDecodedInstruction,
    _signature: &String
) {
    // instruction.accounts.get(9) is the mint
    // instruction.accounts.get(16) is the market id
    // with the market id you can get the serum information, either through cache/db/on-chain data
    let token_address = instruction.accounts.get(7).unwrap();

    info!(
        "Processing {} instruction for {} {}", 
        Paint::magenta("INITIALIZE_2"), 
        Paint::cyan("RAYDIUM_PROGRAM"),
        Paint::black(token_address.to_string())
    );
    info!("{}", Paint::red(_signature));
}