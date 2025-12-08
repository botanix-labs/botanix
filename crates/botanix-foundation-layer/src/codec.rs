//! CBOR serialization utilities with byte counting.
//!
//! This module provides a [`Codec`] trait that wraps `ciborium` to track the
//! number of bytes read/written during serialization. This is necessary because
//! `ciborium` doesn't expose this information, making it difficult to handle
//! trailing data or concatenated values.
use serde::{de::DeserializeOwned, Serialize};
use std::io::{Read, Write};

/// A reader wrapper that tracks the number of bytes read.
struct CountingReader<R> {
    inner: R,
    bytes_read: usize,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read += n;
        Ok(n)
    }
}

/// A writer wrapper that tracks the number of bytes written.
struct CountingWriter<W> {
    inner: W,
    bytes_written: usize,
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes_written += n;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// CBOR encoding/decoding with byte counting.
///
/// Automatically implemented for any type that implements `Serialize` and
/// `DeserializeOwned`.
pub trait Codec: Sized + Serialize + DeserializeOwned {
    /// Encodes the value to a new `Vec<u8>`.
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        let _len =
            self.encode_to(&mut buf).expect("writing to heap must not fail");
        buf
    }
    /// Encodes the value to a writer, returning the number of bytes written.
    fn encode_to<W>(
        &self,
        writer: W,
    ) -> Result<usize, ciborium::ser::Error<std::io::Error>>
    where
        W: Write,
    {
        let mut writer = CountingWriter { inner: writer, bytes_written: 0 };

        ciborium::into_writer(self, &mut writer)?;
        Ok(writer.bytes_written)
    }
    /// Decodes a value from a reader, returning the value and bytes consumed.
    ///
    /// **Note**: If the reader is `&mut &[u8]`, it will be advanced past the
    /// decoded value. Otherwise, use the returned byte count to manually track
    /// position.
    fn decode<R>(
        reader: R,
    ) -> Result<(Self, usize), ciborium::de::Error<std::io::Error>>
    where
        R: Read,
    {
        let mut reader = CountingReader { inner: reader, bytes_read: 0 };

        let this: Self = ciborium::from_reader(&mut reader)?;
        Ok((this, reader.bytes_read))
    }
}

/// Blanket implementation for any type that implements `Serialize` and
/// `DeserializeOwned`.
impl<T> Codec for T where T: Serialize + DeserializeOwned {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, serde::Deserialize)]
    struct TestData([u8; 10]);

    #[test]
    fn encode_decode() {
        let original = TestData([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

        let bytes = original.encode();
        let (decoded, bytes_read) = TestData::decode(bytes.as_slice()).unwrap();

        assert_eq!(original, decoded);
        assert_eq!(bytes_read, bytes.len());
    }

    #[test]
    fn encode_to_decode() {
        let original = TestData([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

        let mut bytes = Vec::new();
        let bytes_written = original.encode_to(&mut bytes).unwrap();
        let (decoded, bytes_read) = TestData::decode(bytes.as_slice()).unwrap();

        assert_eq!(original, decoded);
        // 1 byte CBOR header + 10 bytes data
        assert_eq!(bytes_written, 11);
        assert_eq!(bytes_read, 11);
        assert_eq!(bytes_written, bytes_read);
    }

    #[test]
    fn trailing_data_ignored() {
        let original = TestData([0; 10]);
        let mut bytes = original.encode();
        // Trailing garbage
        bytes.extend_from_slice(&[33, 44, 55, 66]);

        let (decoded, bytes_read) = TestData::decode(bytes.as_slice()).unwrap();

        assert_eq!(original, decoded);
        // Didn't consume trailing data
        assert_eq!(bytes_read, bytes.len() - 4);
    }
}
