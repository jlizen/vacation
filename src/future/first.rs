use std::{
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};

use pin_project_lite::pin_project;

use super::{
    FutureBuilder, HasIncorporateFn, NeedsIncorporateFn, NeedsOffload, NoInnerFuture,
    OffloadFirstStrat, WhileWaitingMode,
};

pin_project! {
/// Initialize the inner future after executing a single piece of work with vacation.
/// That work should output return inputs necessary to construct the inner future.
///
/// Continue polling the inner future as a pass-through after initialization.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// let future = vacation::future::builder()
///         .offload_first(
///             vacation::future::OffloadFirst::builder()
///                 .offload_future(vacation::execute(
///                      // the work to offload
///                     || std::thread::sleep(std::time::Duration::from_millis(100)),
///                     vacation::ChanceOfBlocking::High
///                 ))
///                 // accepts the result of offloaded work, and returns an erorr
///                 // or an inner future
///                 .incorporate_fn(|res| {
///                     // vacation work returned an executor error
///                     if let Err(err) = res {
///                         return Err(false);
///                     }
///                     Ok(Box::pin(async move {
///                         tokio::time::sleep(std::time::Duration::from_millis(50)).await;
///                         true
///                     }))
///                 })
///          )
///         .build();
///
/// assert_eq!(future.await, true)
/// # }
/// ```
pub struct OffloadFirstFuture<InnerFut, OffloadResult, IncorporateFn> {
    inner: Inner<InnerFut, OffloadResult, IncorporateFn>
}
}

enum Inner<InnerFut, OffloadResult, IncorporateFn> {
    OffloadInProgress {
        offload_fut: Pin<Box<dyn Future<Output = Result<OffloadResult, crate::Error>> + Send>>,
        incorporate: Incorporate<IncorporateFn>,
    },
    InnerFuture {
        inner_fut: InnerFut,
    },
}

/// The incorporate function passed via [`incorporate_fn()`].
///
/// Takes output of vacation work and generates inner future
///
/// [`incorporate_fn()`]: crate::future::first::OffloadFirst::incorporate_fn()
#[derive(Debug)]
pub enum Incorporate<IncorporateFn> {
    /// Vacation work still pending
    Incorporate(IncorporateFn),
    /// Transitional state used while resolving
    Consumed,
}

/// A sub-builder for the work to be offloaded via [`OffloadFirstFuture`]
///
/// Use [`OffloadFirst::builder()`] to construct it.
///
/// [`OffloadFirst::builder()`]: crate::future::first::OffloadFirst::builder()
/// [`OffloadFirstFuture`]: crate::future::first::OffloadFirstFuture
#[derive(Debug)]
pub struct OffloadFirst<OffloadFuture, IncorporateFn> {
    offload_future: OffloadFuture,
    incorporate_fn: IncorporateFn,
}

/// Needs input work to execute via vacation
#[derive(Debug)]
pub struct NeedsOffloadFuture;
/// Has work ready to execute via vacation
pub struct HassOffloadFuture<OffloadResult>(
    Pin<Box<dyn Future<Output = Result<OffloadResult, crate::Error>> + Send>>,
);

impl<OffloadResult> std::fmt::Debug for HassOffloadFuture<OffloadResult> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("HassOffloadFuture").finish()
    }
}

impl OffloadFirst<NeedsOffloadFuture, NeedsIncorporateFn> {
    /// A sub-builder for the work to be offloaded via [`OffloadFirstFuture`]
    ///
    /// ## Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///   let sub_builder = vacation::future::OffloadFirst::builder()
    ///                 .offload_future(vacation::execute(
    ///                      // the work to offload
    ///                     || std::thread::sleep(std::time::Duration::from_millis(100)),
    ///                     vacation::ChanceOfBlocking::High
    ///                 ))
    ///                 // accepts the result of offloaded work, and returns an erorr
    ///                 // or an inner future
    ///                 .incorporate_fn(|res: Result<(), vacation::Error>| {
    ///                     // vacation work returned an executor error
    ///                     if let Err(err) = res {
    ///                         return Err(false);
    ///                     }   
    ///                     Ok(Box::pin(async move {
    ///                         tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    ///                         true
    ///                     }))
    ///                 });
    /// # }
    /// ```
    /// [`OffloadFirstFuture`]: crate::future::first::OffloadFirstFuture
    #[must_use = "doesn't do anything unless built"]
    pub fn builder() -> OffloadFirst<NeedsOffloadFuture, NeedsIncorporateFn> {
        OffloadFirst {
            offload_future: NeedsOffloadFuture,
            incorporate_fn: NeedsIncorporateFn,
        }
    }
}

impl<IncorporateFn> OffloadFirst<NeedsOffloadFuture, IncorporateFn> {
    /// The future to offload via vacation. Results of this are processed
    /// via the input to [`incorporate_fn()`]
    ///
    /// [`incorporate_fn()`]: crate::future::first::OffloadFirst::incorporate_fn()
    #[must_use = "doesn't do anything unless built"]
    pub fn offload_future<OffloadResult>(
        self,
        offload_future: impl Future<Output = Result<OffloadResult, crate::Error>> + Send + 'static,
    ) -> OffloadFirst<HassOffloadFuture<OffloadResult>, IncorporateFn>
    where
        OffloadResult: Send + 'static,
    {
        OffloadFirst {
            offload_future: HassOffloadFuture(Box::pin(offload_future)),
            incorporate_fn: self.incorporate_fn,
        }
    }
}

impl<OffloadFuture> OffloadFirst<OffloadFuture, NeedsIncorporateFn> {
    /// Process the output of the offloaded work, along with any other state
    /// owned by this closure, and generate the inner future.
    #[must_use = "doesn't do anything unless built"]
    pub fn incorporate_fn<IncorporateFn, OffloadResult, InnerFut>(
        self,
        incorporate_fn: IncorporateFn,
    ) -> OffloadFirst<OffloadFuture, HasIncorporateFn<IncorporateFn>>
    where
        IncorporateFn:
            FnOnce(Result<OffloadResult, crate::Error>) -> Result<InnerFut, InnerFut::Output>,
        OffloadResult: Send + 'static,
        InnerFut: Future + Unpin,
    {
        OffloadFirst {
            offload_future: self.offload_future,
            incorporate_fn: HasIncorporateFn(incorporate_fn),
        }
    }
}

impl<Strategy, InnerFuture, WhileWaiting>
    FutureBuilder<Strategy, InnerFuture, NeedsOffload, WhileWaiting>
{
    /// Initialize the inner future after executing a single piece of work with vacation.
    /// That work should output return inputs necessary to construct the inner future.
    ///
    /// Continue polling the inner future as a pass-through after initialization.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let future = vacation::future::builder()
    ///         .offload_first(
    ///             vacation::future::OffloadFirst::builder()
    ///                 .offload_future(vacation::execute(
    ///                      // the work to offload
    ///                     || std::thread::sleep(std::time::Duration::from_millis(100)),
    ///                     vacation::ChanceOfBlocking::High
    ///                 ))
    ///                 // accepts the result of offloaded work, and returns an erorr
    ///                 // or an inner future
    ///                 .incorporate_fn(|res| {
    ///                     // vacation work returned an executor error
    ///                     if let Err(err) = res {
    ///                         return Err(false);
    ///                     }
    ///                     Ok(Box::pin(async move {
    ///                         tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    ///                         true
    ///                     }))
    ///                 })
    ///          )
    ///         .build();
    ///
    /// assert_eq!(future.await, true)
    /// # }
    /// ```
    #[must_use = "doesn't do anything unless built"]
    pub fn offload_first<InnerFut, OffloadResult, IncorporateFn>(
        self,
        offload: OffloadFirst<HassOffloadFuture<OffloadResult>, HasIncorporateFn<IncorporateFn>>,
    ) -> FutureBuilder<
        OffloadFirstStrat,
        NoInnerFuture,
        OffloadFirst<HassOffloadFuture<OffloadResult>, HasIncorporateFn<IncorporateFn>>,
        WhileWaitingMode,
    >
    where
        IncorporateFn:
            FnOnce(Result<OffloadResult, crate::Error>) -> Result<InnerFut, InnerFut::Output>,
        OffloadResult: Send + 'static,
        InnerFut: Future + Unpin,
    {
        FutureBuilder::<
            OffloadFirstStrat,
            NoInnerFuture,
            OffloadFirst<HassOffloadFuture<OffloadResult>, HasIncorporateFn<IncorporateFn>>,
            WhileWaitingMode,
        > {
            strategy: OffloadFirstStrat,
            inner_fut: NoInnerFuture,
            offload,
            while_waiting: WhileWaitingMode::SuppressInnerPoll,
        }
    }
}

impl<IncorporateFn, OffloadResult>
    FutureBuilder<
        OffloadFirstStrat,
        NoInnerFuture,
        OffloadFirst<HassOffloadFuture<OffloadResult>, HasIncorporateFn<IncorporateFn>>,
        WhileWaitingMode,
    >
{
    #[must_use = "doesn't do anything unless polled"]
    /// Finish building [`OffloadFirstFuture`]
    pub fn build<InnerFut>(self) -> OffloadFirstFuture<InnerFut, OffloadResult, IncorporateFn>
    where
        IncorporateFn:
            FnOnce(Result<OffloadResult, crate::Error>) -> Result<InnerFut, InnerFut::Output>,
        OffloadResult: Send + 'static,
        InnerFut: Future + Unpin,
    {
        let inner: Inner<InnerFut, OffloadResult, IncorporateFn> = Inner::OffloadInProgress {
            offload_fut: self.offload.offload_future.0,
            incorporate: Incorporate::Incorporate(self.offload.incorporate_fn.0),
        };

        OffloadFirstFuture { inner }
    }
}

impl<InnerFut, OffloadResult, IncorporateFn> Future
    for OffloadFirstFuture<InnerFut, OffloadResult, IncorporateFn>
where
    InnerFut: Future + Unpin,
    OffloadResult: Send + 'static,
    IncorporateFn:
        FnOnce(Result<OffloadResult, crate::Error>) -> Result<InnerFut, InnerFut::Output>,
{
    type Output = InnerFut::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match &mut this.inner {
            // we've already finished our startup vacation work, just poll inner
            Inner::InnerFuture { ref mut inner_fut } => Pin::new(inner_fut).as_mut().poll(cx),
            // vacation startup work still ongoing
            Inner::OffloadInProgress {
                ref mut offload_fut,
                ref mut incorporate,
            } => {
                // drive vacation work to completion
                let res = ready!(Pin::new(offload_fut).as_mut().poll(cx));

                // consume the incorporate
                let incorporate = match std::mem::replace(incorporate, Incorporate::Consumed) {
                    Incorporate::Incorporate(incorporate) => incorporate,
                    Incorporate::Consumed => {
                        panic!("offload first work can only finish and be incorporated once")
                    }
                };

                // incorporate offloaded work result into inner future
                match (incorporate)(res) {
                    Ok(mut inner_fut) => {
                        // poll inner future once
                        match Pin::new(&mut inner_fut).poll(cx) {
                            // inner future is done, return it
                            Poll::Ready(res) => Poll::Ready(res),
                            Poll::Pending => {
                                // otherwise the inner future already registered its wakers this past poll
                                this.inner = Inner::InnerFuture { inner_fut };
                                Poll::Pending
                            }
                        }
                    }
                    // if the incorporate fn returns an error, bubble it up
                    Err(err) => Poll::Ready(err),
                }
            }
        }
    }
}

// these tests all use the default executor which is `ExecuteDirectly`
#[cfg(test)]
mod test {
    use crate::{
        future::test::{TestFuture, TestFutureResponse},
        ChanceOfBlocking,
    };

    use super::*;

    #[tokio::test]
    async fn it_works() {
        let mut inner_fut = TestFuture::new(3);

        let future = crate::future::builder()
            .offload_first(
                OffloadFirst::builder()
                    .offload_future(crate::execute(move || false, ChanceOfBlocking::High))
                    .incorporate_fn(move |res| {
                        Ok(Box::pin(async move {
                            if res.unwrap() {
                                inner_fut.error_after(0);
                            }
                            inner_fut.await
                        }))
                    }),
            )
            .build();

        let res = future.await;
        // polled inner 3 times since we had 3 inner work units
        assert_eq!(res, TestFutureResponse::Success(3));
    }

    #[tokio::test]
    async fn inner_fut_fails() {
        let mut inner_fut = TestFuture::new(3);

        let future = crate::future::builder()
            .offload_first(
                OffloadFirst::builder()
                    .offload_future(crate::execute(move || true, ChanceOfBlocking::High))
                    .incorporate_fn(move |res| {
                        Ok(Box::pin(async move {
                            if res.unwrap() {
                                inner_fut.error_after(0);
                            }
                            inner_fut.await
                        }))
                    }),
            )
            .build();

        let res = future.await;
        // first poll fails
        assert_eq!(res, TestFutureResponse::InnerError(1));
    }

    #[tokio::test]
    async fn vacation_fails() {
        let future = crate::future::builder()
            .offload_first(
                OffloadFirst::builder()
                    .offload_future(crate::execute(|| {}, ChanceOfBlocking::High))
                    .incorporate_fn(move |_res: Result<(), crate::Error>| {
                        Err(TestFutureResponse::VacationIncorporateError)
                    }),
            )
            .build::<TestFuture>();

        let res = future.await;
        assert_eq!(res, TestFutureResponse::VacationIncorporateError);
    }
}
