use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Wrapper for any type that implements `Serialize` and `Deserialize` to be
/// used as a generic MDBX key. This uses regular CBOR encoding without any
/// special optimization.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct UnoptimizedKey<T>(pub T);

impl<T> UnoptimizedKey<T> {
    /// Consumes and returns the inner key.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for UnoptimizedKey<T> {
    fn from(value: T) -> Self {
        UnoptimizedKey(value)
    }
}

/// Keys are de/serialized using the `Encode` and `Decode` traits.
impl<T> reth_db_api::table::Encode for UnoptimizedKey<T>
where
    T: Send + Sync + Sized + std::fmt::Debug + Serialize,
{
    type Encoded = Vec<u8>;

    fn encode(self) -> Self::Encoded {
        let mut bytes = vec![];
        ciborium::into_writer(&self.0, &mut bytes)
            .expect("writer must not fail");

        bytes
    }
}

/// Keys are de/serialized using the `Encode` and `Decode` traits.
impl<T> reth_db_api::table::Decode for UnoptimizedKey<T>
where
    T: Send + Sync + Sized + std::fmt::Debug + DeserializeOwned,
{
    fn decode(value: &[u8]) -> Result<Self, reth_db::DatabaseError> {
        ciborium::from_reader::<T, _>(value)
            .map(UnoptimizedKey)
            .map_err(|_| reth_db::DatabaseError::Decode)
    }
}

/// Wrapper for any type that implements `Serialize` and `Deserialize` to be
/// used as a generic MDBX value. This uses regular CBOR encoding without any
/// special optimization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnoptimizedValue<T>(pub T);

impl<T> UnoptimizedValue<T> {
    /// Consumes and returns the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for UnoptimizedValue<T> {
    fn from(value: T) -> Self {
        UnoptimizedValue(value)
    }
}

/// Values are de/serialized (“compressed”) using the `Compress` and
/// `Decompress` traits.
impl<T> reth_db_api::table::Compress for UnoptimizedValue<T>
where
    T: Send + Sync + Sized + std::fmt::Debug + Serialize,
{
    type Compressed = Vec<u8>;

    fn compress_to_buf<B: bytes::BufMut>(&self, buf: &mut B) {
        ciborium::into_writer(&self.0, bytes::BufMut::writer(buf))
            .expect("writer must not fail");
    }
}

/// Values are de/serialized (“compressed”) using the `Compress` and
/// `Decompress` traits.
impl<T> reth_db_api::table::Decompress for UnoptimizedValue<T>
where
    T: Send + Sync + Sized + std::fmt::Debug + DeserializeOwned,
{
    fn decompress(value: &[u8]) -> Result<Self, reth_db::DatabaseError> {
        ciborium::from_reader::<T, _>(value)
            .map(UnoptimizedValue)
            .map_err(|_| reth_db::DatabaseError::Decode)
    }
}
