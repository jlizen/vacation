#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, deny(warnings))]

//! # vacation
//!
//!  `vacation``: Give your (runtime) workers a break!
//!
//! ## Overview
//!
//! Today, when library authors write async APIs, they don't have a good way to handle long-running sync segments.
//!
//! For an application, they can use selective handling such as `tokio::task::spawn_blocking()` along with concurrency control to delegate sync segments to blocking threads. Or, they might send the work to a `rayon` threadpool.
//!
//! But, library authors generally don't have this flexibility. As, they generally want to be agnostic across runtime. Or, even if they are `tokio`-specific, they generally don't want to call `tokio::task::spawn_blocking()` as it is
//! suboptimal without extra configuration (concurrency control) as well as highly opinionated to send the work across threads.
//!
//! This library solves this problem by providing libray authors a static, globally scoped strategy that they can delegate blocking sync work to without drawing any conclusions about handling.
//!
//! And then, the applications using the library can tune handling based on their preferred approach.
//!
//! ## Usage - Library Authors
//! First add dependency enabling `vacation` (perhaps behind a feature flag).
//!
//! The below will default to 'current context' sync execution (ie non-op) unless the caller enables the tokio feature.
//!
//! ```ignore
//! [dependencies]
//! vacation = { version = "0.1", default-features = false }
//! ```
//!
//! And then wrap any sync work by passing it as a closure to a global `execute()` call:
//!
//! ```
//! fn sync_work(input: String)-> u8 {
//!     std::thread::sleep(std::time::Duration::from_secs(5));
//!     println!("{input}");
//!     5
//! }
//!
//! pub async fn a_future_that_has_blocking_sync_work() -> u8 {
//!     // relies on caller-specified strategy for translating execute into a future that won't
//!     // block the current worker thread
//!     vacation::execute(move || { sync_work("foo".to_string()) }, vacation::ChanceOfBlocking::High).await.unwrap()
//! }
//!
//! ```
//!
//! ## Usage - Application owners
//! Application authors will need to add this library as a a direct dependency in order to customize the execution strategy.
//! By default, the strategy is just a non-op.
//!
//! You can customize the strategy using the [`ExecutorBuilder`] or [`install_tokio_strategy()`].
//!
//! ### Simple example
//!
//! ```ignore
//! [dependencies]
//! // enables `tokio` feature by default => spawn_blocking strategy
//! vacation = { version = "0.1" }
//! ```
//!
//! And then call the `install_tokio_strategy()` helper that uses some sensible defaults:
//! ```ignore
//! #[tokio::main]
//! async fn main() {
//!     vacation::install_tokio_strategy().unwrap();
//! }
//! ```
//!
//! ### Rayon example
//! Or, you can add an alternate strategy, for instance a custom closure using Rayon.
//!
//! ```ignore
//! [dependencies]
//! vacation = { version = "0.1", default-features = false }
//! // used for example with custom executor
//! rayon = "1"
//! ```
//!
//! ```
//! use std::sync::OnceLock;
//! use rayon::ThreadPool;
//!
//! static THREADPOOL: OnceLock<ThreadPool> = OnceLock::new();
//!
//! fn initialize_strategy() {
//!     THREADPOOL.set(rayon::ThreadPoolBuilder::default().build().unwrap());
//!
//!     let custom_closure: vacation::CustomClosure =
//!         Box::new(|f| Box::new(async move { Ok(THREADPOOL.get().unwrap().spawn(f)) }));
//!
//!     vacation::init()
//!         // probably no need for max concurrency as rayon already is defaulting to a thread per core
//!         // and using a task queue
//!         .custom_executor(custom_closure).install().unwrap();
//! }
//! ```
//!
//! ## Feature flags
//!
//! Feature flags are used to reduce the amount
//! of compiled code.
//!
//! - `tokio`: Enables the `SpawnBlocking` sync strategy.
//!

mod concurrency_limit;
mod error;
mod executor;

pub use error::Error;
pub use executor::{
    custom_executor::CustomClosure, global_strategy, install_tokio_strategy, ExecutorBuilder,
};

use executor::{get_global_executor, Execute, Executor, NoStrategy};

use std::fmt::Debug;

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
/// #[cfg(feature = "tokio")]
/// assert_eq!(global_strategy(), GlobalStrategy::Default(ExecutorStrategy::SpawnBlocking));
///
/// #[cfg(not(feature = "tokio"))]
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
/// vacation::init().max_concurrency(3).spawn_blocking().install().unwrap();
/// # }
/// ```
#[must_use = "doesn't do anything unless used"]
pub fn init() -> ExecutorBuilder<NoStrategy> {
    ExecutorBuilder {
        strategy: NoStrategy,
        max_concurrency: None,
    }
}

/// Likelihood of the provided closure blocking for a significant period of time.
/// Will eventually be used to customize strategies with more granularity.
#[derive(Debug)]
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
