use crate::error::*;
use curl::{
    easy::Handler,
    multi::{Easy2Handle, Multi},
};
use std::{
    fmt::Debug,
    future::Future,
    mem,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

#[derive(Debug)]
enum DlHttp2FutureState<'files, T: Handler + std::fmt::Debug> {
    Pending,
    Done(&'files [Easy2Handle<T>]),
    Error(Arc<CurlError>),
}

/// Internal of http2 this is used to lock the
/// wole
///
///
#[derive(Debug)]
struct DlHttp2FutureInner<'files, T: Handler + std::fmt::Debug> {
    pub files: Option<&'files [Easy2Handle<T>]>,
    pub multi: Option<curl::multi::Multi>,
    pub state: DlHttp2FutureState<'files, T>,
    pub join: Option<std::thread::JoinHandle<()>>,
}

impl<'files, T: Handler + std::fmt::Debug> DlHttp2FutureInner<'files, T> {
    #[cfg_attr(feature = "tracing", tracing::instrument)]
    fn poll_multi(&mut self) {
        if let DlHttp2FutureState::Pending = self.state {
            if self.files.map(|a| a.is_empty()).unwrap_or(true) {
                let mut files = None;
                mem::swap(&mut files, &mut self.files);
                self.state = DlHttp2FutureState::Done(files.unwrap());
                let mut multi = None;
                mem::swap(&mut self.multi, &mut multi);
                drop(multi);
                return;
            }
            if let Some(multi) = &mut self.multi {
                match multi.perform() {
                    Ok(0) => {
                        let mut files = None;
                        mem::swap(&mut files, &mut self.files);
                        self.state = DlHttp2FutureState::Done(files.unwrap());
                        let mut multi = None;
                        mem::swap(&mut self.multi, &mut multi);
                        drop(multi);
                    }
                    Err(error) => {
                        self.state = DlHttp2FutureState::Error(Arc::new(error.into()));
                        let mut multi = None;
                        mem::swap(&mut self.multi, &mut multi);
                        drop(multi);
                    }
                    _ => {}
                }
            }
        }
    }

    fn poll(&mut self, cx: &mut Context) -> Poll<Result<&'files [Easy2Handle<T>], Arc<CurlError>>> {
        if let DlHttp2FutureState::Pending = self.state {
            self.poll_multi();
        }
        match &self.state {
            DlHttp2FutureState::Done(files) => Poll::Ready(Ok(<&[Easy2Handle<T>]>::clone(files))),
            DlHttp2FutureState::Error(error) => Poll::Ready(Err(error.clone())),
            _ => {
                let ct = cx.waker().clone();
                self.join = Some(std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(10));
                    ct.wake();
                }));
                Poll::Pending
            }
        }
    }
}

pub struct DlHttp2Future<'files, T: Handler + std::fmt::Debug> {
    inner: DlHttp2FutureInner<'files, T>,
}

impl<'files, T: Handler + Debug> std::fmt::Debug for DlHttp2Future<'files, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DlHttp2Future")
            .field(
                "dbg_files_len",
                &self.inner.files.map(|a| a.len()).unwrap_or(0),
            )
            .field("state", &self.inner.state)
            .finish()
    }
}

impl<'files, T: Handler + std::fmt::Debug> DlHttp2Future<'files, T> {
    pub fn new(files: &'files [Easy2Handle<T>], multi: Multi) -> Self {
        if files.is_empty() {
            drop(multi);
            return Self {
                inner: DlHttp2FutureInner {
                    files: None,
                    multi: None,
                    state: DlHttp2FutureState::Done(files),
                    join: None,
                },
            };
        }

        Self {
            inner: DlHttp2FutureInner {
                state: DlHttp2FutureState::Pending,
                files: Some(files),
                multi: Some(multi),
                join: None,
            },
        }
    }
}

impl<'files, T: Handler + std::fmt::Debug> Future for DlHttp2Future<'files, T> {
    type Output = Result<&'files [Easy2Handle<T>], Arc<CurlError>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.get_mut().inner.poll(cx)
    }
}
