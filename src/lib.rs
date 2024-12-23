#[cfg(feature = "tokio_block_in_place")]
mod block_in_place;
mod concurrency_limit;
mod current_context;
mod custom_executor;
pub mod error;
#[cfg(feature = "secondary_tokio_runtime")]
mod secondary_tokio_runtime;
#[cfg(feature = "tokio")]
mod spawn_blocking;

pub use custom_executor::CustomExecutorClosure;
pub use error::Error;
#[cfg(feature = "secondary_tokio_runtime")]
pub use secondary_tokio_runtime::SecondaryTokioRuntimeStrategyBuilder;

#[cfg(feature = "tokio_block_in_place")]
use block_in_place::BlockInPlaceExecutor;
use current_context::CurrentContextExecutor;
use custom_executor::CustomExecutor;
#[cfg(feature = "secondary_tokio_runtime")]
use secondary_tokio_runtime::SecondaryTokioRuntimeExecutor;
#[cfg(feature = "tokio")]
use spawn_blocking::SpawnBlockingExecutor;

use std::{fmt::Debug, future::Future, sync::OnceLock};

use tokio::{select, sync::oneshot::Receiver};

// TODO: module docs, explain the point of this library, give some samples

/// Initialize a builder to set the global compute heavy future
/// executor strategy.
#[must_use = "doesn't do anything unless used"]
pub fn global_strategy_builder() -> GlobalStrategyBuilder {
    GlobalStrategyBuilder::default()
}

/// Get the currently initialized strategy, or the default strategy for the
/// current feature and runtime type in case no strategy has been loaded.
pub fn global_strategy() -> CurrentStrategy {
    match COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get() {
        Some(strategy) => CurrentStrategy::Initialized(strategy.into()),
        None => CurrentStrategy::Default(<&ExecutorStrategyImpl>::default().into()),
    }
}

#[must_use = "doesn't do anything unless used"]#[derive(Default)]
pub struct GlobalStrategyBuilder {
    max_concurrency: Option<usize>,
}

impl GlobalStrategyBuilder {
    /// Set the max number of simultaneous futures processed by this executor.
    ///
    /// If this number is exceeded, the futures sent to
    /// [`execute_compute_heavy_future()`] will sleep until a permit
    /// can be acquired.
    ///
    /// ## Default
    /// No maximum concurrency
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    ///
    /// # async fn run() {
    /// global_strategy_builder()
    ///         .max_concurrency(10)
    ///         .initialize_current_context()
    ///         .unwrap();
    /// # }
    pub fn max_concurrency(self, max_task_concurrency: usize) -> Self {
        Self {
            max_concurrency: Some(max_task_concurrency),
            ..self
        }
    }

    /// Initializes a new global strategy to wait in the current context.
    ///
    /// This is effectively a non-op wrapper that adds no special handling for the future besides optional concurrency control.
    /// This is the default if the `tokio` feature is disabled.
    ///
    /// # Cancellation
    /// Yes, the future is dropped if the caller drops the returned future from
    ///[`execute_compute_heavy_future()`].
    ///
    /// Note that it will only be dropped across yield points in the case of long-blocking futures.
    ///
    /// ## Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::execute_compute_heavy_future;
    ///
    /// # async fn run() {
    /// global_strategy_builder().initialize_current_context().unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = execute_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    /// ```
    pub fn initialize_current_context(self) -> Result<(), Error> {
        let strategy =
            ExecutorStrategyImpl::CurrentContext(CurrentContextExecutor::new(self.max_concurrency));
        set_strategy(strategy)
    }

    /// Initializes a new global strategy to execute futures by blocking on them inside the
    /// tokio blocking threadpool. This is the default strategy if none is explicitly initialized,
    /// if the `tokio` feature is enabled.
    ///
    /// By default, tokio will spin up a blocking thread
    /// per task, which may be more than your count of CPU cores, depending on runtime config.
    ///
    /// If you expect many concurrent cpu-heavy futures, consider limiting your blocking
    /// tokio threadpool size.
    /// Or, you can use a heavier weight strategy like [`initialize_secondary_tokio_runtime()`].
    ///
    /// # Cancellation
    /// Yes, the future is dropped if the caller drops the returned future
    /// from [`execute_compute_heavy_future()`].
    ///
    /// Note that it will only be dropped across yield points in the case of long-blocking futures.
    ///
    /// ## Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::execute_compute_heavy_future;
    ///
    /// # async fn run() {
    /// global_strategy_builder().initialize_spawn_blocking().unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = execute_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    /// ```
    #[cfg(feature = "tokio")]
    pub fn initialize_spawn_blocking(self) -> Result<(), Error> {
        let strategy =
            ExecutorStrategyImpl::SpawnBlocking(SpawnBlockingExecutor::new(self.max_concurrency));
        set_strategy(strategy)
    }

    /// Initializes a new global strategy to execute futures  by calling tokio::task::block_in_place
    /// on the current tokio worker thread. This evicts other tasks on same worker thread to
    /// avoid blocking them.
    ///
    /// This approach can starve your executor of worker threads if called with too many
    /// concurrent cpu-heavy futures.
    ///
    /// If you expect many concurrent cpu-heavy futures, consider a
    /// heavier weight strategy like [`initialize_secondary_tokio_runtime()`].
    ///
    /// # Cancellation
    /// No, this strategy does not allow futures to be cancelled.
    ///
    /// ## Error
    /// Returns an error if called from a context besides a tokio multithreaded runtime.
    ///
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::execute_compute_heavy_future;
    ///
    /// # async fn run() {
    /// global_strategy_builder().initialize_block_in_place().unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = execute_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    /// ```
    #[cfg(feature = "tokio_block_in_place")]
    pub fn initialize_block_in_place(self) -> Result<(), Error> {
        let strategy =
            ExecutorStrategyImpl::BlockInPlace(BlockInPlaceExecutor::new(self.max_concurrency)?);
        set_strategy(strategy)
    }

    /// Initializes a new global strategy that spins up a secondary background tokio runtime
    /// that executes futures on lower priority worker threads.
    ///
    /// This uses certain defaults, listed below. To modify these defaults,
    /// instead use [`secondary_tokio_runtime_builder()`]
    ///
    /// # Defaults
    /// ## Thread niceness
    /// The thread niceness for the secondary runtime's worker threads,
    /// which on linux is used to increase or lower relative
    /// OS scheduling priority.
    ///
    /// Default: 10
    ///
    /// ## Thread count
    /// The count of worker threads in the secondary tokio runtime.
    ///
    /// Default: CPU core count
    ///
    /// ## Channel size
    /// The buffer size of the channel used to spawn tasks
    /// in the background executor.
    ///
    /// Default: 10
    ///
    /// ## Max task concurrency
    /// The max number of simultaneous background tasks running
    ///
    /// Default: no limit
    ///
    /// # Cancellation
    /// Yes, the future is dropped if the caller drops the returned future
    /// from [`execute_compute_heavy_future()`].
    ///
    /// Note that it will only be dropped across yield points in the case of long-blocking futures.
    ///
    /// ## Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::execute_compute_heavy_future;
    ///
    /// # async fn run() {
    /// global_strategy_builder().initialize_secondary_tokio_runtime().unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = execute_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    /// ```
    #[cfg(feature = "secondary_tokio_runtime")]
    pub fn initialize_secondary_tokio_runtime(self) -> Result<(), Error> {
        self.secondary_tokio_runtime_builder().initialize()
    }

    /// Creates a [`SecondaryTokioRuntimeStrategyBuilder`] for a customized secondary tokio runtime strategy.
    ///
    /// Subsequent calls on the returned builder allow modifying defaults.
    ///
    /// The returned builder will require calling [`SecondaryTokioRuntimeStrategyBuilder::initialize()`] to
    /// ultimately load the strategy.
    ///
    /// # Cancellation
    /// Yes, the future is dropped if the caller drops the returned future
    /// from [`execute_compute_heavy_future()`].
    ///
    /// Note that it will only be dropped across yield points in the case of long-blocking futures.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::execute_compute_heavy_future;
    ///
    /// # async fn run() {
    /// global_strategy_builder()
    ///     .secondary_tokio_runtime_builder()
    ///     .niceness(1)
    ///     .thread_count(2)
    ///     .channel_size(3)
    ///     .max_concurrency(4)
    ///     .initialize()
    ///     .unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = execute_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    /// ```
    #[cfg(feature = "secondary_tokio_runtime")]
    #[must_use = "doesn't do anything unless used"]
    pub fn secondary_tokio_runtime_builder(self) -> SecondaryTokioRuntimeStrategyBuilder {
        SecondaryTokioRuntimeStrategyBuilder::new(self.max_concurrency)
    }

    /// Accepts a closure that will poll an arbitrary feature to completion.
    ///
    /// Intended for injecting arbitrary runtimes/strategies or customizing existing ones.
    ///
    /// # Cancellation
    /// Yes, the closure's returned future is dropped if the caller drops the returned future from [`execute_compute_heavy_future()`].    
    /// Note that it will only be dropped across yield points in the case of long-blocking futures.
    ///
    /// ## Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::execute_compute_heavy_future;
    /// use compute_heavy_future_executor::CustomExecutorClosure;
    ///
    /// // this isn't actually a good strategy, to be clear
    /// # async fn run() {
    /// let closure: CustomExecutorClosure = Box::new(|fut| {
    ///     Box::new(
    ///         async move {
    ///             tokio::task::spawn(async move { fut.await })
    ///             .await
    ///             .map_err(|err| err.into())
    ///         }
    ///     )
    /// });
    ///
    /// global_strategy_builder().initialize_custom_executor(closure).unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = execute_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    ///
    /// ```
    pub fn initialize_custom_executor(self, closure: CustomExecutorClosure) -> Result<(), Error> {
        let strategy = ExecutorStrategyImpl::CustomExecutor(CustomExecutor::new(
            closure,
            self.max_concurrency,
        ));
        set_strategy(strategy)
    }
}

pub(crate) fn set_strategy(strategy: ExecutorStrategyImpl) -> Result<(), Error> {
    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
        .set(strategy)
        .map_err(|_| {
            Error::AlreadyInitialized(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get().unwrap().into())
        })?;

    log::info!(
        "initialized compute-heavy future executor strategy - {:#?}",
        global_strategy()
    );

    Ok(())
}
trait ComputeHeavyFutureExecutor {
    /// Accepts a future and returns its result
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurrentStrategy {
    Default(ExecutorStrategy),
    Initialized(ExecutorStrategy),
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutorStrategy {
    /// A non-op strategy that awaits in the current context
    CurrentContext,
    /// User-provided closure
    CustomExecutor,
    /// tokio task::spawn_blocking
    #[cfg(feature = "tokio")]
    SpawnBlocking,
    /// tokio task::block_in_place
    #[cfg(feature = "tokio_block_in_place")]
    BlockInPlace,
    #[cfg(feature = "secondary_tokio_runtime")]
    /// Spin up a second, lower-priority tokio runtime
    /// that communicates via channels
    SecondaryTokioRuntime,
}

impl From<&ExecutorStrategyImpl> for ExecutorStrategy {
    fn from(value: &ExecutorStrategyImpl) -> Self {
        match value {
            ExecutorStrategyImpl::CurrentContext(_) => Self::CurrentContext,
            ExecutorStrategyImpl::CustomExecutor(_) => Self::CustomExecutor,
            #[cfg(feature = "tokio")]
            ExecutorStrategyImpl::SpawnBlocking(_) => Self::SpawnBlocking,
            #[cfg(feature = "tokio_block_in_place")]
            ExecutorStrategyImpl::BlockInPlace(_) => Self::BlockInPlace,
            #[cfg(feature = "secondary_tokio_runtime")]
            ExecutorStrategyImpl::SecondaryTokioRuntime(_) => Self::SecondaryTokioRuntime,
        }
    }
}

/// The stored strategy used to spawn compute-heavy futures.
static COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY: OnceLock<ExecutorStrategyImpl> = OnceLock::new();

#[non_exhaustive]
enum ExecutorStrategyImpl {
    /// A non-op strategy that awaits in the current context
    CurrentContext(CurrentContextExecutor),
    /// User-provided closure
    CustomExecutor(CustomExecutor),
    /// tokio task::spawn_blocking
    #[cfg(feature = "tokio")]
    SpawnBlocking(SpawnBlockingExecutor),
    /// tokio task::block_in_place
    #[cfg(feature = "tokio_block_in_place")]
    BlockInPlace(BlockInPlaceExecutor),
    #[cfg(feature = "secondary_tokio_runtime")]
    /// Spin up a second, lower-priority tokio runtime
    /// that communicates via channels
    SecondaryTokioRuntime(SecondaryTokioRuntimeExecutor),
}

impl ComputeHeavyFutureExecutor for ExecutorStrategyImpl {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        match self {
            ExecutorStrategyImpl::CurrentContext(executor) => executor.execute(fut).await,
            ExecutorStrategyImpl::CustomExecutor(executor) => executor.execute(fut).await,
            #[cfg(feature = "tokio")]
            ExecutorStrategyImpl::SpawnBlocking(executor) => executor.execute(fut).await,
            #[cfg(feature = "tokio_block_in_place")]
            ExecutorStrategyImpl::BlockInPlace(executor) => executor.execute(fut).await,
            #[cfg(feature = "secondary_tokio_runtime")]
            ExecutorStrategyImpl::SecondaryTokioRuntime(executor) => executor.execute(fut).await,
        }
    }
}

/// The fallback strategy used in case no strategy is explicitly set
static DEFAULT_COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY: OnceLock<ExecutorStrategyImpl> =
    OnceLock::new();

impl Default for &ExecutorStrategyImpl {
    fn default() -> Self {
        &DEFAULT_COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get_or_init(|| {
            #[cfg(feature = "tokio")]
            {
                log::info!("Defaulting to SpawnBlocking strategy for compute-heavy future executor \
                until a strategy is initialized");

                ExecutorStrategyImpl::SpawnBlocking(SpawnBlockingExecutor::new(None))
            }

            #[cfg(not(feature = "tokio"))]
            {
                log::warn!("Defaulting to CurrentContext (non-op) strategy for compute-heavy future executor \
                until a strategy is initialized.");
                ExecutorStrategyImpl::CurrentContext(CurrentContextExecutor::new(None))
            }
        })
    }
}

/// Spawn a future to the configured compute-heavy executor and wait on its output.
///
/// # Strategy selection
///
/// If no strategy is configured, this library will fall back to the following defaults:
/// - no `tokio`` feature - current context
/// - all other cases - spawn blocking
///
/// You can override these defaults by initializing a strategy via [`global_strategy_builder()`]
/// and [`GlobalStrategyBuilder`].
///
/// # Cancellation
///
/// Most strategies will cancel the input future, if the caller drops the returned future,
/// with the following exception:
/// - the block in place strategy never cancels the future (until the executor is shut down)
///
/// # Example
///
/// ```
/// # async fn run() {
/// use compute_heavy_future_executor::execute_compute_heavy_future;
///
/// let future = async {
///     std::thread::sleep(std::time::Duration::from_millis(50));
///     5
///  };
///
/// let res = execute_compute_heavy_future(future).await.unwrap();
/// assert_eq!(res, 5);
/// # }
///
/// ```
///
pub async fn execute_compute_heavy_future<F, R>(fut: F) -> Result<R, Error>
where
    F: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    let executor = COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
        .get()
        .unwrap_or_else(|| <&ExecutorStrategyImpl>::default());
    match executor {
        ExecutorStrategyImpl::CurrentContext(executor) => executor.execute(fut).await,
        ExecutorStrategyImpl::CustomExecutor(executor) => executor.execute(fut).await,
        #[cfg(feature = "tokio_block_in_place")]
        ExecutorStrategyImpl::BlockInPlace(executor) => executor.execute(fut).await,
        #[cfg(feature = "tokio")]
        ExecutorStrategyImpl::SpawnBlocking(executor) => executor.execute(fut).await,
        #[cfg(feature = "secondary_tokio_runtime")]
        ExecutorStrategyImpl::SecondaryTokioRuntime(executor) => executor.execute(fut).await,
    }
}

pub fn make_future_cancellable<F, O>(fut: F) -> (impl Future<Output = ()>, Receiver<O>)
where
    F: std::future::Future<Output = O> + Send + 'static,
    O: Send + 'static,
{
    let (mut tx, rx) = tokio::sync::oneshot::channel();
    let wrapped_future = async {
        select! {
            // if tx is closed, we always want to poll that future first,
            // so we don't need to add rng
            biased;

            _ = tx.closed() => {
                // receiver already dropped, don't need to do anything
                // cancel the background future
            },
            result = fut => {
                // if this fails, the receiver already dropped, so we don't need to do anything
                let _ = tx.send(result);
            }
        }
    };

    (wrapped_future, rx)
}

// tests are in /tests/ to allow separate initialization of oncelock across processes when using default cargo test runner
