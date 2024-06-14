use crate::error::*;
use curl::easy::{Easy2, Handler};
use std::thread;
use std::time::Duration;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pub type Easy2Builder<H> = Box<dyn Send + 'static + FnOnce() -> Result<Easy2<H>, CurlError>>;

enum DlHttp1FutureState<H: Handler> {
    NotStarted(Easy2Builder<H>),
    Pending(std::thread::JoinHandle<Result<Easy2<H>, ThreadSafeError>>),
    Done,
}
impl <H: Handler> std::fmt::Debug for DlHttp1FutureState<H> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(fmt, "DlHttp1FutureState({})", match self {
            Self::NotStarted(_) => "NotStarted",
            Self::Pending(_) => "Pending",
            Self::Done => "Done",
        })
    }
}

#[derive(Debug)]
pub struct DlHttp1Future<H: Handler> {
    state: DlHttp1FutureState<H>,
    waker: Option<std::thread::JoinHandle<()>>,
}

impl<H: Handler + Send + 'static> DlHttp1Future<H> {
    pub fn new<F: Send + 'static + FnOnce() -> Result<Easy2<H>, CurlError>>(
        easy_builder: F,
    ) -> Self {
        Self {
            state: DlHttp1FutureState::NotStarted(Box::new(easy_builder)),
            waker: None,
        }
    }
}

impl<H: Handler + Send + 'static + std::fmt::Debug> Future for DlHttp1Future<H> {
    type Output = Result<Easy2<H>, CurlError>;

    #[cfg_attr(feature = "tracing", tracing::instrument(level = "debug"))]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let self_m = self.get_mut();

        if matches!(&self_m.state, DlHttp1FutureState::NotStarted(_)) {
            // I need to take ownership of easy_builder so i temporarly put the Done state...
            let mut state = DlHttp1FutureState::Done;
            std::mem::swap(&mut self_m.state, &mut state);

            // This may lead to a panic if poll is called now

            if let DlHttp1FutureState::NotStarted(easy_builder) = state {
                let cx2 = cx.waker().clone();
                let mut state = DlHttp1FutureState::Pending(std::thread::spawn(move || {
                    easy_builder()
                        .and_then(|easy| match easy.perform() {
                            Ok(_) => Ok(easy),
                            Err(e) => Err(e.into()),
                        })
                        .map_err(|err| ThreadSafeError::from(format!("curl error occured {}", err)))
                        .map(move |easy| {
                            cx2.wake();
                            easy
                        })
                }));
                std::mem::swap(&mut self_m.state, &mut state);
                // We are back in a valid state
                return Poll::Pending;
            } else {
                panic!("bad state")
            }
        }

        let is_pending = match &self_m.state {
            DlHttp1FutureState::Pending(thread) => !thread.is_finished(),
            _ => panic!("bad state"),
        };
        if is_pending {
            // this branch is only if the promise is waken before thread is properly marked finished
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
                // this branch calls thread.join() wich is non-blocking on completed threads
                DlHttp1FutureState::Pending(thread) => {
                    Poll::Ready(thread.join().expect("join").map_err(CurlError::from))
                }
                _ => panic!("bad state"),
            }
        }
    }
}
