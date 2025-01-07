/// Contains [`OffloadFirstFuture`], used to resolve one round of compute-heavy work with vacation, and then construct and poll an inner future
///
/// [`OffloadFirstFuture`]: crate::future::first::OffloadFirstFuture
pub mod first;
/// Contains [`OffloadWithFuture`], for retrieving and running vacation work alongside polling an inner future
///
/// [`OffloadWithFuture`]: crate::future::with::OffloadWithFuture
pub mod with;

pub use first::{OffloadFirst, OffloadFirstFuture};
pub use with::{OffloadWith, OffloadWithFuture};

/// Needs a call to [`future()`] (for [`OffloadWithFuture`])
/// or [`offload_first()`] (for [`OffloadFirstFuture`])
///
/// [`future()`]: crate::future::FutureBuilder::future()
/// [`offload_first()`]: crate::future::FutureBuilder::offload_first()
/// [`OffloadWithFuture`]: crate::future::with::OffloadWithFuture
/// [`OffloadFirstFuture`]: crate::future::first::OffloadFirstFuture
#[derive(Debug)]
pub struct NeedsStrategy;
/// Builder will construct [`OffloadFirstFuture`]
///
/// [`OffloadFirstFuture`]: crate::future::first::OffloadFirstFuture
#[derive(Debug)]
pub struct OffloadFirstStrat;
/// Builder will construct [`OffloadWithFuture`]
///
/// [`OffloadWithFuture`]: crate::future::with::OffloadWithFuture
#[derive(Debug)]
pub struct OffloadWithFutureStrat;

/// Needs a call to [`future()`] (for [`OffloadWithFuture`])
/// or [`offload_first()`] (for [`OffloadFirstFuture`])
///
/// [`future()`]: crate::future::FutureBuilder::future()
/// [`offload_first()`]: crate::future::FutureBuilder::offload_first()
/// [`OffloadWithFuture`]: crate::future::with::OffloadWithFuture
/// [`OffloadFirstFuture`]: crate::future::first::OffloadFirstFuture
#[derive(Debug)]
pub struct NeedsInnerFuture;
/// Builder will construct [`OffloadFirstFuture`], which does not wrap a future
/// directly.
///
/// [`OffloadFirstFuture`]: crate::future::first::OffloadFirstFuture

#[derive(Debug)]
pub struct NoInnerFuture;

/// Builder needs a call to [`offload_with()`] or [`offload_first()`]
///
/// [`offload_with()`]: crate::future::FutureBuilder::offload_with()
/// [`offload_first()`]: crate::future::FutureBuilder::offload_first()
#[derive(Debug)]
pub struct NeedsOffload;

/// Sub-builder needs a call to [`OffloadWith::incorporate_fn()`]
/// or [`OffloadFirst::incorporate_fn()`]
///
///  [`OffloadWith::incorporate_fn()`]: crate::future::with::OffloadWith::incorporate_fn()
/// [`OffloadFirst::incorporate_fn()`]: crate::future::first::OffloadFirst::incorporate_fn()
#[derive(Debug)]
pub struct NeedsIncorporateFn;
/// Sub-builder has an incorporate function loaded
#[derive(Debug)]
pub struct HasIncorporateFn<IncorporateFn>(IncorporateFn);

/// Builder needs to know whether it should poll the inner future
/// while offloaded vacation work is ongoing, or wait until that offloaded
/// work completes.
///
/// Use [`FutureBuilder::while_waiting_for_offload()`] to specify.
///
/// [`FutureBuilder::while_waiting_for_offload()`]: crate::future::FutureBuilder::while_waiting_for_offload()
#[derive(Debug)]
pub struct NeedsWhileWaiting;

/// A builder struct with which to construct [`OffloadFirst`] or [`OffloadWithFuture`] futures.
///
/// Start with [`vacation::future::builder()`].
///
/// [`OffloadFirst`]: crate::future::first::OffloadFirst
/// [`OffloadWithFuture`]: crate::future::with::OffloadWithFuture
/// [`vacation::future::builder()`]: crate::future::builder()
#[derive(Debug)]
pub struct FutureBuilder<Strategy, InnerFut, Offload, WhileWaiting> {
    strategy: Strategy,
    inner_fut: InnerFut,
    offload: Offload,
    while_waiting: WhileWaiting,
}

/// The mode to use when offload work is active in vacation.
///
/// Either skip polling the inner until vacation work is complete,
/// or continue polling the inner future alongside
/// the vacation future.
///
/// If using [`WhileWaitingMode::SuppressInnerPoll`], note that there is a deadlock risk of the offloaded
/// work completion relies on the inner future making progress.
#[derive(PartialEq, Debug)]
pub enum WhileWaitingMode {
    /// Skip polling the inner future as long as there is vacation work outstanding
    ///
    ///  Note that there is a deadlock risk of the offloaded
    /// work completion relies on the inner future making progress.
    SuppressInnerPoll,
    /// Continue polling the inner future while there is vacation work active.
    PassThroughInnerPoll,
}

/// Get a builder that constructs a [`OffloadFirstFuture`] or [`OffloadWithFuture`] wrapper future
///
/// # Examples
///
/// ## OffloadFirstFuture
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
///
///  ## OffloadWith
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
///
/// [`OffloadFirstFuture`]: crate::future::first::OffloadFirstFuture
/// [`OffloadWithFuture`]: crate::future::with::OffloadWithFuture
#[must_use = "doesn't do anything unless built"]
pub fn builder() -> FutureBuilder<NeedsStrategy, NeedsInnerFuture, NeedsOffload, NeedsWhileWaiting>
{
    FutureBuilder {
        strategy: NeedsStrategy,
        inner_fut: NeedsInnerFuture,
        offload: NeedsOffload,
        while_waiting: NeedsWhileWaiting,
    }
}

#[cfg(test)]
pub(crate) mod test {
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    };

    #[derive(PartialEq)]
    pub(crate) enum TestFutureResponse {
        Success(usize),
        InnerError(usize),
        VacationGetError,
        VacationWorkError,
        VacationIncorporateError,
        ExecutorError,
    }

    impl std::fmt::Debug for TestFutureResponse {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Success(arg0) => f.debug_tuple("Success").field(arg0).finish(),
                Self::InnerError(arg0) => f.debug_tuple("InnerError").field(arg0).finish(),
                Self::VacationGetError => write!(f, "VacationGetError"),
                Self::VacationWorkError => write!(f, "VacationWorkError"),
                Self::VacationIncorporateError => write!(f, "VacationIncorporateError"),
                Self::ExecutorError => write!(f, "ExecutorError"),
            }
        }
    }

    pub(crate) struct TestFuture {
        work: usize,
        poll_count: usize,
        error_after_n_polls: Option<usize>,
    }

    impl TestFuture {
        pub(crate) fn new(work: usize) -> Self {
            Self {
                work,
                error_after_n_polls: None,
                poll_count: 0,
            }
        }

        pub(crate) fn new_with_err(work: usize, poll_count: usize) -> Self {
            Self {
                work,
                error_after_n_polls: Some(poll_count),
                poll_count: 0,
            }
        }

        /// 0 means immediately
        pub(crate) fn error_after(&mut self, poll_count: usize) {
            self.error_after_n_polls = Some(poll_count)
        }
    }

    impl Future for TestFuture {
        type Output = TestFutureResponse;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            self.poll_count += 1;

            if let Some(poll_count) = self.error_after_n_polls {
                if poll_count < self.poll_count {
                    return Poll::Ready(TestFutureResponse::InnerError(self.poll_count));
                }
            }

            self.work -= 1;
            match self.work {
                remaining if remaining > 0 => {
                    // tell executor to immediately poll us again,
                    // which passes through our vacation future as well
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                _ => Poll::Ready(TestFutureResponse::Success(self.poll_count)),
            }
        }
    }
}
