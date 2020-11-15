pub mod error;
pub mod curl_async;

use crate::error::*;
use crate::curl_async::{DlHttp1Future, DlHttp2Future};
use curl::easy::{Easy2, Handler, HttpVersion};
use std::{borrow::Cow, io::Write};
use async_std::path::PathBuf;

#[derive(PartialEq, Debug, Clone)]
pub enum BinaryReprFormat {
    Hex,
    Base64,
}

#[derive(Debug, Clone)]
pub struct BinaryRepr {
    value: Vec<u8>, 
    format: BinaryReprFormat,
}
impl BinaryRepr {
    pub fn new<T: Into<Vec<u8>>>(value: T, format: BinaryReprFormat) -> Self {
        Self {
            value: value.into(),
            format
        }
    }
    pub fn decode(&self) -> Vec<u8> {
        self.value.clone()
    }

    pub fn to_base64(&self) -> String {
        base64::encode(self.value.as_slice())
    }

    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(self.value.len() * 8);
        for n in self.value.iter() {
            for i in 0..8u8 {
                s.push(if 0 < n & (1u8 << i) { '1' } else { '0' });
            }
        }
        s
    }

    pub fn to_string(&self) -> String {
        match self.format {
            BinaryReprFormat::Hex => self.to_hex(),
            BinaryReprFormat::Base64 => self.to_base64(),
        }
    }
}

impl PartialEq<Self> for BinaryRepr {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}

#[derive(Debug)]
pub struct FileCollector {
    path: PathBuf,
    file: Option<std::fs::File>,
}

impl<P: Into<PathBuf>> From<P> for FileCollector {
    fn from(path: P) -> Self {
        Self { path: path.into(), file: None }
    }
}

impl Handler for FileCollector {
    fn write(&mut self, data: &[u8]) -> Result<usize, curl::easy::WriteError> {
        let path = self.path.as_os_str();
        let file = self.file.get_or_insert_with(|| {
            std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(path)
                .expect("file created")
        });
        file.write(data).map_err(|_| curl::easy::WriteError::Pause)
    }
}

impl<'p> From<FileCollector> for Easy2<FileCollector> {
    fn from(c: FileCollector) -> Self {
        Self::new(c)
    }
}   

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

pub struct BinaryCollector(Vec<u8>);
impl Default for BinaryCollector {
    fn default() -> Self {
        Self(Vec::new())
    }
}
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
        if !self.if_not_exists || !f.target.exists().await {
            self.files.push(f);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &FileToDl> {
        self.files.iter()
    }

    pub fn into_iter(&self) -> impl Iterator<Item = FileToDl> {
        self.files.clone().into_iter()
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
    use async_std::fs;

    let target = PathBuf::from(file.target.as_os_str());
    if !target.exists().await || file.check_sum == CheckSum::None {
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

pub struct IterChunck<ITER, ITEM>
where
    ITER: Sized + std::iter::Iterator<Item = ITEM>,
{
    iter: ITER,
    size: usize,
}

impl<ITER, ITEM> IterChunck<ITER, ITEM>
where
    ITER: Sized + std::iter::Iterator<Item = ITEM>,
{
    /// Create a new Batching iterator.
    pub fn new(iter: ITER, size: usize) -> IterChunck<ITER, ITEM> {
        IterChunck { iter, size }
    }
}

impl<ITER, ITEM> Iterator for IterChunck<ITER, ITEM>
where
    ITER: Sized + std::iter::Iterator<Item = ITEM>,
{
    type Item = Vec<ITEM>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mut v = Vec::with_capacity(self.size);
        let mut i = 0usize;
        while i < self.size {
            if let Some(e) = self.iter.next() {
                v.push(e);
            } else if i == 0 {
                return None;
            } else {
                break;
            }
            i += 1;
        }
        Some(v)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // No information about closue behavior
        (0, None)
    }
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

    pub fn into_iter(&self) -> impl Iterator<Item = FileToDl> {
        self.folders.clone().into_iter().map(|f| f.into_iter()).flatten()
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

    pub async fn download_http2(&self) -> Result<(), DlError> {
        for chunk_files in IterChunck::new(self.into_iter(), 16) {
            // dl_tokens must be droped after Multi::perform
            let multi = curl::multi::Multi::new();
            let mut dl_tokens = Vec::with_capacity(chunk_files.len());
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
        }
        self.check_hashes().await?;
        Ok(())
    }

    pub async fn download_http11(&self) -> Result<(), DlError> {
        use futures::future::try_join_all;

        try_join_all(self.into_iter().map(|file| async move {
            (DlHttp1Future::new(move ||download_file_http11(file).map_err(CurlError::from))).await
                .map_err(CurlError::from)
        }))
        .await?;

        &self.check_hashes().await?;
        Ok(())
    }
}



#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
