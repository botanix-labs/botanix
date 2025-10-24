use alloy_consensus::{transaction::Transaction, TxType};

/// Determines the transaction type by checking EIP standards.
/// Checks in order of precedence (newest to oldest) to ensure correct classification.
pub fn tx_type<T: Transaction>(transaction: &T) -> TxType {
    if transaction.is_eip7702() {
        TxType::Eip7702
    } else if transaction.is_eip4844() {
        TxType::Eip4844
    } else if transaction.is_eip1559() {
        TxType::Eip1559
    } else if transaction.is_eip2930() {
        TxType::Eip2930
    } else {
        TxType::Legacy
    }
}

#[cfg(test)]
mod tests {
    use alloy_consensus::{Signed, TxEip1559};
    use reth_revm::primitives::alloy_primitives::Signature;

    use super::*;

    #[test]
    fn test_tx_type() {
        let tx = Signed::new_unhashed(
            TxEip1559 {
                chain_id: 1,
                gas_limit: 21000,
                max_fee_per_gas: 1000,
                max_priority_fee_per_gas: 1000,
                ..Default::default()
            },
            Signature::new(Default::default(), Default::default(), Default::default()),
        );
        assert_eq!(tx_type(&tx), TxType::Eip1559);

        // Add tests for other transaction types as needed
    }
}
