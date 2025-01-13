//! # Utilities for Composing Futures
//!
//! This module provides a lower-level wrapper for manually implemented futures. It is intended for library
//! authors that already have a hand-implemented future, that needs to offload compute-heavy work to vacation.
//!
//! Such a use case cannot simply call [`vacation::execute(_)`] and await the returned future
//! because they are already in a sync context. Instead, the offloaded work needs to be driven
//! across the individual polls within the custom future's current await handling.
//!
//! The entrypoint for this api is [`vacation::future::builder()`], which allows constructing an [`OffloadWith`]
//! to wrap your custom inner future.
//!
//! This wrapper future processes occasional work offloaded from the inner future, while driving the inner
//! future if no offloaded work is active. Offloaded work is retrieved via a 'pull' model, where,
//! after the inner future returns `Poll::Pending`, a [`get_offload()`] closure is called that has
//! owned access to the inner future. That closure can return any work that needs to be offloaded,
//! structured as an async future that invokes `vacation::execute()` and processes any results.
//!
//! **Note that `OffloadWith` does NOT poll the inner future while offloaded work is active, so there is a deadlock
//! risk if the offloaded work depends on the inner future making progress to resolve.**
//!
//! ## Q&A
//!
//! ### This seems complicated, why not just call vacation from inside my custom future?
//! There are many ways to structure a custom future to efficiently use `vacation`. This is designed
//! to be more of a 'bolt on' utility that requires minimal code changes inside your inner future -
//! mostly just adding a hook with which to get work.
//!
//! If you are writing a custom future with `vacation` in mind, it probably is simplest to store the state
//! of any offloaded work directly in your future, and poll it as part of your `Future` implementation.
//! That way you can simply initialize that work whenever you reach a point that needs it, poll it once, and either
//! return `Poll::Pending` (if you need to wait on that work completing) or carry on (if you can make other progress
//! while the offloaded work is ongoing).
//!
//! ### I only need to send work to vacation once, in order to construct my inner future. What should I do?
//! This can be handled very simply with [`futures_util::then()`]. Simply call `vacation::execute()` first,
//! and then chain `then()` to construct the inner future.
//!
//! Here is an example:
//!
//! ```
//! use futures_util::FutureExt;
//!
//! let vacation_future = vacation::execute(
//!     || {
//!         // stand-in for compute-heavy work
//!         std::thread::sleep(std::time::Duration::from_millis(50));
//!         5
//!     },
//!     vacation::ExecuteContext::new(vacation::ChanceOfBlocking::Frequent)
//! );
//!
//! let future = vacation_future.then(|res| async move {
//!     match res {
//!         Err(error) => Err("there was a vacation error"),
//!         // stand-in for your custom future
//!         Ok(res) => Ok(tokio::time::sleep(std::time::Duration::from_millis(res)).await),
//!     }
//! });
//!
//! ```
//!
//! ### This seems suspicious, what if the inner future has a waker fire while it is being suppressed
//! due to ongoing offloaded work?
//! We defensively poll the inner future anytime the offloaded work completes. This means that if any
//! waker had fired while that offloaded work was ongoing, the inner future will definitely be polled again.
//!
//! Essentially, our guarantee is, every time the wrapper future is polled, either the inner future is polled
//! directly (if no offloaded work), it is immediately polled after offloaded work (if offloaded work just completed),
//! or there is at minimum a waker registered for the offloaded work- which, again, will result in the inner future
//! being polled when that work completes.
//!
//! This results in potentially redundant polls, but no lost polls.
//!
//! ### Why do I need an owned inner future in my `get_offload()` closure? It seems cumbersome.
//! A couple reasons:
//!
//! First, this allows the API to accept just a single closure, which can both offload work to vacation,
//! which might need to be sent across threads, and then post-process the work with mutable access
//! to the inner future. We can't simply pass in a &mut reference because this library targets
//! pre-async-closure Rust versions, so it would insist that the inner future reference have a 'static lifetime,
//! which it doesn't.
//!
//! Second, this makes it simpler to model the case where the offloaded work needs some state from the inner future
//! that the inner future can't operate without. If it wasn't owned, the inner future would need to add a new variant
//! to its internal state, which represents the case where it is missing state, and then panic or return `Poll::Pending`
//! if it is polled while in that state.
//!
//! If you have a use case that wants to continue polling the inner future, while also driving any offloaded work
//! (or multiple pieces of offloaded work), please cut a GitHub issue! Certainly we can add this functionality in
//! if consumers would use it. It just would be a bit more complicated API, so we're trying to avoid adding complexity
//! if it wouldn't be used.
//!
//! [`futures_util::then()`]: https://docs.rs/futures-util/latest/futures_util/future/trait.FutureExt.html#method.then
//! [`vacation::future::builder()`]: crate::future::builder()
//! [`get_offload()`]: crate::future::FutureBuilder::get_offload()

use std::{
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};

use pin_project_lite::pin_project;

pin_project! {
///  Wrapper future that processes occasional work offloaded from the inner future, while driving the inner
/// future if no offloaded work is active.
///
/// **Note that `OffloadWith` does NOT poll the inner future while offloaded work is active, so there is a deadlock
/// risk if the offloaded work depends on the inner future making progress to resolve.**
///
/// The order of execution is:
/// - poll any active offloaded vacation work
///     - when it completes, immediately poll the inner future
/// - poll inner future if no active offloaded work
/// - if inner future is pending, check poll to see if there is any vacation work to get_offload
///     - if there is, poll it once and then update the future to continue polling it
///
/// Returning an error in any vacation handling (getting work, or if the offloaded work resolves to an error)
/// will abort the inner future and bubble up. But, you can also discard errors to make these
/// calls infallible.
///
/// # Examples
///
/// ```
/// # // this is hideous to include everywhere but works around https://github.com/rust-lang/rust/issues/67295
/// # // and keeps examples concise to the reader
/// #    use std::{
/// #        future::Future,
/// #        pin::Pin,
/// #        task::{Context, Poll},
/// #    };
/// #
/// #    #[derive(Debug)]
/// #    pub struct SampleFuture;
/// #
/// #    #[derive(Debug, PartialEq)]
/// #    pub enum SampleFutureResponse {
/// #        Success,
/// #        InnerError,
/// #        VacationError(&'static str),
/// #    }
/// #
/// #    impl SampleFuture {
/// #        pub fn new() -> Self {
/// #            Self
/// #        }
/// #
/// #        pub fn get_offload_work(
/// #            &mut self,
/// #        ) -> Result<Option<impl FnOnce() -> Result<(), &'static str>>, &'static str> {
/// #            Ok(Some(|| Ok(())))
/// #        }
/// #
/// #        pub fn post_process_offload_work(&self, _input: ()) {}
/// #    }
/// #
/// #    impl Future for SampleFuture {
/// #        type Output = SampleFutureResponse;
/// #
/// #        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
/// #            Poll::Ready(SampleFutureResponse::Success)
/// #        }
/// #    }
///
/// # #[tokio::main]
/// # async fn main() {
/// let future = vacation::future::builder()
///          // the inner future, you might need to `Box::pin` it if it is !Unpin
///         .future(SampleFuture::new())
///         .get_offload(move |mut inner_fut: SampleFuture| {
///             // this method is directly exposed on the inner future by the library author
///             let maybe_work = match inner_fut.get_offload_work() {
///                 Ok(maybe_work) => maybe_work,
///                 // any bubbled up errors must match the inner future output type
///                 Err(err) => return Err(SampleFutureResponse::VacationError(err))
///             };
///
///             match maybe_work {
///                 // if no work, return the owned inner future back out again
///                 None => Ok(vacation::future::OffloadWork::NoWork(inner_fut)),
///                 // if there is work, return an async closure that calls `vacation::execute()`
///                 // and processes any results (if post-processing is needed)
///                 Some(work) => Ok(vacation::future::OffloadWork::HasWork(
///                     Box::new(async move {
///                         match vacation::execute(work, vacation::ExecuteContext::new(vacation::ChanceOfBlocking::Frequent)).await {
///                             Ok(work_res) => {
///                                 match work_res {
///                                     Ok(work_output) => {
///                                         inner_fut.post_process_offload_work(work_output);
///                                         // on offload work success, you must return the inner future
///                                         Ok(inner_fut)
///                                     },
///                                     Err(work_err) => Err(SampleFutureResponse::VacationError(work_err))
///                                 }
///                             },
///                             Err(_vacation_executor_err) => Err(SampleFutureResponse::VacationError("executor_error"))
///                         }
///                     })
///                 ))
///             }
///         })
///         .build();
/// # }
/// ```
pub struct OffloadWith<InnerFut: Future, GetOffload> {
    inner: OffloadWithInner<InnerFut>,
    get_offload: GetOffload,
}
}

enum OffloadWithInner<InnerFut: Future> {
    InnerFut(InnerFut),
    OffloadActive(Pin<Box<dyn Future<Output = Result<InnerFut, InnerFut::Output>> + Send>>),
    // used within a poll only, to transition between states as needed
    UpdatingState,
}

/// The successful result of the [`get_offload()`] call. This contains either:
/// - Work to be offloaded, passed as an async closure that resolves to
/// either an error or the owner inner future (which calls `vacation::execute()`) inside and handles
/// the result
/// - No work, with the owned inner future
///
/// [`get_offload()`]: crate::future::FutureBuilder::get_offload()
pub enum OffloadWork<InnerFut: Future> {
    /// Work to be offloaded, passed as an async closure that resolves to
    /// either an error or the owner inner future (which calls `vacation::execute()`) inside and handles
    /// the result
    HasWork(Box<dyn Future<Output = Result<InnerFut, InnerFut::Output>> + Send>),
    /// No work needs offloading, so this returns the owned inner future
    NoWork(InnerFut),
}
impl<InnerFut: Future> std::fmt::Debug for OffloadWork<InnerFut> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HasWork(_) => f.debug_tuple("HasWork").finish(),
            Self::NoWork(_) => f.debug_tuple("NoWork").finish(),
        }
    }
}

/// Needs a call to [`future()`]
///
/// [`future()`]: crate::future::FutureBuilder::future()
#[derive(Debug)]
pub struct NeedsInnerFuture;

/// Already has called [`future()`]
///
/// [`future()`]: crate::future::FutureBuilder::future()
pub struct HasInnerFuture<InnerFut: Future>(InnerFut);
impl<InnerFut: Future> std::fmt::Debug for HasInnerFuture<InnerFut> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("HasInnerFuture").finish()
    }
}

/// Needs a call to [`get_offload()`]
///
/// [`get_offload()`]: crate::future::FutureBuilder::get_offload()
#[derive(Debug)]
pub struct NeedsGetOffload;
/// Has already called [`get_offload()`]
///
/// [`get_offload()`]: crate::future::FutureBuilder::get_offload()
pub struct HasGetOffload<GetOffload>(GetOffload);
impl<GetOffload> std::fmt::Debug for HasGetOffload<GetOffload> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("HasGetOffload").finish()
    }
}

/// A builder struct with which to construct [`OffloadWith`] future.
///
/// Start with [`vacation::future::builder()`].
///
/// Note that the wrapper future does NOT poll the inner future while offloaded work is active, so there is a deadlock
/// risk if the offloaded work depends on the inner future making progress to resolve.
///
/// The order of execution is:
/// - poll any active offloaded vacation work
///     - when it completes, immediately poll the inner future
/// - poll inner future if no active offloaded work
/// - if inner future is pending, check poll to see if there is any vacation work to get_offload
///     - if there is, poll it once and then update the future to continue polling it
///
/// Returning an error in any vacation handling (getting work, or if the offloaded work resolves to an error)
/// will abort the inner future and bubble up. But, you can also discard errors to make these
/// calls infallible.
///
/// # Examples
///
/// ```
/// # // this is hideous to include everywhere but works around https://github.com/rust-lang/rust/issues/67295
/// # // and keeps examples concise to the reader
/// #    use std::{
/// #        future::Future,
/// #        pin::Pin,
/// #        task::{Context, Poll},
/// #    };
/// #
/// #    #[derive(Debug)]
/// #    pub struct SampleFuture;
/// #
/// #    #[derive(Debug, PartialEq)]
/// #    pub enum SampleFutureResponse {
/// #        Success,
/// #        InnerError,
/// #        VacationError(&'static str),
/// #    }
/// #
/// #    impl SampleFuture {
/// #        pub fn new() -> Self {
/// #            Self
/// #        }
/// #
/// #        pub fn get_offload_work(
/// #            &mut self,
/// #        ) -> Result<Option<impl FnOnce() -> Result<(), &'static str>>, &'static str> {
/// #            Ok(Some(|| Ok(())))
/// #        }
/// #
/// #        pub fn post_process_offload_work(&self, _input: ()) {}
/// #    }
/// #
/// #    impl Future for SampleFuture {
/// #        type Output = SampleFutureResponse;
/// #
/// #        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
/// #            Poll::Ready(SampleFutureResponse::Success)
/// #        }
/// #    }
///
/// # #[tokio::main]
/// # async fn main() {
/// let future = vacation::future::builder()
///          // the inner future, you might need to `Box::pin` it if it is !Unpin
///         .future(SampleFuture::new())
///         .get_offload(move |mut inner_fut: SampleFuture| {
///             // this method is directly exposed on the inner future by the library author
///             let maybe_work = match inner_fut.get_offload_work() {
///                 Ok(maybe_work) => maybe_work,
///                 // any bubbled up errors must match the inner future output type
///                 Err(err) => return Err(SampleFutureResponse::VacationError(err))
///             };
///
///             match maybe_work {
///                 // if no work, return the owned inner future back out again
///                 None => Ok(vacation::future::OffloadWork::NoWork(inner_fut)),
///                 // if there is work, return an async closure that calls `vacation::execute()`
///                 // and processes any results (if post-processing is needed)
///                 Some(work) => Ok(vacation::future::OffloadWork::HasWork(
///                     Box::new(async move {
///                         match vacation::execute(work, vacation::ExecuteContext::new(vacation::ChanceOfBlocking::Frequent)).await {
///                             Ok(work_res) => {
///                                 match work_res {
///                                     Ok(work_output) => {
///                                         inner_fut.post_process_offload_work(work_output);
///                                         // on offload work success, you must return the inner future
///                                         Ok(inner_fut)
///                                     },
///                                     Err(work_err) => Err(SampleFutureResponse::VacationError(work_err))
///                                 }
///                             },
///                             Err(_vacation_executor_err) => Err(SampleFutureResponse::VacationError("executor_error"))
///                         }
///                     })
///                 ))
///             }
///         })
///         .build();
/// # }
/// ```
///
/// [`OffloadWith`]: crate::future::OffloadWith
/// [`vacation::future::builder()`]: crate::future::builder()
#[derive(Debug)]
pub struct FutureBuilder<InnerFut, GetOffload> {
    inner_fut: InnerFut,
    get_offload: GetOffload,
}

/// Get a builder that constructs an[`OffloadWith`] wrapper future
///
/// [`OffloadWith`]: crate::future::OffloadWith
#[must_use = "doesn't do anything unless built"]
pub fn builder() -> FutureBuilder<NeedsInnerFuture, NeedsGetOffload> {
    FutureBuilder {
        inner_fut: NeedsInnerFuture,
        get_offload: NeedsGetOffload,
    }
}

impl FutureBuilder<NeedsInnerFuture, NeedsGetOffload> {
    #[must_use = "doesn't do anything unless built"]
    ///  Accepts an inner future to wrap with [`OffloadWith`]
    ///
    /// Note that this future will be polled only when there is NOT
    /// offload work active. If offload work completes, it will be immediately
    /// polled again.
    pub fn future<InnerFut>(
        self,
        future: InnerFut,
    ) -> FutureBuilder<HasInnerFuture<InnerFut>, NeedsGetOffload>
    where
        InnerFut: Future + Unpin,
    {
        FutureBuilder::<HasInnerFuture<InnerFut>, NeedsGetOffload> {
            inner_fut: HasInnerFuture(future),
            get_offload: self.get_offload,
        }
    }
}

impl<InnerFut: Future> FutureBuilder<HasInnerFuture<InnerFut>, NeedsGetOffload> {
    #[must_use = "doesn't do anything unless built"]
    ///  A closure accepting an owned inner future, that returns any work to be offloaded,
    /// or the inner future if no offload work is needed.
    ///
    /// Work to be offload should be returned as an async closure that resolves back
    /// to the inner future, or an error. Use [`vacation::execute()`] inside the closure future
    /// and process its results as needed.
    ///
    /// Note that the wrapper future does NOT poll the inner future while offloaded work is active, so there is a deadlock
    /// risk if the offloaded work depends on the inner future making progress to resolve.
    ///
    /// The order of execution is:
    /// - poll any active offloaded vacation work
    ///     - when it completes, immediately poll the inner future
    /// - poll inner future if no active offloaded work
    /// - if inner future is pending, check poll to see if there is any vacation work to get_offload
    ///     - if there is, poll it once and then update the future to continue polling it
    ///
    /// Returning an error in any vacation handling (getting work, incorporating the results of work)
    /// will abort the inner future and bubble up. But, you can also discard errors to make these
    /// calls infallible.
    ///
    /// # Examples
    ///
    /// ```
    /// # // this is hideous to include everywhere but works around https://github.com/rust-lang/rust/issues/67295
    /// # // and keeps examples concise to the reader
    /// #    use std::{
    /// #        future::Future,
    /// #        pin::Pin,
    /// #        task::{Context, Poll},
    /// #    };
    /// #
    /// #    #[derive(Debug)]
    /// #    pub struct SampleFuture;
    /// #
    /// #    #[derive(Debug, PartialEq)]
    /// #    pub enum SampleFutureResponse {
    /// #        Success,
    /// #        InnerError,
    /// #        VacationError(&'static str),
    /// #    }
    /// #
    /// #    impl SampleFuture {
    /// #        pub fn new() -> Self {
    /// #            Self
    /// #        }
    /// #
    /// #        pub fn get_offload_work(
    /// #            &mut self,
    /// #        ) -> Result<Option<impl FnOnce() -> Result<(), &'static str>>, &'static str> {
    /// #            Ok(Some(|| Ok(())))
    /// #        }
    /// #
    /// #        pub fn post_process_offload_work(&self, _input: ()) {}
    /// #    }
    /// #
    /// #    impl Future for SampleFuture {
    /// #        type Output = SampleFutureResponse;
    /// #
    /// #        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
    /// #            Poll::Ready(SampleFutureResponse::Success)
    /// #        }
    /// #    }
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let future = vacation::future::builder()
    ///          // the inner future, you might need to `Box::pin` it if it is !Unpin
    ///         .future(SampleFuture::new())
    ///         .get_offload(move |mut inner_fut: SampleFuture| {
    ///             // this method is directly exposed on the inner future by the library author
    ///             let maybe_work = match inner_fut.get_offload_work() {
    ///                 Ok(maybe_work) => maybe_work,
    ///                 // any bubbled up errors must match the inner future output type
    ///                 Err(err) => return Err(SampleFutureResponse::VacationError(err))
    ///             };
    ///
    ///             match maybe_work {
    ///                 // if no work, return the owned inner future back out again
    ///                 None => Ok(vacation::future::OffloadWork::NoWork(inner_fut)),
    ///                 // if there is work, return an async closure that calls `vacation::execute()`
    ///                 // and processes any results (if post-processing is needed)
    ///                 Some(work) => Ok(vacation::future::OffloadWork::HasWork(
    ///                     Box::new(async move {
    ///                         match vacation::execute(work, vacation::ExecuteContext::new(vacation::ChanceOfBlocking::Frequent)).await {
    ///                             Ok(work_res) => {
    ///                                 match work_res {
    ///                                     Ok(work_output) => {
    ///                                         inner_fut.post_process_offload_work(work_output);
    ///                                         // on offload work success, you must return the inner future
    ///                                         Ok(inner_fut)
    ///                                     },
    ///                                     Err(work_err) => Err(SampleFutureResponse::VacationError(work_err))
    ///                                 }
    ///                             },
    ///                             Err(_vacation_executor_err) => Err(SampleFutureResponse::VacationError("executor_error"))
    ///                         }
    ///                     })
    ///                 ))
    ///             }
    ///         })
    ///         .build();
    /// # }
    /// ```
    ///
    /// [`vacation::execute()`]: crate::execute()
    pub fn get_offload<GetOffload>(
        self,
        get_offload: GetOffload,
    ) -> FutureBuilder<HasInnerFuture<InnerFut>, HasGetOffload<GetOffload>>
    where
        GetOffload: Fn(InnerFut) -> Result<OffloadWork<InnerFut>, InnerFut::Output>,
        InnerFut: Future + Unpin,
    {
        FutureBuilder::<HasInnerFuture<InnerFut>, HasGetOffload<GetOffload>> {
            inner_fut: self.inner_fut,
            get_offload: HasGetOffload(get_offload),
        }
    }
}

impl<InnerFut: Future, GetOffload>
    FutureBuilder<HasInnerFuture<InnerFut>, HasGetOffload<GetOffload>>
{
    #[must_use = "doesn't do anything unless polled"]
    /// Finish building [`OffloadWith`]
    pub fn build(self) -> OffloadWith<InnerFut, GetOffload> {
        OffloadWith {
            inner: OffloadWithInner::InnerFut(self.inner_fut.0),
            get_offload: self.get_offload.0,
        }
    }
}

impl<InnerFut: Future, GetOffload> OffloadWith<InnerFut, GetOffload> {
    /// Polls any offloaded work that is present and return any error.
    /// If work completes successfully, also poll the inner future and update inner state.
    fn poll_offload_and_then_inner(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<InnerFut::Output>
    where
        InnerFut: Future + Unpin,
    {
        if let OffloadWithInner::OffloadActive(ref mut offload_future) = self.as_mut().inner {
            let offload_res = ready!(offload_future.as_mut().poll(cx));
            match offload_res {
                Ok(inner_fut) => {
                    self.inner = OffloadWithInner::InnerFut(inner_fut);
                    self.poll_inner(cx)
                }
                Err(err) => Poll::Ready(err),
            }
        } else {
            Poll::Pending
        }
    }

    /// Poll any inner future that is present
    fn poll_inner(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<InnerFut::Output>
    where
        InnerFut: Future + Unpin,
    {
        let this = self.project();
        if let OffloadWithInner::InnerFut(inner_fut) = this.inner {
            Pin::new(inner_fut).as_mut().poll(cx)
        } else {
            Poll::Pending
        }
    }
}

impl<InnerFut, GetOffload> Future for OffloadWith<InnerFut, GetOffload>
where
    InnerFut: Future + Unpin,
    GetOffload: Fn(InnerFut) -> Result<OffloadWork<InnerFut>, InnerFut::Output>,
{
    type Output = InnerFut::Output;

    // Flow:
    //
    // First, drive the stored future, one of:
    // a) - Poll inner future
    // b) - Poll offload work -> if done, poll inner future
    //
    // Then:
    // 1. See if we have offload work active
    // 2. If yes, we are done, return Poll::Pending
    // 2. If no, split apart inner to get owned inner future
    // 3. Send inner future into get_offload() closure
    // 4. Put inner back together with returned offload work or inner future
    // 5. Poll any new offload work (and inner future, if offload compltes)
    //
    // Poll::Ready(..) from the inner future, or Poll::Ready(Err<>) from any
    // offload work handling, will return Poll::Ready. Else Poll::Pending.
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // drive our stored future, inner or offloaded work
        if let Poll::Ready(inner_res) = match self.inner {
            OffloadWithInner::InnerFut(_) => self.as_mut().poll_inner(cx),
            OffloadWithInner::OffloadActive(_) => self.as_mut().poll_offload_and_then_inner(cx),
            OffloadWithInner::UpdatingState => {
                // we only set this state in the subsequent block, where we unconditionally set a different
                // state or return Poll::Ready
                unreachable!("unexpected state while polling OffloadWith")
            }
        } {
            return Poll::Ready(inner_res);
        }

        // if we don't already have offloaded work, see if there is new work.
        if matches!(self.inner, OffloadWithInner::InnerFut(_)) {
            match std::mem::replace(&mut self.inner, OffloadWithInner::UpdatingState) {
                OffloadWithInner::InnerFut(inner_fut) => match (self.get_offload)(inner_fut) {
                    Ok(offload_work) => match offload_work {
                        OffloadWork::HasWork(work) => {
                            self.inner = OffloadWithInner::OffloadActive(Box::into_pin(work));
                            self.poll_offload_and_then_inner(cx)
                        }
                        OffloadWork::NoWork(inner_fut) => {
                            self.inner = OffloadWithInner::InnerFut(inner_fut);
                            Poll::Pending
                        }
                    },
                    Err(err) => Poll::Ready(err),
                },
                _ => {
                    // we match to confirm we have an inner future immediately above
                    unreachable!("unexpected state while polling OffloadWith")
                }
            }
        } else {
            // we already have offload work active, don't look for more
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    };

    #[derive(Debug, PartialEq)]
    enum TestFutureResponse {
        Success {
            poll_count: usize,
            sync_work_remaining: usize,
        },
        InnerError {
            poll_count: usize,
            sync_work_remaining: usize,
        },
        VacationGetError,
        VacationOffloadWorkError,
        VacationExecutorError,
    }

    struct TestFuture {
        async_work: usize,
        sync_work: usize,
        poll_count: usize,
        error_after_n_polls: Option<usize>,
    }

    impl TestFuture {
        fn new(async_work: usize, sync_work: usize) -> Self {
            Self {
                async_work,
                sync_work,
                error_after_n_polls: None,
                poll_count: 0,
            }
        }

        fn new_with_err_after(
            async_work: usize,
            sync_work: usize,
            polls_before_err: usize,
        ) -> Self {
            Self {
                async_work,
                sync_work,
                error_after_n_polls: Some(polls_before_err),
                poll_count: 0,
            }
        }

        fn get_sync_work(&mut self) -> Option<()> {
            if self.sync_work > 0 {
                self.sync_work -= 1;
                Some(())
            } else {
                None
            }
        }
    }

    impl Future for TestFuture {
        type Output = TestFutureResponse;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            self.poll_count += 1;

            if let Some(poll_count) = self.error_after_n_polls {
                if poll_count < self.poll_count {
                    return Poll::Ready(TestFutureResponse::InnerError {
                        poll_count: self.poll_count,
                        sync_work_remaining: self.sync_work,
                    });
                }
            }

            self.async_work -= 1;
            match self.async_work {
                remaining if remaining > 0 => {
                    // tell executor to immediately poll us again,
                    // which passes through our vacation future as well
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                _ => Poll::Ready(TestFutureResponse::Success {
                    poll_count: self.poll_count,
                    sync_work_remaining: self.sync_work,
                }),
            }
        }
    }

    /// Wraps a [`TestFuture`] in vacation handling
    /// to send to a mpsc channel each stage's call, or
    /// return an error if that channel is dropped.
    fn vacation_fut(
        inner_fut: TestFuture,
        get_offload_err: bool,
        offload_work_err: bool,
    ) -> OffloadWith<
        TestFuture,
        impl Fn(TestFuture) -> Result<OffloadWork<TestFuture>, TestFutureResponse>,
    > {
        crate::future::builder()
            .future(inner_fut)
            .get_offload(move |mut inner_fut| {
                if get_offload_err {
                    return Err(TestFutureResponse::VacationGetError);
                }

                match inner_fut.get_sync_work() {
                    None => Ok(OffloadWork::NoWork(inner_fut)),
                    Some(_) => Ok(OffloadWork::HasWork(Box::new(async move {
                        match crate::execute(
                            move || {
                                if offload_work_err {
                                    Err(TestFutureResponse::VacationOffloadWorkError)
                                } else {
                                    Ok(())
                                }
                            },
                            crate::ExecuteContext::new(crate::ChanceOfBlocking::Frequent),
                        )
                        .await
                        {
                            Ok(res) => match res {
                                Ok(_) => Ok(inner_fut),
                                Err(_) => Err(TestFutureResponse::VacationOffloadWorkError),
                            },
                            Err(_) => Err(TestFutureResponse::VacationExecutorError),
                        }
                    }))),
                }
            })
            .build()
    }

    #[tokio::test]
    async fn no_offload() {
        let inner_fut = TestFuture::new(3, 0);

        let future = vacation_fut(
            inner_fut, false, true, // this error is not reachable since we don't have work
        );

        let res = future.await;

        assert_eq!(
            res,
            TestFutureResponse::Success {
                poll_count: 3,
                sync_work_remaining: 0
            }
        );
    }

    #[tokio::test]
    async fn with_offload() {
        let inner_fut = TestFuture::new(3, 1);

        let future = vacation_fut(inner_fut, false, false);

        let res = future.await;

        assert_eq!(
            res,
            TestFutureResponse::Success {
                poll_count: 3,
                sync_work_remaining: 0
            }
        );
    }

    #[tokio::test]
    async fn with_offload_remaining_after_inner() {
        let inner_fut = TestFuture::new(2, 3);

        let future = vacation_fut(inner_fut, false, false);

        let res = future.await;

        assert_eq!(
            res,
            TestFutureResponse::Success {
                poll_count: 2,
                // poll inner -> get work -> poll inner -> done, with 2 sync remaining
                sync_work_remaining: 2
            }
        );
    }

    #[tokio::test]
    async fn with_get_offload_err() {
        let inner_fut = TestFuture::new(2, 3);

        let future = vacation_fut(inner_fut, true, false);

        let res = future.await;

        assert_eq!(res, TestFutureResponse::VacationGetError);
    }
    #[tokio::test]
    async fn with_offload_work_err() {
        let inner_fut = TestFuture::new(2, 3);

        let future = vacation_fut(inner_fut, false, true);

        let res = future.await;

        assert_eq!(res, TestFutureResponse::VacationOffloadWorkError);
    }

    #[tokio::test]
    async fn delayed_inner_err() {
        let inner_fut = TestFuture::new_with_err_after(2, 1, 1);

        let future = vacation_fut(inner_fut, false, false);

        let res = future.await;

        assert_eq!(
            res,
            // inner poll -> get + run work -> inner poll w/ error
            TestFutureResponse::InnerError {
                poll_count: 2,
                sync_work_remaining: 0
            }
        );
    }
}
