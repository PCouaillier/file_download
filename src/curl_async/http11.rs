use crate::error::*;
use curl::easy::{Easy2, Handler};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pub(super) enum DlHttp1FutureState<H: Handler> {
    Pending(std::thread::JoinHandle<Result<Easy2<H>, CurlError>>),
    Done,
}

fn wrap_curl_error<E: std::fmt::Debug>(err: E) -> CurlError {
    CurlError::ThreadSafeError(ThreadSafeError::from(format!(
        "curl error occured {:?}",
        err
    )))
}

pub struct DlHttp1Future<H: Handler> {
    state: DlHttp1FutureState<H>,
}

impl<H: Handler + Send + 'static> DlHttp1Future<H> {
    pub fn new<F: Send + 'static + FnOnce() -> Result<Easy2<H>, CurlError>>(f: F) -> Self {
        let state = DlHttp1FutureState::Pending(std::thread::spawn(move || {
            f().and_then(|easy| match easy.perform() {
                Ok(_) => Ok(easy),
                Err(err) => Err(wrap_curl_error(err)),
            })
        }));
        Self { state }
    }
}

impl<T: Handler> Future for DlHttp1Future<T> {
    type Output = Result<Easy2<T>, CurlError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        use std::thread;
        use std::time::Duration;
        let unpin_self = self.get_mut();

        let is_finished = {
            if let DlHttp1FutureState::Pending(handle) = &unpin_self.state {
                handle.is_finished()
            } else {
                false
            }
        };
        if is_finished {
            let mut state = DlHttp1FutureState::Done;
            std::mem::swap(&mut unpin_self.state, &mut state);
            if let DlHttp1FutureState::Pending(bg_task) = state {
                let val = bg_task.join().map_err(wrap_curl_error).and_then(|a| a);
                Poll::Ready(val)
            } else {
                Poll::Ready(Err(ThreadSafeError::from("value is gone").into()))
            }
        } else {
            match unpin_self.state {
                DlHttp1FutureState::Pending(_) => {
                    let cx2 = cx.waker().clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(1));
                        cx2.wake();
                    });
                    Poll::Pending
                }
                DlHttp1FutureState::Done => {
                    Poll::Ready(Err(ThreadSafeError::from("value is gone").into()))
                }
            }
        }
    }
}
