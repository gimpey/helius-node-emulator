use solana_transaction_status::{option_serializer::OptionSerializer, UiTransactionTokenBalance};

pub fn find_token_balance_by_address<'a>(
    objects: &'a [UiTransactionTokenBalance],
    target_value: &str,
) -> Option<&'a UiTransactionTokenBalance> {
    for obj in objects {
        if let OptionSerializer::Some(owner) = &obj.owner {
            if owner == target_value {
                return Some(obj);
            }
        }
    }
    None
}