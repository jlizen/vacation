#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, deny(warnings))]
#![doc = include_str!("../README.md")]

mod concurrency_limit;
mod error;
mod executor;
#[cfg(feature = "future")]
/// Vacation future implementations for libraries that are manually implementing futures
pub mod future;

pub use error::Error;
#[cfg(feature = "tokio")]
pub use executor::install_tokio_strategy;
pub use executor::{
    custom::{CustomClosureInput, CustomClosureOutput},
    global_strategy, ExecutorBuilder,
};

use executor::{get_global_executor, Executor, NeedsStrategy};

/// The currently loaded global strategy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlobalStrategy {
    /// The strategy loaded by default.
    Default(ExecutorStrategy),
    /// Caller-specified strategy, replacing default
    Initialized(ExecutorStrategy),
}

/// The different types of executor strategies that can be loaded.
/// See [`ExecutorBuilder`] for more detail on each strategy.
///
/// # Examples
///
/// ```
/// use vacation::{
///     global_strategy,
///     GlobalStrategy,
///     ExecutorStrategy
/// };
///
/// # fn run() {
///
/// assert_eq!(global_strategy(), GlobalStrategy::Default(ExecutorStrategy::ExecuteDirectly));
///
/// vacation::init()
///         .execute_directly()
///         .install()
///         .unwrap();
///
/// assert_eq!(global_strategy(), GlobalStrategy::Initialized(ExecutorStrategy::ExecuteDirectly));
///
/// # }
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutorStrategy {
    /// A non-op strategy that awaits in the current context
    ExecuteDirectly,
    /// User-provided closure
    Custom,
    /// tokio task::spawn_blocking
    #[cfg(feature = "tokio")]
    SpawnBlocking,
}

/// Initialize a builder to set the global sync function
/// executor strategy.
///
/// See [`ExecutorBuilder`] for details on strategies.
///
/// # Examples
///
/// ```
/// # fn run() {
/// vacation::init().max_concurrency(3).execute_directly().install().unwrap();
/// # }
/// ```
#[must_use = "doesn't do anything unless used"]
pub fn init() -> ExecutorBuilder<NeedsStrategy> {
    ExecutorBuilder {
        strategy: NeedsStrategy,
        max_concurrency: None,
    }
}

/// Likelihood of the provided closure blocking for a significant period of time.
/// Will eventually be used to customize strategies with more granularity.
#[derive(Debug, Clone, Copy)]
pub enum ChanceOfBlocking {
    /// Very likely to block, use primary sync strategy
    High,
}

/// Send a sync closure to the configured or default global compute-heavy executor, and wait on its output.
///
/// # Strategy selection
///
/// If no strategy is configured, this library will fall back to a non-op `ExecuteDirectly` strategy.
///
/// You can override these defaults by initializing a strategy via [`init()`]
/// and [`ExecutorBuilder`].
///
/// # Examples
///
/// ```
/// # async fn run() {
/// let closure = || {
///     std::thread::sleep(std::time::Duration::from_secs(1));
///     5
/// };
///
/// let res = vacation::execute(closure, vacation::ChanceOfBlocking::High).await.unwrap();
/// assert_eq!(res, 5);
/// # }
///
/// ```
///
pub async fn execute<F, R>(f: F, _chance_of_blocking: ChanceOfBlocking) -> Result<R, Error>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
    Error: Send + Sync + 'static,
{
    let executor = get_global_executor();

    match executor {
        Executor::ExecuteDirectly(executor) => executor.execute(f).await,
        Executor::Custom(executor) => executor.execute(f).await,
        #[cfg(feature = "tokio")]
        Executor::SpawnBlocking(executor) => executor.execute(f).await,
    }
}

// tests are in /tests/ to allow separate initialization of oncelock across processes when using default cargo test runner
