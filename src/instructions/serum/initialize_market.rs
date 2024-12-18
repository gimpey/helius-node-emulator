/// Data Deserialization: https://github.com/project-serum/serum-ts/blob/master/packages/serum/src/instructions.js#L33
/// 
/// Accounts: https://github.com/project-serum/serum-ts/blob/master/packages/serum/src/instructions.js#L195

use solana_transaction_status::{parse_accounts::ParsedAccount, UiPartiallyDecodedInstruction};
use tokio::sync::mpsc::UnboundedSender;

use crate::messaging::MpscMessage;

pub fn initialize_market_handler(
    _instruction: &UiPartiallyDecodedInstruction,
    _accounts: &Vec<ParsedAccount>,
    _tx: UnboundedSender<MpscMessage>,
) {
    // todo: ensure the sender is pump.fun (I guess someone could create one)?
    // todo: parse which accounts are which
    // todo: cache them
}