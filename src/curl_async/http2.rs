use super::unlock;
use crate::error::*;
use curl::{
    easy::Handler,
    multi::{Easy2Handle, Multi},
};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

#[derive(Debug)]
enum DlHttp2FutureState {
    Pending,
    Done,
    Error(Arc<CurlError>),
}

/// Internal of http2 this is used to lock the
/// wole
///
///
struct DlHttp2FutureInner<'files, T: Handler> {
    pub files: &'files [Easy2Handle<T>],
    pub multi: Option<curl::multi::Multi>,
    pub state: DlHttp2FutureState,
    pub join: Option<std::thread::JoinHandle<()>>,
}

type FutureResult = Result<(), Arc<CurlError>>;

impl<'files, T: Handler> DlHttp2FutureInner<'files, T> {
    fn poll_multi(&mut self) {
        if let DlHttp2FutureState::Pending = self.state {
            if self.files.is_empty() {
                self.state = DlHttp2FutureState::Done;
                let mut multi = None;
                std::mem::swap(&mut self.multi, &mut multi);
                drop(multi);
                return;
            }
            if let Some(multi) = &mut self.multi {
                match multi.perform() {
                    Ok(bytes) if bytes == 0 => {
                        self.state = DlHttp2FutureState::Done;
                        let mut multi = None;
                        std::mem::swap(&mut self.multi, &mut multi);
                        drop(multi);
                    }
                    Err(error) => {
                        self.state = DlHttp2FutureState::Error(Arc::new(error.into()));
                        let mut multi = None;
                        std::mem::swap(&mut self.multi, &mut multi);
                        drop(multi);
                    }
                    _ => {}
                }
            }
        }
    }

    fn poll(&mut self, cx: &mut Context) -> Poll<FutureResult> {
        if let DlHttp2FutureState::Pending = self.state {
            self.poll_multi();
        }
        match &self.state {
            DlHttp2FutureState::Done => Poll::Ready(Ok(())),
            DlHttp2FutureState::Error(error) => Poll::Ready(Err(error.clone())),
            _ => {
                let ct = cx.waker().clone();
                self.join = Some(std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    ct.wake();
                }));
                Poll::Pending
            }
        }
    }
}

pub struct DlHttp2Future<'files, T: Handler> {
    inner: Mutex<DlHttp2FutureInner<'files, T>>,
}

impl<'files, T: Handler> std::fmt::Debug for DlHttp2Future<'files, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = match self.inner.lock() {
            Ok(a) => a,
            Err(p) => p.into_inner(),
        };
        f.debug_struct("DlHttp2Future")
            .field("dbg_files_len", &inner.files.len())
            .field("state", &inner.state)
            .finish()
    }
}

impl<'files, T: Handler> DlHttp2Future<'files, T> {
    pub fn new(files: &'files [Easy2Handle<T>], multi: Multi) -> Self {
        if files.is_empty() {
            drop(multi);
            return Self {
                inner: Mutex::new(DlHttp2FutureInner {
                    files,
                    multi: None,
                    state: DlHttp2FutureState::Done,
                    join: None,
                }),
            };
        }

        Self {
            inner: Mutex::new(DlHttp2FutureInner {
                state: DlHttp2FutureState::Pending,
                files,
                multi: Some(multi),
                join: None,
            }),
        }
    }
}

impl<'files, T: Handler> Future for DlHttp2Future<'files, T> {
    type Output = FutureResult;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        unlock(&self.inner).poll(cx)
    }
}
