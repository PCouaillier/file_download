use super::BinaryReprFormat;
use crate::error::*;
use crate::IterChunk;
use base64::engine::general_purpose::{GeneralPurpose, STANDARD};
use base64::Engine as _;
pub const BASE64_ENGINE: GeneralPurpose = STANDARD;

#[derive(Debug, Clone)]
pub struct BinaryRepr {
    value: Vec<u8>,
    format: BinaryReprFormat,
}

impl BinaryRepr {
    /// ```
    /// use file_download::hash::{BinaryRepr, BinaryReprFormat};
    /// let b = BinaryRepr::new("01FF", BinaryReprFormat::Hex).unwrap();
    /// assert_eq!(b, BinaryRepr::new("Af8=", BinaryReprFormat::Base64).unwrap());
    /// assert_eq!(true, BinaryRepr::new("1F", BinaryReprFormat::Base64).is_err());
    /// assert_eq!(true, BinaryRepr::new("G", BinaryReprFormat::Hex).is_err());
    /// assert_eq!(Vec::from([1u8,255u8]), BinaryRepr::new("0000000111111111", BinaryReprFormat::Bin).unwrap().decode());
    /// ```
    pub fn new(value: &str, format: BinaryReprFormat) -> Result<Self, BinaryReprError> {
        match &format {
            BinaryReprFormat::Base64 => BASE64_ENGINE.decode(value)
                .map_err(|err| BinaryReprError::new(value, BinaryReprFormat::Base64, err.into())),
            BinaryReprFormat::Hex => hex::decode(value)
                .map_err(|err| BinaryReprError::new(value, BinaryReprFormat::Hex, err.into())),
            BinaryReprFormat::Bin => from_bin(value),
        }
        .map(|value| Self { value, format })
    }

    /// ```
    /// use file_download::hash::{BinaryRepr, BinaryReprFormat};
    /// let v = [1u8, 255u8];
    /// let b = BinaryRepr::new("01FF", BinaryReprFormat::Hex).unwrap();
    /// assert_eq!(b.decode(), &v);
    /// let b = BinaryRepr::new("Af8=", BinaryReprFormat::Base64).unwrap();
    /// assert_eq!(b.decode(), &v);
    /// ```
    pub fn decode(&self) -> Vec<u8> {
        self.value.clone()
    }

    /// ```
    /// use file_download::hash::{BinaryRepr, BinaryReprFormat};
    /// let v = "Af8=";
    /// let b = BinaryRepr::new(v, BinaryReprFormat::Base64).unwrap();
    /// assert_eq!(b.to_base64(), v);
    /// ```
    pub fn to_base64(&self) -> String {
        BASE64_ENGINE.encode(&self.value)
    }

    /// ```
    /// use file_download::hash::{BinaryRepr, BinaryReprFormat};
    /// let v = "1f";
    /// let b = BinaryRepr::new(v, BinaryReprFormat::Hex).unwrap();
    /// assert_eq!(b.to_hex(), v);
    /// ```
    pub fn to_hex(&self) -> String {
        hex::encode(&self.value)
    }

    /// ```
    /// use file_download::hash::{BinaryRepr, BinaryReprFormat};
    /// let b = BinaryRepr::new("1F", BinaryReprFormat::Hex).unwrap();
    /// assert_eq!(b.to_bin(), "00011111");
    /// ```
    pub fn to_bin(&self) -> String {
        let mut s = String::with_capacity(self.value.len() * 8);
        for n in self.value.iter() {
            for i in 0..8u8 {
                s.push(if 0 < (n & (1u8 << i)) { '1' } else { '0' });
            }
        }
        s.chars().rev().collect()
    }
}

impl std::fmt::Display for BinaryRepr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.format {
            BinaryReprFormat::Hex => self.to_hex(),
            BinaryReprFormat::Base64 => self.to_base64(),
            BinaryReprFormat::Bin => format!("{:?}", &self.value),
        })
    }
}

impl PartialEq<Self> for BinaryRepr {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}

fn from_bin(chars: &str) -> Result<Vec<u8>, BinaryReprError> {
    let mut res = Vec::with_capacity(chars.len() / 8 + usize::from(chars.len() % 8 != 0));
    for chunk_c in IterChunk::new(chars.as_bytes().iter().rev(), 8) {
        let mut chunk_val = 0u8;
        let chunk_len = chunk_c.len();
        for i in 0..chunk_len {
            if let Some(v) = chunk_c.get(i) {
                if **v == b'1' {
                    chunk_val += 1 << i;
                } else if **v != b'0' {
                    return Err(BinaryReprError::new(
                        chars,
                        BinaryReprFormat::Bin,
                        BinaryReprRootError::None,
                    ));
                }
            }
        }
        res.push(chunk_val);
    }
    res.reverse();
    Ok(res)
}

#[cfg(test)]
mod test {
    #[test]
    fn test_from_bin() {
        assert_eq!(super::from_bin("1").unwrap(), Vec::from([1]));
        assert_eq!(
            super::from_bin("0000000111111111").unwrap(),
            Vec::from([1, 255])
        );
    }
}
