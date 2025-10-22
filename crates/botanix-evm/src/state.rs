use std::{collections::HashMap, str::FromStr};

use botanix_chainspec::BotanixChainSpec;
use botanix_consensus_common::utils;
use reth_revm::primitives::Address;

/// Collect all balance changes at the end of the block.
///
/// Balance changes involve splitting the block fees between the
/// LST FeeReceiver, Botanix fee recipient, and the block fee recipient address.
#[inline]
pub fn post_block_balance_increments(
    chain_spec: &BotanixChainSpec,
    total_block_fees: Option<u128>,
    block_fee_recipient_address: Option<Address>,
) -> HashMap<Address, u128> {
    let mut balance_increments = HashMap::new();

    // Split block fees between LST FeeReceiver, Botanix, and the Block Fee Recipient address.
    // A conditional statement is needed so reth tests can pass:
    // sometimes tests will pass None for fees (ie processor eip4788 tests)
    // sometimes it will pass fees with a zero block fee recipient address (ie blockhchain_tree fork
    // choice tests). During normal operation, the block fee recipient address will never be a zero
    // address: it will be an address passed by the node operator.
    let fees = total_block_fees.unwrap_or(0);
    let block_fee_recipient = block_fee_recipient_address.unwrap_or(Address::ZERO);

    if fees > 0 &&
        block_fee_recipient != Address::ZERO &&
        chain_spec.botanix_fee_recipient.is_some() &&
        chain_spec.lst_fee_receiver.is_some()
    {
        let (lst_fee_receiver_fees, botanix_fees, block_fee_recipient_fees) =
            utils::block_fees_split(fees);

        // FeeReceiver fees
        let lst_fee_receiver_addr = Address::from_str(
            chain_spec.lst_fee_receiver.clone().expect("FeeReceiver to exist").as_str(),
        )
        .expect("Valid FeeReceiver");
        balance_increments
            .entry(lst_fee_receiver_addr)
            .and_modify(|bal: &mut u128| {
                *bal = bal
                    .checked_add(lst_fee_receiver_fees)
                    .expect("overflow incrementing balance for LST FeeReceiver");
            })
            .or_insert(lst_fee_receiver_fees);

        // Botanix fees
        let botanix_addr = Address::from_str(
            chain_spec.botanix_fee_recipient.clone().expect("FeeReceiver to exist").as_str(),
        )
        .expect("Valid FeeReceiver");
        balance_increments
            .entry(botanix_addr)
            .and_modify(|bal| {
                *bal = bal
                    .checked_add(botanix_fees)
                    .expect("overflow incrementing balance for Botanix fee recipient");
            })
            .or_insert(botanix_fees);

        // Block fee recipient fees
        balance_increments
            .entry(block_fee_recipient)
            .and_modify(|bal| {
                *bal = bal
                    .checked_add(block_fee_recipient_fees)
                    .expect("overflow incrementing balance for block fee recipient");
            })
            .or_insert(block_fee_recipient_fees);
    }

    balance_increments
}
