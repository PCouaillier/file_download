use crate::BinaryReprFormat;
#[cfg(feature = "async-std")]
use async_std::io;
use base64::DecodeError;
use hex::FromHexError;
use std::{
    error::Error,
    ffi::OsString,
    fmt::{Debug, Display, Formatter, Result as FmtResult},
};
#[cfg(all(not(feature = "async-std"), feature = "tokio"))]
use tokio::io;

#[derive(Debug)]
pub struct BadCheckSumErrorDetail {
    pub url: String,
    pub expected_hash: String,
    pub current_hash: String,
}

#[derive(Debug)]
pub struct BadCheckSumError {
    pub file_sources: Vec<BadCheckSumErrorDetail>,
}
impl Display for BadCheckSumError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(self, f)
    }
}
impl Error for BadCheckSumError {}

impl From<Vec<BadCheckSumErrorDetail>> for BadCheckSumError {
    fn from(file_sources: Vec<BadCheckSumErrorDetail>) -> Self {
        Self { file_sources }
    }
}

#[derive(Clone)]
pub struct ThreadSafeError {
    pub message: String,
}
impl Display for ThreadSafeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_tuple("ThreadSafeError")
            .field(&self.message)
            .finish()
    }
}
impl Debug for ThreadSafeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Display::fmt(&self, f)
    }
}

impl Error for ThreadSafeError {}

impl<T: Into<String>> From<T> for ThreadSafeError {
    fn from(f: T) -> Self {
        ThreadSafeError { message: f.into() }
    }
}

#[derive(Debug)]
pub enum CurlError {
    CurlError(curl::Error),
    CurlMultiError(curl::MultiError),
    ThreadSafeError(ThreadSafeError),
}
impl Display for CurlError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(&self, f)
    }
}

impl Error for CurlError {}

impl From<curl::Error> for CurlError {
    fn from(error: curl::Error) -> Self {
        Self::CurlError(error)
    }
}
impl From<curl::MultiError> for CurlError {
    fn from(error: curl::MultiError) -> Self {
        Self::CurlMultiError(error)
    }
}
impl From<ThreadSafeError> for CurlError {
    fn from(error: ThreadSafeError) -> Self {
        Self::ThreadSafeError(error)
    }
}
impl From<CurlError> for ThreadSafeError {
    fn from(error: CurlError) -> Self {
        Self {
            message: format!("{:?}", error),
        }
    }
}

#[derive(Debug)]
pub enum DlError {
    BadCheckSumError(BadCheckSumError),
    CurlError(CurlError),
    IoError(io::Error),
}
impl Display for DlError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(self, f)
    }
}
impl Error for DlError {}

impl From<BadCheckSumError> for DlError {
    fn from(error: BadCheckSumError) -> Self {
        Self::BadCheckSumError(error)
    }
}
impl From<CurlError> for DlError {
    fn from(error: CurlError) -> Self {
        Self::CurlError(error)
    }
}
impl From<curl::Error> for DlError {
    fn from(error: curl::Error) -> Self {
        Self::CurlError(error.into())
    }
}
impl From<curl::MultiError> for DlError {
    fn from(error: curl::MultiError) -> Self {
        Self::CurlError(error.into())
    }
}
impl From<io::Error> for DlError {
    fn from(error: io::Error) -> Self {
        Self::IoError(error)
    }
}

#[derive(Debug)]
pub enum ThreadSafeDlError {
    BadCheckSumError(BadCheckSumError),
    ThreadSafeError(ThreadSafeError),
}
impl Display for ThreadSafeDlError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(&self, f)
    }
}
impl Error for ThreadSafeDlError {}

impl From<BadCheckSumError> for ThreadSafeDlError {
    fn from(error: BadCheckSumError) -> Self {
        Self::BadCheckSumError(error)
    }
}
impl From<ThreadSafeError> for ThreadSafeDlError {
    fn from(error: ThreadSafeError) -> Self {
        Self::ThreadSafeError(error)
    }
}

#[derive(Debug)]
pub enum BinaryReprRootError {
    FromHexError(FromHexError),
    DecodeError(DecodeError),
    None,
}
impl std::fmt::Display for BinaryReprRootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}
impl From<FromHexError> for BinaryReprRootError {
    fn from(value: FromHexError) -> Self {
        Self::FromHexError(value)
    }
}
impl From<DecodeError> for BinaryReprRootError {
    fn from(value: DecodeError) -> Self {
        Self::DecodeError(value)
    }
}

#[derive(Debug)]
pub struct BinaryReprError {
    pub format: BinaryReprFormat,
    pub value: OsString,
    pub err: BinaryReprRootError,
}
impl std::fmt::Display for BinaryReprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl Error for BinaryReprError {}

impl BinaryReprError {
    pub fn new<T: Into<OsString>>(
        value: T,
        format: BinaryReprFormat,
        err: BinaryReprRootError,
    ) -> Self {
        Self {
            format,
            value: value.into(),
            err,
        }
    }
}
