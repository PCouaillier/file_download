mod http11;
mod http2;

pub use http11::DlHttp1Future;
pub use http2::DlHttp2Future;
use std::sync::{Mutex, MutexGuard};

#[inline(always)]
fn unlock<'m, T>(mutex: &'m Mutex<T>) -> MutexGuard<'m, T> {
    match mutex.lock() {
        Ok(e) => e,
        Err(p) => p.into_inner(),
    }
}
