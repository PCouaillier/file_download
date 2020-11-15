use std::{error::Error, fmt::{Debug, Display, Formatter, Result as FmtResult}};

#[derive(Debug)]
pub struct BadCheckSumError {
    file_sources: Vec<(String, String)>,
}
impl Display for BadCheckSumError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(self, f)
    }
}
impl Error for BadCheckSumError {}

impl From<Vec<(String, String)>> for BadCheckSumError {
    fn from(file_sources: Vec<(String, String)>) -> Self {
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
    ThreadSafeError(ThreadSafeError)
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
impl Into<ThreadSafeError> for CurlError {
    fn into(self) -> ThreadSafeError {
        ThreadSafeError { message: format!("{:?}", self) }
    }
}

#[derive(Debug)]
pub enum DlError {
    BadCheckSumError(BadCheckSumError),
    CurlError(CurlError)
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
