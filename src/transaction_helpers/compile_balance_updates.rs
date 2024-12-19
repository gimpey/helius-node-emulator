use solana_transaction_status::{option_serializer::OptionSerializer, UiTransactionTokenBalance};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OwnerMintKey {
    pub owner: String,
    pub mint: String,
}

impl OwnerMintKey {
    pub fn new(owner: String, mint: String) -> Self {
        OwnerMintKey { owner, mint }
    }
}

#[derive(Debug)]
pub struct BalanceUpdate {
    pub pre: u64,
    pub post: u64,
    pub decimals: u8,
}

pub fn compile_balance_updates(
    pre_token_balances: &[UiTransactionTokenBalance],
    post_token_balances: &[UiTransactionTokenBalance],
) -> HashMap<OwnerMintKey, BalanceUpdate> {
    let mut balance_map: HashMap<OwnerMintKey, BalanceUpdate> = HashMap::new();

    for pre_balance in pre_token_balances {
        if let OptionSerializer::Some(owner) = &pre_balance.owner {
            let key = OwnerMintKey::new(owner.clone(), pre_balance.mint.clone());
            let pre_amount = pre_balance.ui_token_amount.amount.parse::<u64>().unwrap_or(0);
            let decimals = pre_balance.ui_token_amount.decimals;
            
            balance_map.entry(key)
                .and_modify(|balance| {
                    balance.pre = pre_amount;
                    balance.decimals = decimals;
                })
                .or_insert(BalanceUpdate {
                    pre: pre_amount,
                    post: 0,
                    decimals,
                });
        }
    }

    for post_balance in post_token_balances {
        if let OptionSerializer::Some(owner) = &post_balance.owner {
            let key = OwnerMintKey::new(owner.clone(), post_balance.mint.clone());
            let post_amount = post_balance.ui_token_amount.amount.parse::<u64>().unwrap_or(0);
            let decimals = post_balance.ui_token_amount.decimals;

            balance_map.entry(key)
                .and_modify(|balance| {
                    balance.post = post_amount;
                    balance.decimals = decimals;
                })
                .or_insert(BalanceUpdate {
                    pre: 0,
                    post: post_amount,
                    decimals,
                });
        }
    }

    balance_map
}
