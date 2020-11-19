use super::unlock;
use crate::error::*;
use curl::easy::{Easy2, Handler};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

enum DlHttp1FutureState<H: Handler> {
    NotStarted,
    Pending(std::thread::JoinHandle<()>),
    Done(Option<Easy2<H>>),
    Error(ThreadSafeError),
}

pub struct DlHttp1Future<T: Handler> {
    state: Arc<Mutex<DlHttp1FutureState<T>>>,
    waker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl<H: Handler + Send + 'static> DlHttp1Future<H> {
    pub fn new<F: Send + 'static + FnOnce() -> Result<Easy2<H>, CurlError>>(f: F) -> Self {
        let state = Arc::new(Mutex::new(DlHttp1FutureState::NotStarted));
        let thread_state = state.clone();
        *(unlock(&state)) = DlHttp1FutureState::Pending(std::thread::spawn(move || {
            let result = match f().and_then(|easy| match easy.perform() {
                Ok(_) => Ok(Some(easy)),
                Err(e) => Err(e.into()),
            }) {
                Ok(ok) => DlHttp1FutureState::Done(ok),
                Err(_) => DlHttp1FutureState::Error(ThreadSafeError::from("curl error occured")),
            };
            *(unlock(&thread_state)) = result;
        }));
        Self {
            state: state,
            waker: Mutex::new(None),
        }
    }
}

impl<T: Handler> Future for DlHttp1Future<T> {
    type Output = Result<Easy2<T>, CurlError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        use std::thread;
        use std::time::Duration;

        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        match &mut (*state) {
            DlHttp1FutureState::Pending(_) | DlHttp1FutureState::NotStarted => {
                let cx2 = cx.waker().clone();
                *(self.waker.lock().expect("locking waker")) = Some(thread::spawn(move || {
                    let _ = thread::sleep(Duration::from_secs(1));
                    cx2.wake();
                }));
                Poll::Pending
            }
            DlHttp1FutureState::Done(ok) if ok.is_some() => {
                let mut ret = None;
                std::mem::swap(ok, &mut ret);
                Poll::Ready(Ok(ret.unwrap()))
            }
            DlHttp1FutureState::Done(_) => {
                Poll::Ready(Err(ThreadSafeError::from("value is gone").into()))
            }
            DlHttp1FutureState::Error(err) => Poll::Ready(Err(err.clone().into())),
        }
    }
}
