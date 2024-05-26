use crate::error::*;
use curl::easy::{Easy2, Handler};
use std::thread;
use std::time::Duration;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

enum DlHttp1FutureState<H: Handler> {
    Pending(std::thread::JoinHandle<Result<Easy2<H>, ThreadSafeError>>),
    Done,
}

pub struct DlHttp1Future<T: Handler> {
    state: DlHttp1FutureState<T>,
    waker: Option<std::thread::JoinHandle<()>>,
}

impl<H: Handler + Send + 'static> DlHttp1Future<H> {
    pub fn new<F: Send + 'static + FnOnce() -> Result<Easy2<H>, CurlError>>(f: F) -> Self {
        let state = DlHttp1FutureState::Pending(std::thread::spawn(move || {
            f().and_then(|easy| match easy.perform() {
                Ok(_) => Ok(easy),
                Err(e) => Err(e.into()),
            })
            .map_err(|err| ThreadSafeError::from(format!("curl error occured {}", err)))
        }));
        Self { state, waker: None }
    }
}

impl<T: Handler> Future for DlHttp1Future<T> {
    type Output = Result<Easy2<T>, CurlError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let self_m = self.get_mut();

        let is_pending = match &self_m.state {
            DlHttp1FutureState::Pending(thread) => !thread.is_finished(),
            DlHttp1FutureState::Done => {
                return Poll::Ready(Err(ThreadSafeError::from("Value is gone").into()))
            }
        };
        if is_pending {
            let cx2 = cx.waker().clone();
            let mut tmp = Some(thread::spawn(move || {
                thread::sleep(Duration::from_secs(1));
                cx2.wake();
            }));
            std::mem::swap(&mut self_m.waker, &mut tmp);
            Poll::Pending
        } else {
            let mut done = DlHttp1FutureState::Done;
            std::mem::swap(&mut self_m.state, &mut done);
            match done {
                DlHttp1FutureState::Pending(thread) => {
                    Poll::Ready(thread.join().expect("join").map_err(CurlError::from))
                }
                DlHttp1FutureState::Done => {
                    Poll::Ready(Err(ThreadSafeError::from("Value is gone").into()))
                }
            }
        }
    }
}
