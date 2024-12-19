#[cfg(compute_heavy_executor_tokio)]
mod block_in_place;
mod current_context;
mod custom_executor;
mod error;
#[cfg(compute_heavy_executor_tokio)]
mod secondary_tokio_runtime;
#[cfg(compute_heavy_executor_tokio)]
mod spawn_blocking;

pub use custom_executor::AnyWrappedFuture;
pub use custom_executor::CustomExecutorClosure;

use std::{future::Future, sync::OnceLock};

#[cfg(compute_heavy_executor_tokio)]
use block_in_place::BlockInPlaceExecutor;
use current_context::CurrentContextExecutor;
use custom_executor::CustomExecutor;
use error::Error;
#[cfg(compute_heavy_executor_tokio)]
use secondary_tokio_runtime::SecondaryTokioRuntimeExecutor;
#[cfg(compute_heavy_executor_tokio)]
use spawn_blocking::SpawnBlockingExecutor;

// TODO: module docs, explain the point of this library, explain how to use `cfg(compute_heavy_executor_tokio)` to enable
// tokio, explain how calling libraries can expose it as a toggle via `cfg(compute_heavy_executor)`

/// Awaits the future in the current context. This is effectively a non-op wrapper
/// that adds no special handling for the future. This is the default if 
/// the #[cfg(compute_heavy_executor_tokio)] rust flag is disabled.
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
#[cfg(compute_heavy_executor_tokio)]
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
#[cfg(compute_heavy_executor_tokio)]
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
#[cfg(compute_heavy_executor_tokio)]
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
#[cfg(compute_heavy_executor_tokio)]
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

/// Accepts a closure that will process a type-erased future and return its results,
/// also type erased.
/// 
/// Intended for use to allow alternate executors besides tokio, or to allow
/// additional wrapper logic compared to the inbuilt strategies like the secondary tokio runtime.
///
/// # Panics
/// Panics if passed a closure that resolves type-erased futures to a different output
/// type than the future's output type.
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
/// initialize_custom_executor_strategy(closure).await;
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
pub async fn initialize_custom_executor_strategy(closure: CustomExecutorClosure) {
    log::info!("initializing compute-heavy executor with custom strategy");
    let strategy = ExecutorStrategy::CustomExecutor(CustomExecutor::new(closure).await);
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
    #[cfg(compute_heavy_executor_tokio)]
    SpawnBlocking(SpawnBlockingExecutor),
    /// tokio task::block_in_place
    #[cfg(compute_heavy_executor_tokio)]
    BlockInPlace(BlockInPlaceExecutor),
    #[cfg(compute_heavy_executor_tokio)]
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
        .get_or_init(|| {
                #[cfg(compute_heavy_executor_tokio)]
                match tokio::runtime::Handle::current().runtime_flavor() {
                    tokio::runtime::RuntimeFlavor::MultiThread => {
                        log::info!("spawn_compute_heavy_future called without setting an explicit executor strategy, \
                        setting to BlockInPlace due to tokio multithreaded runtime context");
                        return ExecutorStrategy::BlockInPlace(BlockInPlaceExecutor {})
                    },
                    _ => {
                        log::info!("spawn_compute_heavy_future called without setting an explicit executor strategy, \
                        setting to SpawnBlocking");
                        return ExecutorStrategy::SpawnBlocking(SpawnBlockingExecutor {})
                    },
                };

                #[allow(unreachable_code)]
                {
                    log::warn!("spawn_compute_heavy_future called without setting an explicit executor strategy, setting to \
                    current context due to no `cfg(compute_heavy_executor_tokio)`. This is a non-op and probably not what you want.");
                    ExecutorStrategy::CurrentContext(CurrentContextExecutor {})    
                }
        });
    match executor {
        ExecutorStrategy::CurrentContext(executor) => executor.execute(fut).await,
        ExecutorStrategy::CustomExecutor(executor) => executor.execute(fut).await,
        #[cfg(compute_heavy_executor_tokio)]
        ExecutorStrategy::BlockInPlace(executor) => executor.execute(fut).await,
        #[cfg(compute_heavy_executor_tokio)]
        ExecutorStrategy::SpawnBlocking(executor) => executor.execute(fut).await,
        #[cfg(compute_heavy_executor_tokio)]
        ExecutorStrategy::SecondaryTokioRuntime(executor) => executor.execute(fut).await,
    }
}

// Use nextest to run these, otherwise there will be race conditions against the strategy oncecell
#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;

    #[cfg(not(compute_heavy_executor_tokio))]
    #[tokio::test]
    async fn default_to_current_context_non_tokio() {
        let future = async { 5 };
        
        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::CurrentContext(..))));
    }

    #[cfg(compute_heavy_executor_tokio)]
    #[tokio::test]
    async fn default_to_current_context_tokio_single_threaded() {
        // this is a tokio test but we haven't enabled the tokio config flag

        let future = async { 5 };
        
        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::SpawnBlocking(..))));
    }

    #[cfg(compute_heavy_executor_tokio)]
    #[tokio::test(flavor = "multi_thread")]
    async fn default_to_current_context_tokio_multi_threaded() {
        let future = async { 5 };
        
        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::BlockInPlace(..))));
    }

    #[tokio::test]
    async fn current_context_strategy() {
        initialize_current_context_strategy();

        let future = async { 5 };

        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::CurrentContext(..))));
    }

    #[tokio::test]
    async fn custom_strategy_legal_closure() {
        let closure: CustomExecutorClosure = Box::new(|fut| {
            Box::new(async move {
                let res = fut.await;
                Ok(res)
            })
        });

        initialize_custom_executor_strategy(closure).await;

        let future = async { 5 };
        
        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::CustomExecutor(..))));
    }

    #[cfg(compute_heavy_executor_tokio)]
    #[tokio::test]
    async fn custom_strategy_legal_closure_tokio_spawn() {
        let closure: CustomExecutorClosure = Box::new(|fut| {
            Box::new(
                async move {
                    let handle = tokio::task::spawn(async move { fut.await });
                    handle.await.map_err(|err| err.into())
                }
            )
        });
        initialize_custom_executor_strategy(closure).await;

        let future = async { 5 };
        
        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::CustomExecutor(..))));
    }

    #[tokio::test]
    #[should_panic]
    async fn custom_strategy_illegal_closure() {
        // this closure overwrites the input type, causing a mismatch
        let closure: CustomExecutorClosure = Box::new(|_| {
            Box::new(async move {
                Ok(Box::new("foo") as Box<dyn Any + Send>)
            })
        });
        // this should panic due to bad closure
        initialize_custom_executor_strategy(closure).await;
    }

    #[cfg(compute_heavy_executor_tokio)]
    #[tokio::test]
    async fn spawn_blocking_strategy() {
        initialize_spawn_blocking_strategy();

        let future = async { 5 };

        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::SpawnBlocking(..))));
    }

    #[cfg(compute_heavy_executor_tokio)]
    #[tokio::test(flavor = "multi_thread")]
    async fn block_in_place_strategy() {
        initialize_block_in_place_strategy();

        let future = async { 5 };
        
        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::BlockInPlace(..))));
    }

    #[cfg(compute_heavy_executor_tokio)]
    #[tokio::test]
    async fn secondary_tokio_runtime_strategy() {
        initialize_secondary_tokio_runtime_strategy();

        let future = async { 5 };
        
        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::SecondaryTokioRuntime(..))));
    }

    #[cfg(compute_heavy_executor_tokio)]
    #[tokio::test]
    async fn secondary_tokio_runtime_strategy_allowed_config() {
        initialize_secondary_tokio_runtime_strategy_and_config(Some(5), Some(50));

        let future = async { 5 };
        
        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::SecondaryTokioRuntime(..))));
    }

    #[cfg(compute_heavy_executor_tokio)]
    #[tokio::test]
    #[should_panic]
    async fn secondary_tokio_runtime_strategy_disallowed_config() {
        initialize_secondary_tokio_runtime_strategy_and_config(Some(30), Some(50));

        let future = async { 5 };
        
        let res = spawn_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert!(matches!(COMPUTE_HEAVY_FUTURE_EXECUTOR_STRATEGY.get(), Some(&ExecutorStrategy::SecondaryTokioRuntime(..))));
    }

    #[tokio::test]
    #[should_panic]
    async fn multiple_initializes_panic() {
        initialize_current_context_strategy();
        initialize_current_context_strategy();
    }
}