#[cfg(feature = "tokio_multi_threaded")]
mod block_in_place;
mod current_context;
mod custom_executor;
pub mod error;
#[cfg(feature = "secondary_tokio_runtime")]
mod secondary_tokio_runtime;
#[cfg(feature = "tokio")]
mod spawn_blocking;

pub use custom_executor::CustomExecutorClosure;
#[cfg(feature = "secondary_tokio_runtime")]
pub use secondary_tokio_runtime::SecondaryTokioRuntimeStrategyBuilder;

use std::{fmt::Debug, future::Future, sync::OnceLock};

#[cfg(feature = "tokio_multi_threaded")]
use block_in_place::BlockInPlaceExecutor;
use current_context::CurrentContextExecutor;
use custom_executor::CustomExecutor;
use error::Error;
#[cfg(feature = "secondary_tokio_runtime")]
use secondary_tokio_runtime::SecondaryTokioRuntimeExecutor;
#[cfg(feature = "tokio")]
use spawn_blocking::SpawnBlockingExecutor;

// TODO: module docs, explain the point of this library, give some samples

/// Initialize a builder to set the global compute heavy future
/// executor strategy.
///
/// ## Error
/// Returns an error if the global strategy is already initialized.
/// It can only be initialized once.
pub fn global_strategy_builder() -> Result<GlobalStrategyBuilder, Error> {
    if let Some(val) = COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get() {
        return Err(Error::AlreadyInitialized(val.into()));
    }

    Ok(GlobalStrategyBuilder::default())
}

/// Get the currently initialized strategy, or the default strategy for the
/// current feature and runtime type in case no strategy has been loaded.
pub fn global_strategy() -> CurrentStrategy {
    match COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get() {
        Some(strategy) => CurrentStrategy::Initialized(strategy.into()),
        None => CurrentStrategy::Default(<&ExecutorStrategyImpl>::default().into()),
    }
}

#[must_use]
#[derive(Default)]
pub struct GlobalStrategyBuilder {}

impl GlobalStrategyBuilder {
    /// Initializes a new global strategy to wait in the current context.
    ///
    /// This is effectively a non-op wrapper
    /// that adds no special handling for the future. This is the default if
    /// the `tokio` feature is disabled.
    ///
    /// # Cancellation
    /// Yes, the future is dropped if the caller drops the returned future from spawn_compute_heavy_future().
    ///
    /// ## Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::spawn_compute_heavy_future;
    ///
    /// # async fn run() {
    /// global_strategy_builder().unwrap().initialize_current_context().unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = spawn_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    /// ```
    pub fn initialize_current_context(self) -> Result<(), Error> {
        log::info!("initializing compute-heavy executor with current context strategy");
        let strategy = ExecutorStrategyImpl::CurrentContext(CurrentContextExecutor {});
        COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
            .set(strategy)
            .map_err(|_| {
                Error::AlreadyInitialized(
                    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get().unwrap().into(),
                )
            })
    }

    /// Initializes a new global strategy to execute futures by blocking on them inside the
    /// tokio blocking threadpool.
    ///
    /// By default, tokio will spin up a blocking thread
    /// per task, which may be more than your count of CPU cores, depending on runtime config.
    ///
    /// If you expect many concurrent cpu-heavy futures, consider limiting your blocking tokio threadpool size.
    /// Or, you can use a heavier weight strategy like [`initialize_secondary_tokio_runtime()`].
    ///
    /// # Cancellation
    /// Yes, the future is dropped if the caller drops the returned future from spawn_compute_heavy_future().
    ///
    /// ## Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::spawn_compute_heavy_future;
    ///
    /// # async fn run() {
    /// global_strategy_builder().unwrap().initialize_spawn_blocking().unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = spawn_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    /// ```
    #[cfg(feature = "tokio")]
    pub fn initialize_spawn_blocking(self) -> Result<(), Error> {
        log::info!("initializing compute-heavy executor with spawn blocking strategy");
        let strategy = ExecutorStrategyImpl::SpawnBlocking(SpawnBlockingExecutor {});
        COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
            .set(strategy)
            .map_err(|_| {
                Error::AlreadyInitialized(
                    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get().unwrap().into(),
                )
            })
    }

    /// Initializes a new global strategy to execute futures  by calling task::block_in_place on the
    /// current tokio worker. This evicts other tasks on same worker thread to avoid blocking them.
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
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::spawn_compute_heavy_future;
    ///
    /// # async fn run() {
    /// global_strategy_builder().unwrap().initialize_block_in_place().unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = spawn_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    /// ```
    #[cfg(feature = "tokio_multi_threaded")]
    pub fn initialize_block_in_place(self) -> Result<(), Error> {
        log::info!("initializing compute-heavy executor with block in place strategy");
        let strategy = ExecutorStrategyImpl::BlockInPlace(BlockInPlaceExecutor {});
        COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
            .set(strategy)
            .map_err(|_| {
                Error::AlreadyInitialized(
                    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get().unwrap().into(),
                )
            })
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
    /// Yes, the future is dropped if the caller drops the returned future from spawn_compute_heavy_future().
    ///
    /// ## Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// Subsequent calls to the [`SecondaryTokioRuntimeStrategyBuilder`] can also return errors,
    /// which will result in the strategy not being initialized.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::spawn_compute_heavy_future;
    ///
    /// # async fn run() {
    /// global_strategy_builder().unwrap().initialize_secondary_tokio_runtime().unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = spawn_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    /// ```
    #[cfg(feature = "secondary_tokio_runtime")]
    pub fn initialize_secondary_tokio_runtime(self) -> Result<(), Error> {
        log::info!("initializing compute-heavy executor with secondary tokio runtime strategy");
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
    /// Yes, the future is dropped if the caller drops the returned future from spawn_compute_heavy_future().
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::spawn_compute_heavy_future;
    ///
    /// # async fn run() {
    /// global_strategy_builder().unwrap().secondary_tokio_runtime_builder()
    ///     .niceness(10).unwrap()
    ///     .thread_count(5)
    ///     .channel_size(5)
    ///     .max_task_concurrency(5)
    ///     .initialize().unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = spawn_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    /// ```
    #[cfg(feature = "secondary_tokio_runtime")]
    pub fn secondary_tokio_runtime_builder(self) -> SecondaryTokioRuntimeStrategyBuilder {
        SecondaryTokioRuntimeStrategyBuilder::default()
    }

    /// Accepts a closure that will poll an arbitrary feature to completion.
    ///
    /// Intended for injecting arbitrary runtimes/strategies or customizing existing ones.
    ///
    /// # Cancellation
    /// Yes, the future is dropped if the caller drops the returned future from spawn_compute_heavy_future(),
    /// unless you spawn an uncancellable task within it.
    ///
    /// ## Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Example
    ///
    /// ```
    /// use compute_heavy_future_executor::global_strategy_builder;
    /// use compute_heavy_future_executor::spawn_compute_heavy_future;
    /// use compute_heavy_future_executor::CustomExecutorClosure;
    ///
    /// // this isn't actually a good strategy, to be clear
    /// # async fn run() {
    /// let closure: CustomExecutorClosure = Box::new(|fut| {
    ///     Box::new(
    ///         async move {
    ///             let handle = tokio::task::spawn(async move { fut.await });
    ///             handle.await.map_err(|err| err.into())
    ///         }
    ///     )
    /// });
    ///
    /// global_strategy_builder().unwrap().initialize_custom_executor(closure).unwrap();
    ///
    /// let future = async {
    ///     std::thread::sleep(std::time::Duration::from_millis(50));
    ///     5
    ///  };
    ///
    /// let res = spawn_compute_heavy_future(future).await.unwrap();
    /// assert_eq!(res, 5);
    /// # }
    ///
    /// ```
    pub fn initialize_custom_executor(self, closure: CustomExecutorClosure) -> Result<(), Error> {
        log::info!("initializing compute-heavy executor with custom strategy");
        let strategy = ExecutorStrategyImpl::CustomExecutor(CustomExecutor { closure });
        COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
            .set(strategy)
            .map_err(|_| {
                Error::AlreadyInitialized(
                    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get().unwrap().into(),
                )
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurrentStrategy {
    Default(ExecutorStrategy),
    Initialized(ExecutorStrategy),
}

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
    #[cfg(feature = "tokio_multi_threaded")]
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
            #[cfg(feature = "tokio_multi_threaded")]
            ExecutorStrategyImpl::BlockInPlace(_) => Self::BlockInPlace,
            #[cfg(feature = "secondary_tokio_runtime")]
            ExecutorStrategyImpl::SecondaryTokioRuntime(_) => Self::SecondaryTokioRuntime,
        }
    }
}

/// The stored strategy used to spawn compute-heavy futures.
static COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY: OnceLock<ExecutorStrategyImpl> = OnceLock::new();

trait ComputeHeavyFutureExecutor {
    /// Accepts a future and returns its result
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static;
}

enum ExecutorStrategyImpl {
    /// A non-op strategy that awaits in the current context
    CurrentContext(CurrentContextExecutor),
    /// User-provided closure
    CustomExecutor(CustomExecutor),
    /// tokio task::spawn_blocking
    #[cfg(feature = "tokio")]
    SpawnBlocking(SpawnBlockingExecutor),
    /// tokio task::block_in_place
    #[cfg(feature = "tokio")]
    BlockInPlace(BlockInPlaceExecutor),
    #[cfg(feature = "tokio")]
    /// Spin up a second, lower-priority tokio runtime
    /// that communicates via channels
    SecondaryTokioRuntime(SecondaryTokioRuntimeExecutor),
}

impl Default for &ExecutorStrategyImpl {
    fn default() -> Self {
        #[cfg(feature = "tokio")]
        match tokio::runtime::Handle::current().runtime_flavor() {
            tokio::runtime::RuntimeFlavor::MultiThread => {
                &ExecutorStrategyImpl::BlockInPlace(BlockInPlaceExecutor {})
            }
            _ => {
                log::trace!("spawn_compute_heavy_future called without setting an explicit executor strategy, \
                setting to SpawnBlocking");
                &ExecutorStrategyImpl::SpawnBlocking(SpawnBlockingExecutor {})
            }
        }

        #[cfg(not(feature = "tokio"))]
        {
            &ExecutorStrategyImpl::CurrentContext(CurrentContextExecutor {})
        }
    }
}

/// Spawn a future to the configured compute-heavy executor and wait on its output.
///
/// # Strategy selection
///
/// If no strategy is configured, this library will fall back to the following defaults:
/// - no `tokio`` feature - current context
/// - `tokio_multi_threaded` feature, inside multi-threaded runtime flavor - block in place
/// - `tokio` feature, all other cases - spawn blocking
///
/// You can override these defaults by initializing a strategy via [`global_strategy_builder()`]
/// and [`GlobalStrategyBuilder`].
///
/// # Cancellation
///
/// Most strategies will cancel the input future, if the caller drops the returned future,
/// with the following exceptions:
/// - the block in place strategy never cancels the futuer (until the executor is shut down)
/// - the custom executor will generally cancel input futures, unless they are spawned to a task
/// that is itself uncancellable
///
/// # Example
///
/// ```
/// # async fn run() {
/// use compute_heavy_future_executor::spawn_compute_heavy_future;
///
/// let future = async {
///     std::thread::sleep(std::time::Duration::from_millis(50));
///     5
///  };
///
/// let res = spawn_compute_heavy_future(future).await.unwrap();
/// assert_eq!(res, 5);
/// # }
///
/// ```
///
pub async fn spawn_compute_heavy_future<F, R>(fut: F) -> Result<R, Error>
where
    F: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    let executor = COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
        .get().unwrap_or_else(|| {
            let strategy = <&ExecutorStrategyImpl>::default();
            log::debug!("using default strategy of {:#?} for compute-heavy future executor because no strategy has been initialized", ExecutorStrategy::from(strategy));
            &strategy
        });
    match executor {
        ExecutorStrategyImpl::CurrentContext(executor) => executor.execute(fut).await,
        ExecutorStrategyImpl::CustomExecutor(executor) => executor.execute(fut).await,
        #[cfg(feature = "tokio_multi_threaded")]
        ExecutorStrategyImpl::BlockInPlace(executor) => executor.execute(fut).await,
        #[cfg(feature = "tokio")]
        ExecutorStrategyImpl::SpawnBlocking(executor) => executor.execute(fut).await,
        #[cfg(feature = "secondary_tokio_runtime")]
        ExecutorStrategyImpl::SecondaryTokioRuntime(executor) => executor.execute(fut).await,
    }
}

// tests are in /tests/ to allow separate initialization of oncelock across processes when using default cargo test runner
