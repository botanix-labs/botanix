//! Models for migration records.
//!
//! This module contains the data structures used for tracking multisig migrations
//! during the dynafed transition process.

use alloy_primitives::B256;
use bytes::BufMut;
use reth_db_api::table::{Compress, Decompress};
use serde::{Deserialize, Serialize};

/// A migration ID is a 32-byte identifier (UUID stored as B256).
pub type MigrationId = B256;

/// The status of a multisig migration.
#[derive(
    Debug, Default, Eq, PartialEq, Clone, Copy, Serialize, Deserialize,
)]
#[cfg_attr(any(test, feature = "arbitrary"), derive(arbitrary::Arbitrary))]
pub enum MigrationStatus {
    /// Migration has been started but DKG hasn't begun yet.
    #[default]
    Started = 0,
    /// DKG is running for the target multisig.
    Running = 1,
    /// Migration has completed successfully.
    Finished = 2,
}

impl From<u8> for MigrationStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => MigrationStatus::Started,
            1 => MigrationStatus::Running,
            2 => MigrationStatus::Finished,
            _ => MigrationStatus::Started,
        }
    }
}

impl From<MigrationStatus> for u8 {
    fn from(value: MigrationStatus) -> Self {
        match value {
            MigrationStatus::Started => 0,
            MigrationStatus::Running => 1,
            MigrationStatus::Finished => 2,
        }
    }
}

impl Compress for MigrationStatus {
    type Compressed = Vec<u8>;

    fn compress_to_buf<B: BufMut + AsMut<[u8]>>(&self, buf: &mut B) {
        buf.put_u8((*self).into());
    }
}

impl Decompress for MigrationStatus {
    fn decompress(
        value: &[u8],
    ) -> Result<Self, reth_storage_errors::db::DatabaseError> {
        if value.is_empty() {
            return Ok(MigrationStatus::Started);
        }
        Ok(MigrationStatus::from(value[0]))
    }
}

/// A migration record stored in the database.
///
/// This represents an active multisig migration from one federation
/// to another during the dynafed transition process.
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "arbitrary"), derive(arbitrary::Arbitrary))]
pub struct MigrationRecord {
    /// The unique migration ID (UUID stored as B256).
    migration_id: B256,
    /// The source multisig ID (funds are being moved FROM this multisig).
    multisig_id_from: u32,
    /// The target multisig ID (funds are being moved TO this multisig).
    multisig_id_to: u32,
    /// The current status of the migration.
    status: MigrationStatus,
}

// Layout: migration_id (32 bytes) + multisig_id_from (4 bytes) + multisig_id_to (4 bytes) + status (1 byte) = 41 bytes
const MIGRATION_RECORD_SIZE: usize = 32 + 4 + 4 + 1;

impl Compress for MigrationRecord {
    type Compressed = Vec<u8>;

    fn compress_to_buf<B: BufMut + AsMut<[u8]>>(&self, buf: &mut B) {
        buf.put_slice(self.migration_id.as_slice());
        buf.put_u32_le(self.multisig_id_from);
        buf.put_u32_le(self.multisig_id_to);
        buf.put_u8(self.status.into());
    }
}

impl Decompress for MigrationRecord {
    fn decompress(
        value: &[u8],
    ) -> Result<Self, reth_storage_errors::db::DatabaseError> {
        if value.len() < MIGRATION_RECORD_SIZE {
            return Err(reth_storage_errors::db::DatabaseError::Decode);
        }

        let migration_id = B256::from_slice(&value[0..32]);
        let multisig_id_from =
            u32::from_le_bytes(value[32..36].try_into().expect("4 bytes"));
        let multisig_id_to =
            u32::from_le_bytes(value[36..40].try_into().expect("4 bytes"));
        let status = MigrationStatus::from(value[40]);

        Ok(Self { migration_id, multisig_id_from, multisig_id_to, status })
    }
}

impl MigrationRecord {
    /// Creates a new migration record.
    ///
    /// # Parameters
    ///
    /// * `migration_id` - The unique migration ID (UUID as B256)
    /// * `multisig_id_from` - The source multisig ID
    /// * `multisig_id_to` - The target multisig ID
    /// * `status` - The initial status of the migration
    ///
    /// # Returns
    ///
    /// A new `MigrationRecord` instance.
    pub fn new(
        migration_id: B256,
        multisig_id_from: u32,
        multisig_id_to: u32,
        status: MigrationStatus,
    ) -> Self {
        Self { migration_id, multisig_id_from, multisig_id_to, status }
    }

    /// Returns the migration ID.
    pub const fn migration_id(&self) -> B256 {
        self.migration_id
    }

    /// Returns the source multisig ID.
    pub const fn multisig_id_from(&self) -> u32 {
        self.multisig_id_from
    }

    /// Returns the target multisig ID.
    pub const fn multisig_id_to(&self) -> u32 {
        self.multisig_id_to
    }

    /// Returns the current status.
    pub const fn status(&self) -> MigrationStatus {
        self.status
    }

    /// Sets the status of the migration.
    pub fn set_status(&mut self, status: MigrationStatus) {
        self.status = status;
    }
}

/// Converts a `uuid::Uuid` to a `MigrationId` (B256).
///
/// # Parameters
///
/// * `uuid` - The UUID to convert
///
/// # Returns
///
/// A 32-byte `MigrationId` with the UUID bytes in the first 16 bytes
/// and zeros in the remaining 16 bytes.
pub fn uuid_to_migration_id(uuid: uuid::Uuid) -> MigrationId {
    let mut bytes = [0u8; 32];
    bytes[0..16].copy_from_slice(uuid.as_bytes());
    bytes.into()
}

/// Converts a `MigrationId` (B256) back to a `uuid::Uuid`.
///
/// # Parameters
///
/// * `migration_id` - The MigrationId to convert
///
/// # Returns
///
/// The UUID extracted from the first 16 bytes of the MigrationId.
pub fn migration_id_to_uuid(migration_id: MigrationId) -> uuid::Uuid {
    let bytes: [u8; 16] =
        migration_id.as_slice()[0..16].try_into().expect("16 bytes");
    uuid::Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_record() {
        let uuid = uuid::Uuid::new_v4();
        let migration_id = uuid_to_migration_id(uuid);
        let record =
            MigrationRecord::new(migration_id, 0, 1, MigrationStatus::Started);

        assert_eq!(record.migration_id(), migration_id);
        assert_eq!(record.multisig_id_from(), 0);
        assert_eq!(record.multisig_id_to(), 1);
        assert_eq!(record.status(), MigrationStatus::Started);

        // Test round-trip UUID conversion
        let recovered_uuid = migration_id_to_uuid(migration_id);
        assert_eq!(uuid, recovered_uuid);
    }

    #[test]
    fn test_status_update() {
        let migration_id = B256::ZERO;
        let mut record =
            MigrationRecord::new(migration_id, 0, 1, MigrationStatus::Started);

        assert_eq!(record.status(), MigrationStatus::Started);

        record.set_status(MigrationStatus::Running);
        assert_eq!(record.status(), MigrationStatus::Running);

        record.set_status(MigrationStatus::Finished);
        assert_eq!(record.status(), MigrationStatus::Finished);
    }

    #[test]
    fn test_migration_record_compress_decompress() {
        let migration_id = B256::repeat_byte(0xAB);
        let original = MigrationRecord::new(
            migration_id,
            123,
            456,
            MigrationStatus::Running,
        );

        let mut buf = vec![];
        original.compress_to_buf(&mut buf);
        assert_eq!(buf.len(), MIGRATION_RECORD_SIZE);

        let decompressed = MigrationRecord::decompress(&buf).unwrap();
        assert_eq!(original, decompressed);
    }

    #[test]
    fn test_migration_status_compress_decompress() {
        for status in [
            MigrationStatus::Started,
            MigrationStatus::Running,
            MigrationStatus::Finished,
        ] {
            let mut buf = vec![];
            status.compress_to_buf(&mut buf);
            assert_eq!(buf.len(), 1);

            let decompressed = MigrationStatus::decompress(&buf).unwrap();
            assert_eq!(status, decompressed);
        }
    }
}
