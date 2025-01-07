use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project_lite::pin_project;

use super::{
    FutureBuilder, HasIncorporateFn, NeedsIncorporateFn, NeedsInnerFuture, NeedsOffload,
    NeedsStrategy, NeedsWhileWaiting, OffloadWithFutureStrat, WhileWaitingMode,
};

pin_project! {
///  Wrapper future that processes occasional work offloaded from the inner future, while driving the inner future.
///
/// The order of execution is:
/// - poll any active vacation work
///     - if work completes, the incorporate fn to resolve it (includes mutable pointer to inner task + result of vacation)
/// - poll inner future (if either [`WhileWaitingMode::PassThroughInnerPoll`], or no active offloaded work
/// - poll to see if there is any vacation work to prepare (if no work is already active)
///     - if there is, poll it once and then call the incorporate fn with its results if complete
///
/// Returning an error in any vacation handling (getting work, incorporating the results of work)
/// will abort the inner future and bubble up. But, you can also discard errors to make these
/// calls infallible.
///
/// # Examples
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// let future = vacation::future::builder()
///          // the wrapped future
///         .future(Box::pin( async move {
///             tokio::time::sleep(std::time::Duration::from_millis(50)).await;
///             true
///         }))
///         .offload_with(
///             vacation::future::OffloadWith::builder()
///                 // take a mutable reference to inner future,
///                 // and retrieve any work to offload
///                 .get_offload_fn(|_inner_fut| {
///                     // it could be conditional, but here it's always returning work
///                     Ok(Some(Box::pin(vacation::execute(
///                         || std::thread::sleep(std::time::Duration::from_millis(50)),
///                         vacation::ChanceOfBlocking::High
///                     ))))
///                 })
///                 // called with the results of the offloaded work and the inner future,
///                 // use to convert errors or do any post-processing
///                 .incorporate_fn(|_inner_fut, res| {
///                     println!("work complete: {res:#?}");
///                     Ok(())
///                 })
///         )
///         // could also suppress the inner poll
///         .while_waiting_for_offload(vacation::future::WhileWaitingMode::PassThroughInnerPoll)
///         .build();
///
/// assert_eq!(future.await, true)
/// # }
/// ```
/// [`WhileWaitingMode::PassThroughInnerPoll`]: crate::future::WhileWaitingMode::PassThroughInnerPoll
pub struct OffloadWithFuture<InnerFut, GetOffloadFn, OffloadResult, IncorporateFn> {
    inner_fut: InnerFut,
    get_offload_fn: GetOffloadFn,
    incorporate_fn: IncorporateFn,
    offload_fut: Option<Pin<Box<dyn Future<Output = Result<OffloadResult, crate::Error>> + Send>>>,
    while_waiting: WhileWaitingMode,
}
}

#[derive(Debug)]
/// A sub-builder for work to be offloaded via [`OffloadWithFuture`]
///
/// # Examples
///
/// ```
/// # type InnerFut = Box<std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>>;
/// # #[tokio::main]
/// # async fn run() {
/// let builder = vacation::future::OffloadWith::builder()
///      // take a mutable reference to inner future,
///      // and retrieve any work to offload
///      .get_offload_fn(|_inner_fut: &mut InnerFut| {
///          // it could be conditional, but here it's always returning work
///          Ok(Some(Box::pin(vacation::execute(
///              || std::thread::sleep(std::time::Duration::from_millis(50)),
///              vacation::ChanceOfBlocking::High
///          ))))
///      })
///      .incorporate_fn(|_inner_fut: &mut InnerFut, res: Result<bool, vacation::Error>| {
///           println!("work complete: {res:#?}");
///           Ok(())
///       });
/// # }
/// ```
pub struct OffloadWith<GetOffloadFn, IncorporateFn> {
    get_offload_fn: GetOffloadFn,
    incorporate_fn: IncorporateFn,
}

// this is the builder impl that splits into together or initialize builders
impl FutureBuilder<NeedsStrategy, NeedsInnerFuture, NeedsOffload, NeedsWhileWaiting> {
    #[must_use = "doesn't do anything unless built"]
    ///  Accepts an inner future to wrap with [`OffloadWithFuture`]
    ///
    /// The order of execution is:
    /// - poll any active vacation work
    ///     - if work completes, the incorporate fn to resolve it (includes mutable pointer to inner task + result of vacation)
    /// - poll inner future (if either [`WhileWaitingMode::PassThroughInnerPoll`], or no active offloaded work
    /// - poll to see if there is any vacation work to prepare (if no work is already active)
    ///     - if there is, poll it once and then call the incorporate fn with its results if complete
    ///
    /// Returning an error in any vacation handling (getting work, incorporating the results of work)
    /// will abort the inner future and bubble up. But, you can also discard errors to make these
    /// calls infallible.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let future = vacation::future::builder()
    ///          // the wrapped future
    ///         .future(Box::pin( async move {
    ///             tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    ///             true
    ///         }))
    ///         .offload_with(
    ///             vacation::future::OffloadWith::builder()
    ///                 // take a mutable reference to inner future,
    ///                 // and retrieve any work to offload
    ///                 .get_offload_fn(|_inner_fut| {
    ///                     // it could be conditional, but here it's always returning work
    ///                     Ok(Some(Box::pin(vacation::execute(
    ///                         || std::thread::sleep(std::time::Duration::from_millis(50)),
    ///                         vacation::ChanceOfBlocking::High
    ///                     ))))
    ///                 })
    ///                 // called with the results of the offloaded work and the inner future,
    ///                 // use to convert errors or do any post-processing
    ///                 .incorporate_fn(|_inner_fut, res| {
    ///                     println!("work complete: {res:#?}");
    ///                     Ok(())
    ///                 })
    ///         )
    ///         // could also suppress the inner poll
    ///         .while_waiting_for_offload(vacation::future::WhileWaitingMode::PassThroughInnerPoll)
    ///         .build();
    ///
    /// assert_eq!(future.await, true)
    /// # }
    /// ```
    /// [`WhileWaitingMode::PassThroughInnerPoll`]: crate::future::WhileWaitingMode::PassThroughInnerPoll
    /// [`OffloadWith`]: crate::future::with::OffloadWithFuture
    pub fn future<InnerFut>(
        self,
        inner_fut: InnerFut,
    ) -> FutureBuilder<OffloadWithFutureStrat, InnerFut, NeedsOffload, NeedsWhileWaiting>
    where
        InnerFut: Future + Unpin,
    {
        FutureBuilder::<OffloadWithFutureStrat, InnerFut, NeedsOffload, NeedsWhileWaiting> {
            strategy: OffloadWithFutureStrat,
            inner_fut,
            offload: NeedsOffload,
            while_waiting: NeedsWhileWaiting,
        }
    }
}

/// The sub-builder still needs a call to [`get_offload_fn()`]
///
/// [`get_offload_fn()`]: crate::future::with::OffloadWith::get_offload_fn()
#[derive(Debug)]
pub struct NeedsGetOffloadFn;
/// Sub-builder has a get offload function loaded
#[derive(Debug)]
pub struct HasGetOffloadFn<GetOffloadFn>(GetOffloadFn);

impl OffloadWith<NeedsGetOffloadFn, NeedsIncorporateFn> {
    /// docs
    #[must_use = "doesn't do anything unless built"]
    pub fn builder() -> OffloadWith<NeedsGetOffloadFn, NeedsIncorporateFn> {
        OffloadWith {
            get_offload_fn: NeedsGetOffloadFn,
            incorporate_fn: NeedsIncorporateFn,
        }
    }
}

impl<IncorporateFn> OffloadWith<NeedsGetOffloadFn, IncorporateFn> {
    /// A function to call after polling the inner future, to see
    /// if any work is available. Returns the offloaded work future,
    /// already passed into [`vacation::execute()`].
    ///
    /// This will generally require that you wrap the work in `Box::new(Box::pin())`.
    ///
    /// # Examples
    /// ```
    /// # type InnerFut = Box<std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>>;
    /// # #[tokio::main]
    /// # async fn run() {
    /// let builder = vacation::future::OffloadWith::builder()
    ///      // take a mutable reference to inner future,
    ///      // and retrieve any work to offload
    ///      .get_offload_fn(|_inner_fut: &mut InnerFut| {
    ///          // it could be conditional, but here it's always returning work
    ///          Ok(Some(Box::pin(vacation::execute(
    ///              || std::thread::sleep(std::time::Duration::from_millis(50)),
    ///              vacation::ChanceOfBlocking::High
    ///          ))))
    ///      });
    /// # }
    /// ```
    ///
    /// [`vacation::execute()`]: crate::execute()
    #[must_use = "doesn't do anything unless built"]
    pub fn get_offload_fn<GetOffloadFn, OffloadResult, InnerFut>(
        self,
        get_offload_fn: GetOffloadFn,
    ) -> OffloadWith<HasGetOffloadFn<GetOffloadFn>, IncorporateFn>
    where
        GetOffloadFn: Fn(
            &mut InnerFut,
        ) -> Result<
            Option<Pin<Box<dyn Future<Output = Result<OffloadResult, crate::Error>> + Send>>>,
            InnerFut::Output,
        >,
        OffloadResult: Send + 'static,
        InnerFut: Future + Unpin,
    {
        OffloadWith {
            get_offload_fn: HasGetOffloadFn(get_offload_fn),
            incorporate_fn: self.incorporate_fn,
        }
    }
}

impl<GetOffloadFn> OffloadWith<GetOffloadFn, NeedsIncorporateFn> {
    /// A function to call with the result of the offloaded work,
    /// to handle executor errors and do any post processing.
    ///
    /// Errors returned by this function will be bubbled up, aborting
    /// the inner future.
    ///
    /// # Examples:
    ///
    /// ```
    /// # type InnerFut = Box<std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>>;
    /// # #[tokio::main]
    /// # async fn run() {
    /// let builder = vacation::future::OffloadWith::builder()
    ///      // take a mutable reference to inner future,
    ///      // and retrieve any work to offload
    ///      .get_offload_fn(|_inner_fut: &mut InnerFut| {
    ///          // it could be conditional, but here it's always returning work
    ///          Ok(Some(Box::pin(vacation::execute(
    ///              || std::thread::sleep(std::time::Duration::from_millis(50)),
    ///              vacation::ChanceOfBlocking::High
    ///          ))))
    ///      })
    ///      .incorporate_fn(|_inner_fut: &mut InnerFut, res: Result<bool, vacation::Error>| {
    ///           println!("work complete: {res:#?}");
    ///           Ok(())
    ///       });
    /// # }
    /// ```
    #[must_use = "doesn't do anything unless built"]
    pub fn incorporate_fn<IncorporateFn, OffloadResult, InnerFut>(
        self,
        incorporate_fn: IncorporateFn,
    ) -> OffloadWith<GetOffloadFn, HasIncorporateFn<IncorporateFn>>
    where
        IncorporateFn:
            Fn(&mut InnerFut, Result<OffloadResult, crate::Error>) -> Result<(), InnerFut::Output>,
        OffloadResult: Send + 'static,
        InnerFut: Future + Unpin,
    {
        OffloadWith {
            get_offload_fn: self.get_offload_fn,
            incorporate_fn: HasIncorporateFn(incorporate_fn),
        }
    }
}

impl<InnerFut, WhileWaiting>
    FutureBuilder<OffloadWithFutureStrat, InnerFut, NeedsOffload, WhileWaiting>
{
    /// Accepts an [`OffloadWith`] builder with a get offload function loaded, as well
    /// as an optional incorporator function
    #[must_use = "doesn't do anything unless built"]
    pub fn offload_with<GetOffloadFn, IncorporateFn>(
        self,
        offload: OffloadWith<HasGetOffloadFn<GetOffloadFn>, IncorporateFn>,
    ) -> FutureBuilder<
        OffloadWithFutureStrat,
        InnerFut,
        OffloadWith<HasGetOffloadFn<GetOffloadFn>, IncorporateFn>,
        WhileWaiting,
    >
    where
        InnerFut: Future + Unpin,
    {
        FutureBuilder::<
            OffloadWithFutureStrat,
            InnerFut,
            OffloadWith<HasGetOffloadFn<GetOffloadFn>, IncorporateFn>,
            WhileWaiting,
        > {
            strategy: self.strategy,
            inner_fut: self.inner_fut,
            offload,
            while_waiting: self.while_waiting,
        }
    }
}

impl<InnerFut, Offload>
    FutureBuilder<OffloadWithFutureStrat, InnerFut, Offload, NeedsWhileWaiting>
{
    /// Specify behavior while vacation work is being actively driven.
    ///
    /// Either skip polling the inner until vacation work is complete,
    /// or continue polling the inner future alongside
    /// the vacation future.
    ///
    /// If using [`WhileWaitingMode::SuppressInnerPoll`], note that there is a deadlock risk of the offloaded
    /// work completion relies on the inner future making progress.
    #[must_use = "doesn't do anything unless built"]
    pub fn while_waiting_for_offload(
        self,
        while_waiting: WhileWaitingMode,
    ) -> FutureBuilder<OffloadWithFutureStrat, InnerFut, Offload, WhileWaitingMode> {
        FutureBuilder::<OffloadWithFutureStrat, InnerFut, Offload, WhileWaitingMode> {
            strategy: self.strategy,
            inner_fut: self.inner_fut,
            offload: self.offload,
            while_waiting,
        }
    }
}

impl<InnerFut, GetOffloadFn, IncorporateFn>
    FutureBuilder<
        OffloadWithFutureStrat,
        InnerFut,
        OffloadWith<HasGetOffloadFn<GetOffloadFn>, HasIncorporateFn<IncorporateFn>>,
        WhileWaitingMode,
    >
{
    #[must_use = "doesn't do anything unless polled"]
    /// Finish building [`OffloadWithFuture`]
    pub fn build<OffloadResult>(
        self,
    ) -> OffloadWithFuture<InnerFut, GetOffloadFn, OffloadResult, IncorporateFn> {
        OffloadWithFuture {
            inner_fut: self.inner_fut,
            get_offload_fn: self.offload.get_offload_fn.0,
            incorporate_fn: self.offload.incorporate_fn.0,
            offload_fut: None,
            while_waiting: self.while_waiting,
        }
    }
}

impl<InnerFut: Future, GetOffloadFn, OffloadResult, IncorporateFn>
    OffloadWithFuture<InnerFut, GetOffloadFn, OffloadResult, IncorporateFn>
{
    /// Helper fn to poll the offloaded future and call its incorporator as needed
    ///
    /// Updates self to store the future again if it's not complete
    fn poll_offloaded_work(
        &mut self,
        mut offload_fut: Pin<Box<dyn Future<Output = Result<OffloadResult, crate::Error>> + Send>>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), InnerFut::Output>>
    where
        IncorporateFn:
            Fn(&mut InnerFut, Result<OffloadResult, crate::Error>) -> Result<(), InnerFut::Output>,
    {
        if let Poll::Ready(res) = Pin::new(&mut offload_fut).poll(cx) {
            // if work completes, call the incorporate function with it
            match (self.incorporate_fn)(&mut self.inner_fut, res) {
                Ok(()) => Poll::Ready(Ok(())),
                // bubble any errors up
                Err(err) => Poll::Ready(Err(err)),
            }
        } else {
            self.offload_fut = Some(offload_fut);
            Poll::Pending
        }
    }
}

impl<InnerFut, GetOffloadFn, OffloadResult, IncorporateFn> Future
    for OffloadWithFuture<InnerFut, GetOffloadFn, OffloadResult, IncorporateFn>
where
    InnerFut: Future + Unpin,
    GetOffloadFn: Fn(
        &mut InnerFut,
    ) -> Result<
        Option<Pin<Box<dyn Future<Output = Result<OffloadResult, crate::Error>> + Send>>>,
        InnerFut::Output,
    >,
    OffloadResult: Send + 'static,
    IncorporateFn:
        Fn(&mut InnerFut, Result<OffloadResult, crate::Error>) -> Result<(), InnerFut::Output>,
{
    type Output = InnerFut::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        // first execute any vacation work
        if let Some(offload_fut) = this.offload_fut.take() {
            match this.poll_offloaded_work(offload_fut, cx) {
                // if we are done, clean up state
                Poll::Ready(Ok(())) => this.offload_fut = None,
                // bubble up any errors
                Poll::Ready(Err(err)) => return Poll::Ready(err),
                _ => (),
            }
        }

        if this.offload_fut.is_none()
            || this.while_waiting == WhileWaitingMode::PassThroughInnerPoll
        {
            if let Poll::Ready(res) = Pin::new(&mut this.inner_fut).poll(cx) {
                // if inner is ready, we drop any active vacation work (at least on this task's side) and return inner's result
                return Poll::Ready(res);
            }
        }

        // we only prepare new vacation work if there is none loaded already
        // TODO: consider allowing a vec of multiple work units
        if this.offload_fut.is_none() {
            match (this.get_offload_fn)(&mut this.inner_fut) {
                // bubble up any errors
                Err(err) => return Poll::Ready(err),
                Ok(Some(offload_fut)) => {
                    // poll the work once and bubble up any errors
                    // poll_offloaded_work updates the state for us if the offloaded future is still pending
                    if let Poll::Ready(Err(err)) = this.poll_offloaded_work(offload_fut, cx) {
                        return Poll::Ready(err);
                    }
                }
                // no work = do nothing
                _ => (),
            }
        }

        Poll::Pending
    }
}

// these tests all use the default executor which is `ExecuteDirectly`
#[cfg(test)]
mod test {
    use std::{future::Future, time::Duration};

    use tokio::sync::mpsc::Sender;

    use crate::{
        future::{
            test::{TestFuture, TestFutureResponse},
            WhileWaitingMode,
        },
        ChanceOfBlocking,
    };

    use super::*;

    /// Wraps a [`TestFuture`] in vacation handling
    /// to send to a mpsc channel each stage's call, or
    /// return an error if that channel is dropped.
    fn vacation_fut(
        inner_fut: TestFuture,
        get_tx: Sender<()>,
        work_tx: Sender<()>,
        incorporate_tx: Sender<()>,
        while_waiting: WhileWaitingMode,
    ) -> OffloadWithFuture<
        TestFuture,
        impl Fn(
            &mut TestFuture,
        ) -> Result<
            Option<
                Pin<
                    Box<
                        dyn Future<Output = Result<Result<(), TestFutureResponse>, crate::Error>>
                            + Send,
                    >,
                >,
            >,
            TestFutureResponse,
        >,
        Result<(), TestFutureResponse>,
        impl Fn(
            &mut TestFuture,
            Result<Result<(), TestFutureResponse>, crate::Error>,
        ) -> Result<(), TestFutureResponse>,
    > {
        crate::future::builder()
            .future(inner_fut)
            .offload_with(
                OffloadWith::builder()
                .get_offload_fn(move |_| {
                    if get_tx.clone().try_send(()).is_err() {
                        return Err(TestFutureResponse::VacationGetError);
                    }

                    let tx = work_tx.clone();
                    let closure = move || {
                        if tx.try_send(()).is_err() {
                            return Err(TestFutureResponse::VacationWorkError);
                        }
                        Ok(())
                    };

                    let offload_fut = crate::execute(closure, ChanceOfBlocking::High);

                    Ok(Some(Box::pin(offload_fut)))
                })
                .incorporate_fn(
                    move |_inner: &mut TestFuture,
                    res: Result<Result<(), TestFutureResponse>, crate::Error>| {
                  if res.is_err() {
                      return Err(TestFutureResponse::ExecutorError);
                  }

                  if res.unwrap().is_err() {
                      return Err(TestFutureResponse::VacationWorkError);
                  }

                  if incorporate_tx.clone().try_send(()).is_err() {
                      return Err(TestFutureResponse::VacationIncorporateError);
                  }

                  Ok(())
                }
                )
            )
            .while_waiting_for_offload(while_waiting)
            .build()
    }

    #[tokio::test]
    async fn it_works() {
        let inner_fut = TestFuture::new(3);

        let (get_tx, get_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (work_tx, work_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (incorporate_tx, incorporate_rx) = tokio::sync::mpsc::channel::<()>(5);

        let future = vacation_fut(
            inner_fut,
            get_tx,
            work_tx,
            incorporate_tx,
            WhileWaitingMode::PassThroughInnerPoll,
        );

        let res = future.await;

        // gets work on first poll,
        // executes work on second poll + gets more work
        // executes work on third poll, then finishes inner future and returns before getting more work
        assert_eq!(res, TestFutureResponse::Success(3));
        assert_eq!(2, get_rx.len());
        assert_eq!(2, work_rx.len());
        assert_eq!(2, incorporate_rx.len());
    }

    #[tokio::test]
    async fn inner_error() {
        let inner_fut = TestFuture::new_with_err(3, 0);

        let (get_tx, get_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (work_tx, work_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (incorporate_tx, incorporate_rx) = tokio::sync::mpsc::channel::<()>(5);

        let future = vacation_fut(
            inner_fut,
            get_tx,
            work_tx,
            incorporate_tx,
            WhileWaitingMode::PassThroughInnerPoll,
        );

        let res = future.await;
        //

        // first poll fails, no work picked up or executed
        assert_eq!(res, TestFutureResponse::InnerError(1));
        assert_eq!(0, get_rx.len());
        assert_eq!(0, work_rx.len());
        assert_eq!(0, incorporate_rx.len());
    }

    #[tokio::test]
    async fn delayed_inner_error() {
        let inner_fut = TestFuture::new_with_err(3, 1);

        let (get_tx, get_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (work_tx, work_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (incorporate_tx, incorporate_rx) = tokio::sync::mpsc::channel::<()>(5);

        let future = vacation_fut(
            inner_fut,
            get_tx,
            work_tx,
            incorporate_tx,
            WhileWaitingMode::PassThroughInnerPoll,
        );

        let res = future.await;

        // first poll succeeds, new work from get
        // vacation work executes and resolves, then inner poll fails, no new get
        assert_eq!(res, TestFutureResponse::InnerError(2));
        assert_eq!(1, get_rx.len());
        assert_eq!(1, work_rx.len());
        assert_eq!(1, incorporate_rx.len());
    }

    #[tokio::test]
    async fn incorporate_err() {
        let inner_fut: TestFuture = TestFuture::new(3);

        let (get_tx, get_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (work_tx, work_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (incorporate_tx, incorporate_rx) = tokio::sync::mpsc::channel::<()>(5);

        let future = vacation_fut(
            inner_fut,
            get_tx,
            work_tx,
            incorporate_tx,
            WhileWaitingMode::PassThroughInnerPoll,
        );

        std::mem::drop(incorporate_rx);

        let res = future.await;

        // first poll succeeds and gathers new work
        // execute work succeeds, then resolve fails
        assert_eq!(res, TestFutureResponse::VacationIncorporateError);
        assert_eq!(1, get_rx.len());
        assert_eq!(1, work_rx.len());
    }

    #[tokio::test]
    async fn work_err() {
        let inner_fut: TestFuture = TestFuture::new(2);

        let (get_tx, get_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (work_tx, work_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (incorporate_tx, incorporate_rx) = tokio::sync::mpsc::channel::<()>(5);

        let future = vacation_fut(
            inner_fut,
            get_tx,
            work_tx,
            incorporate_tx,
            WhileWaitingMode::PassThroughInnerPoll,
        );

        std::mem::drop(work_rx);

        let res = future.await;

        // first poll succeeds and gathers new work
        // then execute fails
        assert_eq!(res, TestFutureResponse::VacationWorkError);
        assert_eq!(1, get_rx.len());
        assert_eq!(0, incorporate_rx.len());
    }

    #[tokio::test]
    async fn work_err_but_inner_ready() {
        let inner_fut: TestFuture = TestFuture::new(1);

        let (get_tx, get_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (work_tx, work_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (incorporate_tx, incorporate_rx) = tokio::sync::mpsc::channel::<()>(5);

        let future = vacation_fut(
            inner_fut,
            get_tx,
            work_tx,
            incorporate_tx,
            WhileWaitingMode::PassThroughInnerPoll,
        );

        std::mem::drop(work_rx);

        let res = future.await;

        // first poll succeeds so we never get or execute the failing work
        assert_eq!(res, TestFutureResponse::Success(1));
        assert_eq!(0, get_rx.len());
        assert_eq!(0, incorporate_rx.len());
    }

    #[tokio::test]
    async fn get_err() {
        let inner_fut: TestFuture = TestFuture::new(2);

        let (get_tx, get_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (work_tx, work_rx) = tokio::sync::mpsc::channel::<()>(5);
        let (incorporate_tx, incorporate_rx) = tokio::sync::mpsc::channel::<()>(5);

        let future = vacation_fut(
            inner_fut,
            get_tx,
            work_tx,
            incorporate_tx,
            WhileWaitingMode::PassThroughInnerPoll,
        );

        std::mem::drop(get_rx);

        let res = future.await;

        // first poll succeeds and then get work fails
        assert_eq!(res, TestFutureResponse::VacationGetError);
        assert_eq!(0, incorporate_rx.len());
        assert_eq!(0, work_rx.len());
    }

    // we test suppressing the inner poll in the integ tests, since we can't block
    // and return pending without spawn_blocking strategy

    #[tokio::test]
    async fn supress_inner_poll_works() {
        // 2 units of work means one poll, then we get work, and then next poll we finish...
        // except that we suppress inner poll, so vacation finishes first despite being slower
        let inner_fut = TestFuture::new(3);

        let future = crate::future::builder()
            .future(inner_fut)
            .offload_with(
                OffloadWith::builder()
                    .get_offload_fn(|_: &mut TestFuture| {
                        Ok(Some(Box::pin(crate::execute(
                            || std::thread::sleep(Duration::from_millis(50)),
                            ChanceOfBlocking::High,
                        ))))
                    })
                    .incorporate_fn(|_: &mut TestFuture, _: Result<(), crate::Error>| {
                        Err(TestFutureResponse::VacationIncorporateError)
                    }),
            )
            .while_waiting_for_offload(WhileWaitingMode::SuppressInnerPoll)
            .build::<()>();

        let res = future.await;

        // if the inner future completed, we would have success, but instead we delay polling and return
        // an error after the slower vacation work finishes

        // gets work on first poll,
        // executes work on second poll + gets more work
        // executes work on third poll, then finishes inner future and returns before getting more work
        assert_eq!(res, TestFutureResponse::VacationIncorporateError);
    }
}
