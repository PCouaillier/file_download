#![forbid(unsafe_code)]
pub mod curl_async;
pub mod error;
pub mod handler;
pub mod hash;
pub mod iter_chunk;

use crate::curl_async::{DlHttp1Future, DlHttp2Future};
use crate::error::*;
use crate::handler::FileCollector;
use crate::hash::{BinaryRepr, BinaryReprFormat, BASE64_ENGINE};
use base64::Engine as _;

#[cfg(feature = "async-std")]
use async_std::{
    fs, io,
    path::{Path, PathBuf},
};
use curl::easy::{Easy2, HttpVersion};
use futures::future::{join_all, try_join_all};
#[cfg(feature = "async-std")]
use futures::{io::AsyncBufReadExt, AsyncBufRead};
use iter_chunk::*;
#[cfg(all(not(feature = "async-std"), feature = "tokio"))]
use std::path::{Path, PathBuf};
#[cfg(all(not(feature = "async-std"), feature = "tokio"))]
use tokio::{
    fs,
    io::{self, AsyncBufReadExt},
};

async fn md5_hash_check_file(
    expected_hash: &BinaryRepr,
    file_path: &Path,
) -> Result<(), CheckHashError> {
    let f = fs::File::open(file_path).await?;
    // Find the length of the file
    let len = f.metadata().await?.len();
    // Decide on a reasonable buffer size (1MB in this case, fastest will depend on hardware)
    let buf_len = len.min(1_000_000) as usize;
    let mut buf = io::BufReader::with_capacity(buf_len, f);
    let mut context = md5::Context::new();
    loop {
        // Get a chunk of the file
        let part = buf.fill_buf().await?;
        // If that chunk was empty, the reader has reached EOF
        if part.is_empty() {
            break;
        }
        // Add chunk to the md5
        context.consume(part);
        // Tell the buffer that the chunk is consumed
        let part_len = part.len();
        std::pin::Pin::new(&mut buf).consume(part_len);
    }
    let digest_b64 = BASE64_ENGINE.encode(context.compute().as_ref());
    let expected_hash_b64 = expected_hash.to_base64();
    if digest_b64 == expected_hash_b64 {
        return Ok(());
    }
    return Err(CheckHashError::HashError(BadCheckSumErrorDetail {
        url: file_path.to_string_lossy().to_string(),
        expected_hash: expected_hash_b64,
        current_hash: digest_b64,
    }));
}

#[derive(Debug, PartialEq, Clone)]
pub enum CheckSum {
    None,
    Md5(BinaryRepr),
}

impl CheckSum {
    pub async fn do_file_matches_checksum(&self, file_path: &Path) -> Result<(), CheckHashError> {
        match self {
            Self::None => Ok(()),
            Self::Md5(expected_hash) => md5_hash_check_file(expected_hash, file_path).await,
        }
    }
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
    return fs::metadata(path).await.is_ok();
}

async fn download_files_http11_curl(chunk: Vec<FileToDl>) -> Result<(), DlError> {
    try_join_all(chunk.into_iter().map(|file| async move {
        (DlHttp1Future::new(move || download_file_http11(&file).map_err(CurlError::from)))
            .await
            .map_err(CurlError::from)
    }))
    .await?;
    Ok(())
}

async fn check_hash_and_rename(files: (&FileToDl, &FileToDl)) -> Result<(), CheckHashError> {
    let (tmp_file, file) = files;
    if let Err(err) = check_file_checksum(tmp_file).await {
        Err(err)
    } else {
        fs::rename(&tmp_file.target, &file.target)
            .await
            .map_err(CheckHashError::IoError)
    }
}

async fn download_files_http2_curl(files: &Vec<FileToDl>) -> Result<(), DlError> {
    let mut dl_tokens = Vec::with_capacity(files.len());
    let multi = curl::multi::Multi::new();
    for file in files.iter() {
        dl_tokens.push(multi.add2(download_file_http2_curl(file)?)?);
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

fn download_file_http2_curl(file: &FileToDl) -> Result<Easy2<FileCollector>, curl::Error> {
    let version = if file.source.starts_with("https:") {
        HttpVersion::V2TLS
    } else {
        HttpVersion::V2
    };
    let mut easy = download_file_http11(file)?;
    easy.http_version(version)?;
    Ok(easy)
}

async fn download_files_http11(files: &[FileToDl]) -> Result<(), DlError> {
    let tmp_files = generate_tmp_files(files.iter());

    download_files_http11_curl(tmp_files.clone()).await?;
    let results = join_all(
        tmp_files
            .iter()
            .zip(files.iter())
            .map(check_hash_and_rename),
    )
    .await;
    let mut bad_check: Vec<BadCheckSumErrorDetail> = Vec::new();
    for result in results
        .into_iter()
        .filter(Result::is_err)
        .map(Result::unwrap_err)
    {
        match result {
            CheckHashError::IoError(err) => return Err(DlError::from(err)),
            CheckHashError::HashError(err) => bad_check.push(err),
        }
    }
    if !bad_check.is_empty() {
        return Err(DlError::from(BadCheckSumError::from(bad_check)));
    }
    Ok(())
}

fn generate_tmp_files<'a>(files: impl Iterator<Item = &'a FileToDl>) -> Vec<FileToDl> {
    files
        .map(|f| {
            let mut tmp_target = f.target.clone();
            let mut ext = tmp_target.extension().unwrap_or_default().to_owned();
            ext.push(".tmp");
            tmp_target.set_extension(ext);
            FileToDl {
                source: f.source.clone(),
                target: tmp_target,
                check_sum: f.check_sum.clone(),
            }
        })
        .collect()
}

async fn download_files_http2(files: &[FileToDl]) -> Result<(), DlError> {
    let tmp_files = generate_tmp_files(files.iter());
    download_files_http2_curl(&tmp_files).await?;
    let results = join_all(
        tmp_files
            .iter()
            .zip(files.iter())
            .map(check_hash_and_rename),
    )
    .await;

    let mut bad_check: Vec<BadCheckSumErrorDetail> = Vec::new();
    for result in results
        .into_iter()
        .filter(Result::is_err)
        .map(Result::unwrap_err)
    {
        match result {
            CheckHashError::IoError(err) => return Err(DlError::from(err)),
            CheckHashError::HashError(err) => bad_check.push(err),
        }
    }
    if !bad_check.is_empty() {
        return Err(DlError::from(BadCheckSumError::from(bad_check)));
    }

    Ok(())
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

fn download_file_http11(file: &FileToDl) -> Result<Easy2<FileCollector>, curl::Error> {
    let mut easy: Easy2<_> = FileCollector::from(&file.target).into();
    easy.url(&file.source)?;
    easy.get(true)?;
    easy.max_redirections(3)?;

    Ok(easy)
}

async fn check_file_checksum(file: &FileToDl) -> Result<(), CheckHashError> {
    let target = PathBuf::from(file.target.as_os_str());
    if !file_exists(&target).await {
        return Ok(());
    }
    file.check_sum
        .do_file_matches_checksum(&target)
        .await
        .map_err(|err| match err {
            CheckHashError::IoError(_) => err,
            CheckHashError::HashError(detail) => {
                CheckHashError::HashError(BadCheckSumErrorDetail {
                    url: file.source.clone(),
                    expected_hash: detail.expected_hash,
                    current_hash: detail.current_hash,
                })
            }
        })
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
