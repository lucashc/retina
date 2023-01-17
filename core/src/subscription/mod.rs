//! Subscribable data types.
//!
//! A subscription is a request for a callback.
//! Callback functions are implemented as a closure that takes a subscribable data type as the
//! parameter and immutably borrows values from the environment. Built-in subscribable types can
//! be customized within the framework to provide additional data to the callback if needed.

pub mod zc_frame;

pub use self::zc_frame::ZcFrame;

use crate::{filter::FilterCtx, memory::mbuf::Mbuf};

#[cfg(feature = "timing")]
use crate::timing::timer::Timers;

/// Represents a generic subscribable type. All subscribable types must implement this trait.
pub trait Subscribable {
    /// Process a single incoming packet.
    fn process_packet(mbuf: Mbuf, filter_ctx: &FilterCtx, subscription: &Subscription<Self>)
    where
        Self: Sized;
}

/// A request for a callback on a subset of traffic specified by the filter.
#[doc(hidden)]
pub struct Subscription<'a, S>
where
    S: Subscribable,
{
    callback: Box<dyn Fn(S, &FilterCtx) + 'a>,
    #[cfg(feature = "timing")]
    pub(crate) timers: Timers,
}

impl<'a, S> Subscription<'a, S>
where
    S: Subscribable,
{
    /// Creates a new subscription from a filter and a callback.
    pub(crate) fn new(cb: impl Fn(S, &FilterCtx) + 'a) -> Self {
        Subscription {
            callback: Box::new(cb),
            #[cfg(feature = "timing")]
            timers: Timers::new(),
        }
    }

    /// Invoke the callback on `S`.
    pub(crate) fn invoke(&self, obj: S, filter_ctx: &FilterCtx) {
        tsc_start!(t0);
        (self.callback)(obj, filter_ctx);
        tsc_record!(self.timers, "callback", t0);
    }
}
