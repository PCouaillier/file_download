#![forbid(unsafe_code)]
pub mod curl_async;
pub mod error;
pub mod handler;
pub mod hash;
pub mod iter_chunk;

use crate::curl_async::{DlHttp1Future, DlHttp2Future};
use crate::error::*;
use crate::handler::FileCollector;
use crate::hash::{BinaryRepr, BinaryReprFormat};
#[cfg(feature = "async-std")]
use async_std::{fs, path::{Path,PathBuf}};
#[cfg(all(not(feature = "async-std"), feature = "tokio"))]
use std::path::{Path,PathBuf};
#[cfg(all(not(feature = "async-std"), feature = "tokio"))]
use tokio::fs;
use curl::easy::{Easy2, HttpVersion};
use iter_chunk::*;

#[derive(Debug, PartialEq, Clone)]
pub enum CheckSum {
    None,
    Md5(BinaryRepr),
}

#[derive(Debug, Clone)]
pub struct FileToDl {
    pub target: PathBuf,
    pub source: String,
    pub check_sum: CheckSum,
}

async fn file_exists(path: &Path) -> bool {
    #[cfg(feature = "async-std")]
    return path.exists().await;
    #[cfg(all(not(feature = "async-std"), feature = "tokio"))]
    return fs::metadata(path).await.is_ok()
}

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

fn download_file_http11(file: FileToDl) -> Result<Easy2<FileCollector>, curl::Error> {
    let mut easy: Easy2<_> = FileCollector::from(file.target).into();
    easy.url(&file.source)?;
    easy.get(true)?;
    easy.max_redirections(3)?;

    Ok(easy)
}

fn download_file_http2(file: FileToDl) -> Result<Easy2<FileCollector>, curl::Error> {
    let version = if file.source.starts_with("https:") {
        HttpVersion::V2TLS
    } else {
        HttpVersion::V2
    };
    let mut easy = download_file_http11(file)?;
    easy.http_version(version)?;
    Ok(easy)
}

async fn check_file_checksum(file: &FileToDl) -> Result<(), (String, String)> {
    let target = PathBuf::from(file.target.as_os_str());
    if !file_exists(&target).await || file.check_sum == CheckSum::None {
        return Ok(());
    }
    let f_md5 = match &file.check_sum {
        CheckSum::None => return Ok(()),
        CheckSum::Md5(f_md5) => f_md5.to_base64(),
    };
    if let Ok(content) = fs::read(&target).await {
        let digest = base64::encode(*md5::compute(content));
        if f_md5 != digest {
            return Err((file.source.clone(), f_md5));
        }
    }
    Ok(())
}

pub struct DownloadBuilder {
    folders: Vec<DownloadFolder>,
    if_not_exists: bool,
}

impl Default for DownloadBuilder {
    fn default() -> Self {
        Self {
            folders: Vec::default(),
            if_not_exists: false,
        }
    }
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
        self.folders.iter().map(|f| f.iter()).flatten()
    }

    async fn check_hashes(&self) -> Result<(), BadCheckSumError> {
        let errors = futures::future::join_all(self.iter().map(check_file_checksum))
            .await
            .into_iter()
            .filter_map(|e| if let Err(err) = e { Some(err) } else { None })
            .collect::<Vec<_>>();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.into())
        }
    }

    async fn download_files(chunk_files: Vec<FileToDl>) -> Result<(), DlError> {
        // dl_tokens must be droped after Multi::perform
        let mut dl_tokens = Vec::with_capacity(chunk_files.len());
        let multi = curl::multi::Multi::new();
        for file in chunk_files.into_iter() {
            dl_tokens.push(multi.add2(download_file_http2(file)?)?);
        }
        if !dl_tokens.is_empty() {
            DlHttp2Future::new(dl_tokens.as_slice(), multi)
                .await
                .map_err(|_| {
                    CurlError::from(ThreadSafeError {
                        message: "http2 error".to_owned(),
                    })
                })?;
        }
        Ok(())
    }

    pub async fn download_http2(&self) -> Result<(), DlError> {
        Self::download_files(self.iter().cloned().collect()).await?;
        self.check_hashes().await?;
        Ok(())
    }

    pub async fn download_http2_by_chunk(&self, chunk_size: usize) -> Result<(), DlError> {
        for chunk_files in self.iter().cloned().by_chunk(chunk_size) {
            Self::download_files(chunk_files).await?;
        }
        self.check_hashes().await?;
        Ok(())
    }

    pub async fn download_http11(&self, chunk_size: usize) -> Result<(), DlError> {
        use futures::future::try_join_all;

        for chunk_files in self.iter().cloned().by_chunk(chunk_size) {
            try_join_all(chunk_files.into_iter().map(|file| async move {
                (DlHttp1Future::new(move || download_file_http11(file).map_err(CurlError::from)))
                    .await
                    .map_err(CurlError::from)
            }))
            .await?;
        }

        self.check_hashes().await?;
        Ok(())
    }
}
