use std::{future::Future, pin::Pin};

use tokio::sync::mpsc::Sender;

use crate::{
    concurrency_limit::ConcurrencyLimit,
    error::{Error, InvalidConfig},
    make_future_cancellable, ComputeHeavyFutureExecutor, ExecutorStrategyImpl,
    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY,
};

const DEFAULT_NICENESS: i8 = 10;
const DEFAULT_CHANNEL_SIZE: usize = 10;

fn default_thread_count() -> usize {
    num_cpus::get()
}

/// Extention of [`GlobalStrategyBuilder`] for a customized secondary tokio runtime strategy.
///
/// Requires calling [`SecondaryTokioRuntimeStrategyBuilder::initialize()`] to
/// initialize the strategy.
///
/// # Example
///
/// ```
/// use compute_heavy_future_executor::global_strategy_builder;
/// use compute_heavy_future_executor::spawn_compute_heavy_future;
///
/// # async fn run() {
/// global_strategy_builder().unwrap().secondary_tokio_runtime_builder()
///     .niceness(1).unwrap()
///     .thread_count(2)
///     .channel_size(3)
///     .max_concurrency(4)
///     .initialize()
///     .unwrap();
/// # }
/// ```
#[must_use]
#[derive(Default)]
pub struct SecondaryTokioRuntimeStrategyBuilder {
    niceness: Option<i8>,
    thread_count: Option<usize>,
    channel_size: Option<usize>,
    // passed down from the parent `GlobalStrategy` builder, not modified internally
    max_concurrency: Option<usize>,
}

impl SecondaryTokioRuntimeStrategyBuilder {
    pub(crate) fn new(max_concurrency: Option<usize>) -> Self {
        Self {
            max_concurrency,
            ..Default::default()
        }
    }
}

impl SecondaryTokioRuntimeStrategyBuilder {
    /// Set the thread niceness for the secondary runtime's worker threads,
    /// which on linux is used to increase or lower relative
    /// OS scheduling priority.
    ///
    /// Allowed values are -20..=19
    ///
    /// ## Default
    ///
    /// The default value is 10.
    pub fn niceness(self, niceness: i8) -> Result<Self, Error> {
        // please https://github.com/rust-lang/rfcs/issues/671
        if !(-20..=19).contains(&niceness) {
            return Err(Error::InvalidConfig(InvalidConfig {
                field: "niceness",
                received: niceness.to_string(),
                allowed: "-20..=19",
            }));
        }

        Ok(Self {
            niceness: Some(niceness),
            ..self
        })
    }

    /// Set the count of worker threads in the secondary tokio runtime.
    ///
    /// ## Default
    ///
    /// The default value is the number of cpu cores
    pub fn thread_count(self, thread_count: usize) -> Self {
        Self {
            thread_count: Some(thread_count),
            ..self
        }
    }

    /// Set the buffer size of the channel used to spawn tasks
    /// in the background executor.
    ///
    /// ## Default
    ///
    /// The default value is 10
    pub fn channel_size(self, channel_size: usize) -> Self {
        Self {
            channel_size: Some(channel_size),
            ..self
        }
    }

    /// Set the max number of simultaneous futures processed by this executor.
    ///
    /// Yes, the future is dropped if the caller drops the returned future from
    ///[`spawn_compute_heavy_future()`].
    ///
    /// ## Default
    /// No maximum concurrency
    pub fn max_concurrency(self, max_task_concurrency: usize) -> Self {
        Self {
            max_concurrency: Some(max_task_concurrency),
            ..self
        }
    }

    pub fn initialize(self) -> Result<(), Error> {
        let niceness = self.niceness.unwrap_or(DEFAULT_NICENESS);
        let thread_count = self.thread_count.unwrap_or_else(|| default_thread_count());
        let channel_size = self.channel_size.unwrap_or(DEFAULT_CHANNEL_SIZE);

        log::info!("initializing compute-heavy executor with secondary tokio runtime strategy \
        and niceness {niceness}, thread_count {thread_count}, channel_size {channel_size}, max concurrency {:#?}", self.max_concurrency);

        let executor = SecondaryTokioRuntimeExecutor::new(
            niceness,
            thread_count,
            channel_size,
            self.max_concurrency,
        );

        COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
            .set(ExecutorStrategyImpl::SecondaryTokioRuntime(executor))
            .map_err(|_| {
                Error::AlreadyInitialized(
                    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get().unwrap().into(),
                )
            })
    }
}

type BackgroundFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub(crate) struct SecondaryTokioRuntimeExecutor {
    tx: Sender<BackgroundFuture>,
    concurrency_limit: ConcurrencyLimit,
}

impl SecondaryTokioRuntimeExecutor {
    pub(crate) fn new(
        niceness: i8,
        thread_count: usize,
        channel_size: usize,
        max_concurrency: Option<usize>,
    ) -> Self {
        // channel is only for routing work to new task::spawn so should be very quick
        let (tx, mut rx) = tokio::sync::mpsc::channel(channel_size);

        std::thread::Builder::new()
    .name("compute-heavy-executor".to_string())
    .spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .thread_name("compute-heavy-executor-pool-thread")
            .worker_threads(thread_count)
            .on_thread_start(move || unsafe {
                // Reduce thread pool thread niceness, so they are lower priority
                // than the foreground executor and don't interfere with I/O tasks
                #[cfg(target_os = "linux")]
                {
                    *libc::__errno_location() = 0;
                    if libc::nice(niceness.into()) == -1 && *libc::__errno_location() != 0 {
                        let error = std::io::Error::last_os_error();
                        log::error!("failed to set threadpool niceness of secondary compute-heavy tokio executor: {}", error);
                    }
                }
            })
            .enable_all()
            .build()
            .unwrap_or_else(|e| panic!("cpu heavy runtime failed_to_initialize: {}", e));

        rt.block_on(async {
            log::debug!("starting to process work on secondary compute-heavy tokio executor");

            while let Some(work) = rx.recv().await {
                tokio::task::spawn(async move {
                    work.await
                });
            }
        });
        log::warn!("exiting secondary compute heavy tokio runtime because foreground channel closed");
    })
    .unwrap_or_else(|e| panic!("secondary compute-heavy runtime thread failed_to_initialize: {}", e));

        Self {
            tx,
            concurrency_limit: ConcurrencyLimit::new(max_concurrency),
        }
    }
}

impl ComputeHeavyFutureExecutor for SecondaryTokioRuntimeExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: std::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let _permit = self.concurrency_limit.acquire_permit().await;

        let (wrapped_future, rx) = make_future_cancellable(fut);

        match self.tx.send(Box::pin(wrapped_future)).await {
            Ok(_) => (),
            Err(err) => {
                panic!("secondary compute-heavy runtime channel cannot be reached: {err}")
            }
        }

        rx.await.map_err(|err| Error::RecvError(err))

        // permit implicitly drops
    }
}
