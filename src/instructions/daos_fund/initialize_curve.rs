use solana_transaction_status::{parse_accounts::ParsedAccount, UiPartiallyDecodedInstruction, UiTransactionStatusMeta};
use tokio::sync::mpsc::UnboundedSender;
use prost::Message;
use tracing::info;
use yansi::Paint;

use crate::messaging::MpscMessage;

pub mod daos_fund {
    tonic::include_proto!("daos_fund");
}

use daos_fund::DaosFundInitializeCurveNotification;

pub fn initialize_curve_handler(
    _slot: u64,
    instruction: &UiPartiallyDecodedInstruction,
    _accounts: &Vec<ParsedAccount>,
    _meta: &UiTransactionStatusMeta,
    _signature: &String,
    tx: UnboundedSender<MpscMessage>
) {
    let token_address = instruction.accounts.get(1).unwrap();
    let config_address = instruction.accounts.get(3).unwrap();
    let curve_address = instruction.accounts.get(9).unwrap();

    let message = DaosFundInitializeCurveNotification {
        token_address: token_address.to_string(),
        config_address: config_address.to_string(),
        curve_address: curve_address.to_string(),
    };

    tx.send(MpscMessage {
        topic: "daos_fund_initialize_curve".to_string(),
        payload: message.encode_to_vec(),
    }).expect("Failed to send MPSC Message.");

    info!(
        "Processing {} instruction for {} {}", 
        Paint::magenta("INITIALIZE_CURVE"), 
        Paint::cyan("DAOS_FUND_PROGRAM"), 
        Paint::black(token_address)
    )
}