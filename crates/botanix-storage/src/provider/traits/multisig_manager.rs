use crate::models::{
    AttestedMultisigEntry, MultisigRecord, StagedMultisigEntry,
};
use botanix_types::MultisigId;
use reth_storage_errors::provider::ProviderResult;

/// Read and write operations for multisig federation lifecycle management.
#[auto_impl::auto_impl(&, Arc, Box)]
pub trait MultisigManagerReader: Send + Sync {
    /// Returns the entry for the given staging multisig ID, or `None` if not
    /// found.
    fn get_staging_multisig(
        &self,
        id: MultisigId,
    ) -> ProviderResult<Option<StagedMultisigEntry>>;
    /// Returns the entry for the given attested multisig ID, or `None` if not
    /// found.
    fn get_attested_multisig(
        &self,
        id: MultisigId,
    ) -> ProviderResult<Option<AttestedMultisigEntry>>;
    /// Returns all attested multisigs where pegins are allowed to, respectively
    /// funding and degraded multisigs.
    fn get_active_multisigs(
        &self,
    ) -> ProviderResult<Vec<AttestedMultisigEntry>>;
    /// Returns all raw multisig records from the database, both staged and
    /// attested, regardless of lifecycle status.
    fn get_multisig_records(&self) -> ProviderResult<Vec<MultisigRecord>>;
}

/// Read and write operations for multisig federation lifecycle management.
#[auto_impl::auto_impl(&, Arc, Box)]
pub trait MultisigManagerWriter: Send + Sync {
    /// Inserts a new staging multisig entry into the database.
    fn insert_staging_multisig(
        &self,
        id: MultisigId,
        entry: StagedMultisigEntry,
    ) -> ProviderResult<()>;
    /// Inserts a new attested multisig entry into the database.
    fn insert_attested_multisig(
        &self,
        id: MultisigId,
        entry: AttestedMultisigEntry,
    ) -> ProviderResult<()>;
    /// Removes the multisig entry for the given ID, returning `true` if the
    /// entry was removed or `false` if otherwise.
    fn remove_multisig(&self, id: MultisigId) -> ProviderResult<bool>;
}
