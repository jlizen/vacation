//! # Future module
//! 
//! This module provides a lower-level wrapper for manually implemented futures. It is intended for library
//! authors that already have a hand-implemented future, that needs to delegate work.
//! 
//! Such a use case cannot simply call [`vacation::execute(_)`] and await the returned future
//! because they are already in a sync context. Instead, the vacation future needs to be driven 
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
//! of any offloaded vacation work directly in your future, and poll it as part of your `Future` implementation.
//! That way you can simply initialize that work whenever you reach a point that needs it, poll it once, and either
//! return `Poll::Pending` (if you need to wait on that work completing) or carry on (if you can make other progress
//! while the offloaded work is ongoing).
//! 
//! ### I only need to send work to vacation once, in order to construct my inner future. What should I do?
//! 
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
//!         std::thread::sleep(std::time::Duration::from_millis(50));
//!         5
//!     },
//!     vacation::ExecuteContext {
//!         chance_of_blocking: vacation::ChanceOfBlocking::High,
//!         namespace: "sample_initialize",
//!     }
//! );
//! 
//! let future = vacation_future.then(|res| async move {
//!     match res {
//!         Err(error) => Err("there was a vacation error"),
//!         // placeholder for your custom future
//!         Ok(res) => Ok(tokio::time::sleep(std::time::Duration::from_millis(res)).await),
//!     }
//! });
//! 
//! ```
//! 
//! ### This seems suspicious, what if the inner future has a waker fire while it is being suppressed
//! due to ongoing offloaded work?
//! 
//! We defensively poll the inner future anytime the offloaded work completes. This means that if any
//! waker had fired while that work was ongoing, the inner future will be polled again.
//! 
//! Essentially, our guarantee is, every time the wrapper future is polled, either the inner future is polled
//! directly (if no offloaded work), it is polled after vacation work (if vacation work is completing),
//! or there is at minimum a vacation waker registered - which, again, will result in the inner future
//! being polled when that work completes.
//! 
//! This results in potentially redundant polls, but no lost polls.
//! 
//! ### Why do I need an owned inner future in my `get_offload()` closure? It seems cumbersome.
//! A couple reasons:
//! 
//! First, this allows the API to accept just a single closure, which can both delegate work to vacation,
//! which might need to be sent across threads, and then post-process the work with mutable access
//! to the inner future. We can't simply pass in a &mut reference because this library targets
//! pre-async-closure Rust versions, so it would insist that the inner future reference have a 'static lifetime,
//! which it doesn't. You could instead split into two closures, one of which generates the offloaded work, and the
//! second of which post-processes by taking as inputs the offloaded work output and `&mut inner_future`. However,
//! this was pretty unwieldy.
//! 
//! Second, this makes it simpler to model the case where the offloaded work needs some state from the inner future
//! that the inner future can't operate without. If it wasn't owned, the inner future would need to add a new variant
//! to its internal state, which represents the case where it is missing state, and then panic or return `Poll::Pending`
//! if it is polled while in that state.
//! 
//! If you have a use case that wants to continue polling the inner future, while also driving any offloaded work
//! (or multiple pieces of offloaded work), please cut a GitHub issue! Certainly we can add this functionality in
//! if consumers would use it.
//! 
//! [`futures_util::then()`]: https://docs.rs/futures-util/latest/futures_util/future/trait.FutureExt.html#method.then
//! [`vacation::future::builder()`]: crate::future::builder()
//! [`get_offload()`]: crate::future::FutureBuilder::get_offload()

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
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
///                         match vacation::execute(work, vacation::ExecuteContext {
///                             chance_of_blocking: vacation::ChanceOfBlocking::High,
///                             namespace: "sample-future"
///                         }).await {
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
///                         match vacation::execute(work, vacation::ExecuteContext {
///                             chance_of_blocking: vacation::ChanceOfBlocking::High,
///                             namespace: "sample-future"
///                         }).await {
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
    ///                         match vacation::execute(work, vacation::ExecuteContext {
    ///                             chance_of_blocking: vacation::ChanceOfBlocking::High,
    ///                             namespace: "sample-future"
    ///                         }).await {
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

impl<InnerFut, GetOffload> Future for OffloadWith<InnerFut, GetOffload>
where
    InnerFut: Future + Unpin,
    GetOffload: Fn(InnerFut) -> Result<OffloadWork<InnerFut>, InnerFut::Output>,
{
    type Output = InnerFut::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        // first poll the inner future or any outstanding work
        let inner_fut = match std::mem::replace(&mut this.inner, OffloadWithInner::UpdatingState) {
            OffloadWithInner::UpdatingState => {
                panic!("unexpected state while polling get_offload with future")
            }
            OffloadWithInner::InnerFut(mut inner_fut) => match Pin::new(&mut inner_fut).poll(cx) {
                Poll::Ready(res) => return Poll::Ready(res),
                Poll::Pending => inner_fut,
            },
            OffloadWithInner::OffloadActive(mut offload_fut) => {
                let mut inner_fut = match offload_fut.as_mut().poll(cx) {
                    // get_offload work still ongoing, reconstruct state and bail
                    Poll::Pending => {
                        this.inner = OffloadWithInner::OffloadActive(offload_fut);
                        return Poll::Pending;
                    }
                    // get_offload work complete
                    Poll::Ready(offload_res) => match offload_res {
                        Ok(inner_fut) => inner_fut,
                        // bubble up error in get_offload
                        Err(res) => return Poll::Ready(res),
                    },
                };
                // if get_offload future is done and successful,
                // poll our inner future again in case it didn't set wakers since it was relying on
                // vacation work completing
                match Pin::new(&mut inner_fut).poll(cx) {
                    Poll::Ready(res) => return Poll::Ready(res),
                    Poll::Pending => inner_fut,
                }
            }
        };

        match (this.get_offload)(inner_fut) {
            Ok(get_offload_res) => match get_offload_res {
                OffloadWork::HasWork(get_offload) => {
                    let mut get_offload = Box::into_pin(get_offload);
                    // poll get_offload work once to kick it off
                    match get_offload.as_mut().poll(cx) {
                        // get_offload work didn't actually need to sleep
                        Poll::Ready(offload_res) => match offload_res {
                            // successfully completed get_offload work, poll our inner future
                            // again in case it didn't set wakers since it was relying on
                            // vacation work completing
                            Ok(mut inner_fut) => match Pin::new(&mut inner_fut).poll(cx) {
                                Poll::Ready(res) => Poll::Ready(res),
                                Poll::Pending => {
                                    this.inner = OffloadWithInner::InnerFut(inner_fut);
                                    Poll::Pending
                                }
                            },
                            // get_offload work failed, bubble up any error
                            Err(err) => Poll::Ready(err),
                        },
                        // get_offload work still ongoing, store in state
                        Poll::Pending => {
                            this.inner = OffloadWithInner::OffloadActive(get_offload);
                            Poll::Pending
                        }
                    }
                }
                // get get_offload work didn't return any work, reconstruct inner future state
                OffloadWork::NoWork(inner_fut) => {
                    this.inner = OffloadWithInner::InnerFut(inner_fut);
                    Poll::Pending
                }
            },
            // bubble up error while getting get_offload work
            Err(err) => Poll::Ready(err),
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
                            crate::ExecuteContext {
                                chance_of_blocking: crate::ChanceOfBlocking::High,
                                namespace: "test.operation",
                            },
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
