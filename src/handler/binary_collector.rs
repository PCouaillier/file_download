use curl::easy::{Easy2, Handler};
use std::borrow::Cow;

#[derive(Default)]
pub struct BinaryCollector(Vec<u8>);

impl<'a> std::convert::From<&'a BinaryCollector> for Cow<'a, str> {
    fn from(value: &BinaryCollector) -> Cow<str> {
        String::from_utf8_lossy(&value.0)
    }
}
impl Handler for BinaryCollector {
    fn write(&mut self, data: &[u8]) -> Result<usize, curl::easy::WriteError> {
        self.0.extend_from_slice(data);
        Ok(data.len())
    }
}

impl From<BinaryCollector> for Easy2<BinaryCollector> {
    fn from(c: BinaryCollector) -> Self {
        Self::new(c)
    }
}

impl AsRef<[u8]> for BinaryCollector {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
impl std::fmt::Debug for BinaryCollector {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(fmt, "BinaryCollector([u8; {}])", self.0.len())
    }
}
