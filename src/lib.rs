#![forbid(unsafe_code)]
pub mod curl_async;
pub mod error;
pub mod handler;
pub mod hash;
pub mod http_client;
pub mod iter_chunk;

use crate::error::*;
use crate::hash::BinaryReprFormat;
use http_client::{download_files_http11, download_files_http2, file_exists};
pub use http_client::{CheckSum, FileToDl};

#[cfg(feature = "async-std")]
use async_std::path::PathBuf;
use iter_chunk::*;
#[cfg(all(not(feature = "async-std"), feature = "tokio"))]
use std::path::PathBuf;

#[derive(Clone)]
pub struct DownloadFolder {
    path: PathBuf,
    files: Vec<FileToDl>,
    if_not_exists: bool,
}
impl DownloadFolder {
    pub fn new<T: Into<PathBuf>>(path: T, if_not_exists: bool) -> Self {
        let path = path.into();
        DownloadFolder {
            path,
            files: Vec::default(),
            if_not_exists,
        }
    }

    pub async fn add_file(&mut self, mut f: FileToDl) {
        f.target = self.path.join(
            f.target
                .strip_prefix(&self.path)
                .or_else(|_| f.target.strip_prefix("/"))
                .unwrap_or(&f.target),
        );
        if !self.if_not_exists || !file_exists(&f.target).await {
            self.files.push(f);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &FileToDl> {
        self.files.iter()
    }
}

#[derive(Default)]
pub struct DownloadBuilder {
    folders: Vec<DownloadFolder>,
    if_not_exists: bool,
}

impl DownloadBuilder {
    pub fn add_folder(&mut self, f: DownloadFolder) {
        self.folders.push(f);
    }

    pub fn if_not_exists(&mut self) {
        self.if_not_exists = true;
    }

    /*
    pub fn if_exists_overwrite(&mut self) {
        self.if_not_exists = false;
    }
    */

    pub fn folder<T: Into<PathBuf>>(&self, p: T) -> DownloadFolder {
        DownloadFolder::new(p.into(), self.if_not_exists)
    }

    pub fn iter(&self) -> impl Iterator<Item = &FileToDl> {
        self.folders.iter().flat_map(|f| f.iter())
    }

    pub async fn download_http2(&self) -> Result<(), DlError> {
        download_files_http2(&self.iter().cloned().collect::<Vec<_>>()).await?;
        Ok(())
    }

    pub async fn download_http2_by_chunk(&self, chunk_size: usize) -> Result<(), DlError> {
        for chunk_files in self.iter().cloned().by_chunk(chunk_size) {
            download_files_http2(&chunk_files).await?;
        }
        Ok(())
    }

    pub async fn download_http11(&self, chunk_size: usize) -> Result<(), DlError> {
        for chunk_files in self.iter().cloned().by_chunk(chunk_size) {
            download_files_http11(&chunk_files).await?;
        }
        Ok(())
    }
}
