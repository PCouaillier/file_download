use curl::easy::{self, Easy2, Handler};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug)]
pub struct FileCollector {
    path: PathBuf,
    file: Option<File>,
}

impl<P: Into<PathBuf>> From<P> for FileCollector {
    fn from(path: P) -> Self {
        Self {
            path: path.into(),
            file: None,
        }
    }
}

impl Handler for FileCollector {
    fn write(&mut self, data: &[u8]) -> Result<usize, easy::WriteError> {
        let path = self.path.as_os_str();
        let file = self.file.get_or_insert_with(|| {
            fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(path)
                .expect("file created")
        });
        file.write(data).map_err(|_| easy::WriteError::Pause)
    }
}

impl From<FileCollector> for Easy2<FileCollector> {
    fn from(c: FileCollector) -> Self {
        Self::new(c)
    }
}
