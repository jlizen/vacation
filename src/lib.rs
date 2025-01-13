#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, deny(warnings))]
#![doc = include_str!("../README.md")]

mod concurrency_limit;
mod error;
mod executor;
#[cfg(feature = "future")]
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

/// A context object to be passed into [`execute()`] containing
/// metadata that allows the caller to fine tune strategies
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct ExecuteContext {
    /// The chance of blocking a future for a significant amount of time with this work
    pub chance_of_blocking: ChanceOfBlocking,
}

impl ExecuteContext {
    /// Create a new execute context, to use with [`execute()`], with
    /// a given chance of blocking
    pub fn new(chance_of_blocking: ChanceOfBlocking) -> Self {
        Self { chance_of_blocking }
    }
}

/// Likelihood of the provided closure blocking for a significant period of time (~50μs+).
/// Will eventually be used to customize strategies with more granularity.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub enum ChanceOfBlocking {
    /// Very likely to block for significant amount of time(~50μs+), use primary strategy
    Frequent,
    /// Blocks some of the time for ~50μs+ but not always,
    /// potentially use an optimistic strategy that doesn't
    /// send across threads
    Occasional,
}

/// Send a sync closure to the configured or default global compute-heavy executor, and wait on its output.
///
/// Pass in the chance of blocking as well as namespace of your operation to allow more granular strategy configuration.
///
/// # Strategy selection
///
/// If no strategy is configured, this library will fall back to a non-op `ExecuteDirectly` strategy.
///
/// An application override these defaults by initializing a strategy via [`init()`]
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
/// let res = vacation::execute(
///     closure,
///     vacation::ExecuteContext::new(vacation::ChanceOfBlocking::Frequent)).await.unwrap();
/// assert_eq!(res, 5);
/// # }
///
/// ```
///
pub async fn execute<F, R>(f: F, context: ExecuteContext) -> Result<R, Error>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
    Error: Send + Sync + 'static,
{
    let executor = get_global_executor();

    match executor {
        Executor::ExecuteDirectly(executor) => executor.execute(f).await,
        Executor::Custom(executor) => executor.execute(f, context).await,
        #[cfg(feature = "tokio")]
        Executor::SpawnBlocking(executor) => executor.execute(f).await,
    }
}

// tests are in /tests/ to allow separate initialization of oncelock across processes when using default cargo test runner
