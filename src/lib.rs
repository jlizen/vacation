#[cfg(feature = "tokio_multi_threaded")]
mod block_in_place;
mod current_context;
mod custom_executor;
mod error;
#[cfg(feature = "secondary_tokio_runtime")]
mod secondary_tokio_runtime;
#[cfg(feature = "tokio")]
mod spawn_blocking;

pub use custom_executor::CustomExecutorClosure;

use std::{future::Future, sync::OnceLock};

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

/// Awaits the future in the current context. This is effectively a non-op wrapper
/// that adds no special handling for the future. This is the default if
/// the `tokio` feature is disabled.
///
/// 
/// # Cancellation
/// Yes, the future is dropped if the caller drops the returned future
/// 
/// # Panics
/// Panics if compute-heavy executor strategy is initialized more than once, across all strategies.
///
/// # Example
///
/// ```
/// use compute_heavy_future_executor::initialize_current_context_strategy;
/// use compute_heavy_future_executor::spawn_compute_heavy_future;
///
/// # async fn run() {
/// initialize_current_context_strategy();
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
pub fn initialize_current_context_strategy() {
    log::info!("initializing compute-heavy executor with current context strategy");
    let strategy = ExecutorStrategy::CurrentContext(CurrentContextExecutor {});
    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
        .set(strategy)
        .unwrap_or_else(|_| panic!("Compute heavy future executor can only be initialized once"))
}

/// Executes futures by blocking on them inside the tokio blocking threadpool. By default, tokio will spin up a blocking thread
/// per task, which may be more than your count of CPU cores, depending on runtime config.
///
/// If you expect many concurrent cpu-heavy futures, consider limiting your blocking tokio threadpool size.
/// Or, you can use a heavier weight strategy like [`initialize_secondary_tokio_runtime_strategy`].
/// 
/// # Cancellation
/// Yes, the future is dropped if the caller drops the returned future
/// 
/// # Panics
/// Panics if compute-heavy executor strategy is initialized more than once, across all strategies.
///
/// # Example
///
/// ```
/// use compute_heavy_future_executor::initialize_spawn_blocking_strategy;
/// use compute_heavy_future_executor::spawn_compute_heavy_future;
///
/// # async fn run() {
/// initialize_spawn_blocking_strategy();
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
pub fn initialize_spawn_blocking_strategy() {
    log::info!("initializing compute-heavy executor with spawn blocking strategy");
    let strategy = ExecutorStrategy::SpawnBlocking(SpawnBlockingExecutor {});
    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
        .set(strategy)
        .unwrap_or_else(|_| panic!("Compute heavy future executor can only be initialized once"))
}

/// Executes compute-heavy future by calling task::block_in_place on the current worker,
/// and evicts other tasks on same worker thread to avoid blocking them.
///
/// Can starve your executor of worker threads if called with too many
/// concurrent cpu-heavy futures.
///
/// If you expect many concurrent cpu-heavy futures, consider a
/// heavier weight strategy like [`initialize_secondary_tokio_runtime_strategy`].
///
/// # Cancellation
/// No, this future is not cancellable
/// 
/// # Panics
/// Panics if compute-heavy executor strategy is initialized more than once, across all strategies.
///
/// # Example
///
/// ```
/// use compute_heavy_future_executor::initialize_block_in_place_strategy;
/// use compute_heavy_future_executor::spawn_compute_heavy_future;
///
/// # async fn run() {
/// initialize_block_in_place_strategy();
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
pub fn initialize_block_in_place_strategy() {
    log::info!("initializing compute-heavy executor with block in place strategy");
    let strategy = ExecutorStrategy::BlockInPlace(BlockInPlaceExecutor {});
    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
        .set(strategy)
        .unwrap_or_else(|_| panic!("Compute heavy future executor can only be initialized once"))
}

/// Creates a secondary tokio runtime to delegate compute-heavy futures to via channels.
/// Uses the same number of threads as cpu core, and reduced thread priority on linux (niceness 10).
/// Use secondary_executor_config() to customize the defaults.
///
/// # Cancellation
/// Yes, the future is dropped if the caller drops the returned future
/// 
/// # Panics
/// Panics if compute-heavy executor strategy is initialized more than once, across all strategies.
///
/// # Example
///
/// ```
/// use compute_heavy_future_executor::initialize_secondary_tokio_runtime_strategy;
/// use compute_heavy_future_executor::spawn_compute_heavy_future;
///
/// # async fn run() {
/// initialize_secondary_tokio_runtime_strategy();
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
pub fn initialize_secondary_tokio_runtime_strategy() {
    log::info!("initializing compute-heavy executor with secondary tokio runtime strategy");
    let strategy =
        ExecutorStrategy::SecondaryTokioRuntime(SecondaryTokioRuntimeExecutor::new(None, None));
    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
        .set(strategy)
        .unwrap_or_else(|_| panic!("Compute heavy future executor can only be initialized once"))
}

/// Creates a secondary tokio runtime to delegate compute-heavy futures to via channels.
///
/// Allows custom thread niceness of -20..=19 , and any thread count. If either value is omitted,
/// it will fall back to the default of 10 niceness and/or count of threads = cpu core count.
///
/// # Cancellation
/// Yes, the future is dropped if the caller drops the returned future
/// 
/// # Panics
/// Panics if niceness provided outside of -20..=19 bounds.
///
/// Panics if compute-heavy executor strategy is initialized more than once, across all strategies.
///
/// # Example
///
/// ```
/// use compute_heavy_future_executor::initialize_secondary_tokio_runtime_strategy_and_config;
/// use compute_heavy_future_executor::spawn_compute_heavy_future;
///
/// # async fn run() {
/// initialize_secondary_tokio_runtime_strategy_and_config(Some(5), Some(10));
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
pub fn initialize_secondary_tokio_runtime_strategy_and_config(
    niceness: Option<i8>,
    thread_count: Option<usize>,
) {
    log::info!("initializing compute-heavy executor with secondary tokio runtime strategy");
    let strategy = ExecutorStrategy::SecondaryTokioRuntime(SecondaryTokioRuntimeExecutor::new(
        niceness,
        thread_count,
    ));
    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
        .set(strategy)
        .unwrap_or_else(|_| panic!("Compute heavy future executor can only be initialized once"))
}

/// Accepts a closure that will poll an arbitrary feature to completion.
/// 
/// Intended for injecting arbitrary runtimes/strategies or customizing existing ones.
///
/// # Cancellation
/// Yes, the future is dropped if the caller drops the returned future,
/// unless you spawn an uncancellable task within it
/// 
/// # Panics
///
/// Panics if compute-heavy executor strategy is initialized more than once, across all strategies.
///
/// # Example
///
/// ```
/// use compute_heavy_future_executor::initialize_custom_executor_strategy;
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
/// initialize_custom_executor_strategy(closure);
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
pub fn initialize_custom_executor_strategy(closure: CustomExecutorClosure) {
    log::info!("initializing compute-heavy executor with custom strategy");
    let strategy = ExecutorStrategy::CustomExecutor(CustomExecutor { closure });
    COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY
        .set(strategy)
        .unwrap_or_else(|_| panic!("Compute heavy future executor can only be initialized once"))
}

/// The stored strategy used to spawn compute-heavy futures.
static COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY: OnceLock<ExecutorStrategy> = OnceLock::new();

trait ComputeHeavyFutureExecutor {
    /// Accepts a future and returns its result
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static;
}

enum ExecutorStrategy {
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

/// Spawn a future to the configured compute-heavy executor and wait on its output.
///
/// # Strategy selection
///
/// If no strategy is configured, this library will fall back to the following defaults:
/// - no tokio cfg flag - current context
/// - tokio, in multi-threaded runtime flavor - block in place
/// - tokio, all other flavors - spawn blocking
///
/// You can override these defaults by initializing a strategy:
/// - [`initialize_current_context_strategy`],
/// - [`initialize_spawn_blocking_strategy`]
/// - [`initialize_block_in_place_strategy`]
/// - [`initialize_secondary_tokio_runtime_strategy`]
/// - [`initialize_secondary_tokio_runtime_strategy_and_config`]
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
                #[cfg(feature = "tokio")]
                match tokio::runtime::Handle::current().runtime_flavor() {
                    tokio::runtime::RuntimeFlavor::MultiThread => {
                        log::trace!("spawn_compute_heavy_future called without setting an explicit executor strategy, \
                        setting to BlockInPlace due to tokio multithreaded runtime context");
                        &ExecutorStrategy::BlockInPlace(BlockInPlaceExecutor {})
                    },
                    _ => {
                        log::trace!("spawn_compute_heavy_future called without setting an explicit executor strategy, \
                        setting to SpawnBlocking");
                        &ExecutorStrategy::SpawnBlocking(SpawnBlockingExecutor {})
                    },
                }

                #[cfg(not(feature = "tokio"))]
                {
                    log::debug!("spawn_compute_heavy_future called without setting an explicit executor strategy, setting to \
                    current context due to no `cfg(compute_heavy_executor_tokio)`. This is a non-op and probably not what you want.");
                    &ExecutorStrategy::CurrentContext(CurrentContextExecutor {})
                }
        });
    match executor {
        ExecutorStrategy::CurrentContext(executor) => executor.execute(fut).await,
        ExecutorStrategy::CustomExecutor(executor) => executor.execute(fut).await,
        #[cfg(feature = "tokio_multi_threaded")]
        ExecutorStrategy::BlockInPlace(executor) => executor.execute(fut).await,
        #[cfg(feature = "tokio")]
        ExecutorStrategy::SpawnBlocking(executor) => executor.execute(fut).await,
        #[cfg(feature = "secondary_tokio_runtime")]
        ExecutorStrategy::SecondaryTokioRuntime(executor) => executor.execute(fut).await,
    }
}

// tests are in /tests/ to allow seperate intialization of oncelock when using default cargo test runner
