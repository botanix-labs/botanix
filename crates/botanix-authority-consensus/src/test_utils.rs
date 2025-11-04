//! Test utilities for the authority consensus crate.
use botanix_authority_peg::mint_validation::{BURN_TOPIC, MINT_CONTRACT_ADDRESS};
use ethabi;
use reth_chainspec::ChainInfo;
use alloy_primitives::{
    address, b256, bytes, BlockHash, BlockNumber, Bytes, B256, U256, TxNumber, TxHash, hex_literal::hex
};
use reth_primitives::{
    Log, LogData, Receipt,
    SealedHeader, TransactionMeta, TransactionSigned,
    TxType,
};
use alloy_consensus::{
    Header,
};

use reth_provider::{
    BlockHashReader, BlockNumReader, HeaderProvider, ProviderResult, ReceiptProvider,
    TransactionsProvider,
};
use alloy_eips::BlockHashOrNumber;
use std::ops::{RangeBounds, RangeInclusive};

/// A mock provider for testing purposes.
#[derive(Default, Clone)]
pub struct MockProvider {
    /// The timestamp used for mock headers
    pub timestamp: u64,
}

impl MockProvider {
    /// Sets the timestamp used for mock headers.
    pub fn set_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }
}

impl MockProvider {
    fn receipt() -> Receipt {
        Receipt {
            tx_type: TxType::Legacy,
            cumulative_gas_used: 0x1u64,
            logs: vec![Log::new_unchecked(
                address!("0000000000000000000000000000000000000011"),
                vec![
                    b256!("000000000000000000000000000000000000000000000000000000000000dead"),
                    b256!("000000000000000000000000000000000000000000000000000000000000beef"),
                ],
                bytes!("0100ff"),
            )],
            success: false,
        }
    }
}

impl ReceiptProvider for MockProvider {

    type Receipt = Receipt;

    fn receipt(&self, _id: TxNumber) -> ProviderResult<Option<Self::Receipt>> {
        Ok(Some(MockProvider::receipt()))
    }

    // return receipt with burn log
    fn receipt_by_hash(&self, _hash: TxHash) -> ProviderResult<Option<Self::Receipt>> {
        // encoded values (amount, destination, version)
        let amount =
            ethabi::Token::Uint(ethabi::ethereum_types::U256::from(10_000_000_000_000_u64));
        let destination = ethabi::Token::String("mrpkDJFJdNGA22FaxCWw6T9oXogXfHU1rh".to_string());
        let version = ethabi::Token::Bytes(vec![0]);
        let payload = ethabi::encode(&[amount, destination, version]);

        let log = Log {
            address: *MINT_CONTRACT_ADDRESS,
            data: LogData::new(
                vec![
                    *BURN_TOPIC,
                    // msg.sender
                    B256::from(hex!(
                        "000000000000000000000000a65812bac44dadb79c3e4930dbd98d5a75376b2a"
                    )),
                ],
                Bytes::copy_from_slice(payload.as_slice()),
            )
            .unwrap(),
        };

        let mut receipt = MockProvider::receipt();
        receipt.logs = vec![log];

        Ok(Some(receipt))
    }

    fn receipts_by_block(&self, _block: BlockHashOrNumber) -> ProviderResult<Option<Vec<Self::Receipt>>> {
        Ok(Some(vec![MockProvider::receipt()]))
    }

    fn receipts_by_tx_range(
        &self,
        _range: impl RangeBounds<TxNumber>,
    ) -> ProviderResult<Vec<Self::Receipt>> {
        Ok(vec![MockProvider::receipt()])
    }

    fn receipts_by_block_range(
        &self,
        block_range: RangeInclusive<BlockNumber>,
    ) -> ProviderResult<Vec<Vec<Self::Receipt>>> {
        let mut result = Vec::new();
        for _ in block_range {
            result.push(vec![MockProvider::receipt()]);
        }
        Ok(result)
    }

}

impl TransactionsProvider for MockProvider {

    type Transaction = TransactionSigned;

    fn transaction_by_hash_with_meta(
        &self,
        _hash: TxHash,
    ) -> ProviderResult<Option<(TransactionSigned, TransactionMeta)>> {
        let tx_signed = TransactionSigned::default();
        let tx_meta = TransactionMeta::default();

        Ok(Some((tx_signed, tx_meta)))
    }

    fn transaction_id(&self, _tx_hash: TxHash) -> ProviderResult<Option<TxNumber>> {
        unimplemented!();
    }

    fn transaction_by_id(&self, _id: TxNumber) -> ProviderResult<Option<Self::Transaction>> {
        unimplemented!();
    }

    fn transaction_by_hash(&self, _hash: TxHash) -> ProviderResult<Option<Self::Transaction>> {
        unimplemented!();
    }

    fn transaction_block(&self, _id: TxNumber) -> ProviderResult<Option<BlockNumber>> {
        unimplemented!();
    }

    fn transactions_by_block(
        &self,
        _block: BlockHashOrNumber,
    ) -> ProviderResult<Option<Vec<Self::Transaction>>> {
        unimplemented!();
    }

    fn transactions_by_block_range(
        &self,
        _range: impl RangeBounds<BlockNumber>,
    ) -> ProviderResult<Vec<Vec<Self::Transaction>>> {
        unimplemented!();
    }

    fn transactions_by_tx_range(
        &self,
        _range: impl RangeBounds<TxNumber>,
    ) -> ProviderResult<Vec<Self::Transaction>> {
        unimplemented!();
    }

    fn senders_by_tx_range(
        &self,
        _range: impl RangeBounds<TxNumber>,
    ) -> ProviderResult<Vec<alloy_primitives::Address>> {
        unimplemented!();
    }

    fn transaction_sender(
        &self,
        _id: TxNumber,
    ) -> std::result::Result<
        std::option::Option<alloy_primitives::Address>,
        reth_provider::ProviderError,
    > {
        unimplemented!();
    }

    fn transaction_by_id_unhashed(&self,id:TxNumber) -> ProviderResult<Option<Self::Transaction> >  {
        unimplemented!();
    }
}

impl BlockHashReader for MockProvider {
    fn block_hash(&self, _number: BlockNumber) -> ProviderResult<Option<B256>> {
        unimplemented!()
    }

    fn convert_block_hash(
        &self,
        _hash_or_number: BlockHashOrNumber,
    ) -> ProviderResult<Option<B256>> {
        unimplemented!()
    }

    fn canonical_hashes_range(
        &self,
        _start: BlockNumber,
        _end: BlockNumber,
    ) -> ProviderResult<Vec<B256>> {
        unimplemented!()
    }
}

impl BlockNumReader for MockProvider {
    fn chain_info(&self) -> ProviderResult<ChainInfo> {
        unimplemented!()
    }

    fn block_number(&self, _hash: B256) -> ProviderResult<Option<BlockNumber>> {
        unimplemented!()
    }

    fn best_block_number(&self) -> ProviderResult<BlockNumber> {
        unimplemented!()
    }

    fn last_block_number(&self) -> ProviderResult<BlockNumber> {
        unimplemented!()
    }

    fn convert_hash_or_number(
        &self,
        _id: BlockHashOrNumber,
    ) -> ProviderResult<Option<BlockNumber>> {
        unimplemented!()
    }

    fn convert_number(&self, _id: BlockHashOrNumber) -> ProviderResult<Option<B256>> {
        unimplemented!()
    }
}

impl HeaderProvider for MockProvider {

    type Header = Header;

    fn header_by_number(&self, _num: u64) -> ProviderResult<Option<Self::Header>> {
        Ok(Some(Header { timestamp: self.timestamp, ..Default::default() }))
    }

    fn header(&self, _block_hash: &BlockHash) -> ProviderResult<Option<Self::Header>> {
        unimplemented!()
    }

    fn header_td(&self, _hash: &BlockHash) -> ProviderResult<Option<U256>> {
        unimplemented!()
    }

    fn header_td_by_number(&self, _number: BlockNumber) -> ProviderResult<Option<U256>> {
        unimplemented!()
    }

    fn headers_range(&self, _range: impl RangeBounds<BlockNumber>) -> ProviderResult<Vec<Self::Header>> {
        unimplemented!()
    }

    fn sealed_header(&self, _number: BlockNumber) -> ProviderResult<Option<SealedHeader<Self::Header>>> {
        unimplemented!()
    }

    fn sealed_headers_while(
        &self,
        _range: impl RangeBounds<BlockNumber>,
        _predicate: impl FnMut(&SealedHeader) -> bool,
    ) -> ProviderResult<Vec<SealedHeader<Self::Header>>> {
        unimplemented!()
    }
}
