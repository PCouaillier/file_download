use crate::error::*;
use curl::{
    easy::Handler,
    multi::{Easy2Handle, Multi},
};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

#[derive(Debug)]
enum DlHttp2FutureState {
    Pending,
    Done,
    Error(Arc<CurlError>),
}

pub struct DlHttp2Future<'files, T: Handler> {
    files: &'files [Easy2Handle<T>],
    multi: Option<curl::multi::Multi>,
    state: DlHttp2FutureState,
    join: Option<std::thread::JoinHandle<()>>,
}
impl<'files, T: Handler> std::fmt::Debug for DlHttp2Future<'files, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DlHttp2Future")
            .field("dbg_files_len", &self.files.len())
            .field("state", &self.state)
            .finish()
    }
}

impl<'files, T: Handler> DlHttp2Future<'files, T> {
    pub fn new(files: &'files [Easy2Handle<T>], multi: Multi) -> Self {
        if files.is_empty() {
            drop(multi);
            return Self {
                files,
                multi: None,
                state: DlHttp2FutureState::Done,
                join: None,
            };
        }

        Self {
            state: DlHttp2FutureState::Pending,
            files,
            multi: Some(multi),
            join: None,
        }
    }

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
}

impl<'files, T: Handler> Future for DlHttp2Future<'files, T> {
    type Output = Result<(), Arc<CurlError>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let fu: &mut Self = unsafe { self.get_unchecked_mut() };

        if let DlHttp2FutureState::Pending = &fu.state {
            fu.poll_multi();
        }
        match &fu.state {
            DlHttp2FutureState::Done => Poll::Ready(Ok(())),
            DlHttp2FutureState::Error(error) => Poll::Ready(Err(error.clone())),
            _ => {
                let ct = cx.waker().clone();
                fu.join = Some(std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    ct.wake();
                }));
                Poll::Pending
            }
        }
    }
}
