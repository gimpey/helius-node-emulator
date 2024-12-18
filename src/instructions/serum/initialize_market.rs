/// Data Deserialization: https://github.com/project-serum/serum-ts/blob/master/packages/serum/src/instructions.js#L33
/// 
/// Accounts: https://github.com/project-serum/serum-ts/blob/master/packages/serum/src/instructions.js#L195

use solana_transaction_status::{parse_accounts::ParsedAccount, UiPartiallyDecodedInstruction};
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;
use yansi::Paint;

use crate::messaging::MpscMessage;

// Need to figure out the difference between these two:
// 
// https://solscan.io/tx/2z7wMLr6hCZnwi3yL7PVfNwazxBmkKAXb3mGrsivsEN4xdrYanWsZj25pjS34FDmj3D2o8ceADC9c9pgARmA5tH7
// https://solscan.io/tx/njmt4jVBBRsW1ERZ86QTnX2YoRGcCM3LwZjxXpQfU6C9RgdteNy8PoGCCasERHs5nKNoUkpG3UTREgjaz7kcss1
//
// This shows what it should be:
// https://github.com/project-serum/serum-dex/blob/master/dex/src/instruction.rs#L341

pub fn initialize_market_handler(
    instruction: &UiPartiallyDecodedInstruction,
    _accounts: &Vec<ParsedAccount>,
    _tx: UnboundedSender<MpscMessage>,
    _signature: &String
) {
    let token_address = instruction.accounts.get(7).unwrap();

    info!("{:#?}", instruction);

    info!(
        "Processing {} instruction for {} {}", 
        Paint::magenta("INITIALIZE_MARKET"), 
        Paint::cyan("SERUM_PROGRAM"),
        Paint::black(token_address.to_string())
    );
    info!("{}", Paint::red(_signature));

    // destructure the data to the vault signer nonce
    // instruction.accounts.get(0) is the serum market id address
    // instruction.accounts.get(2) is the serum event queue "
    // instruction.accounts.get(3) is the serum bids "
    // instruction.accounts.get(4) is the serum asks "
    // instruction.accounts.get(5) is the serum coin vault "
    // instruction.accounts.get(6) is the serum pc vault "
    // instruction.accounts.get(7) is the mint "

    // todo: ensure the sender is pump.fun (I guess someone could create one)?
    // todo: parse which accounts are which
    // todo: cache them

    // info!("Serum Initialize Market");
    // let dir_path = format!("./unknown-txs/{}", program_address);
    // fs::create_dir_all(&dir_path).expect("Failed to create directories");

    // let json = serde_json::to_string_pretty(&notification).expect("Failed to serialize notification");

    // let file_path = format!("{}/{}.json", dir_path, "intializeMarket");

    // if !Path::new(&file_path).exists() {
    //     let mut file = File::create(&file_path).expect("Failed to create file");
    //     file.write_all(json.as_bytes()).expect("Failed to write to file");
    // }
}