#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, deny(warnings))]

//! # vacation
//!
//!  `vacation``: give your (runtime) aworkers a break!
//!
//! ## Overview
//!
//! Today, when library authors are writing async APIs, they don't have a good way to handle long-running sync segments.
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
//! And then wrap any sync work by passing it as a closure to a global `execute_sync()` call:
//!
//! ```
//! use vacation::{execute_sync, ChanceOfBlocking};
//!
//! fn sync_work(input: String)-> u8 {
//!     std::thread::sleep(std::time::Duration::from_secs(5));
//!     println!("{input}");
//!     5
//! }
//!
//! pub async fn a_future_that_has_blocking_sync_work() -> u8 {
//!     // relies on caller-specified strategy for translating execute_sync into a future that won't
//!     // block the current worker thread
//!     execute_sync(move || { sync_work("foo".to_string()) }, ChanceOfBlocking::High).await.unwrap()
//! }
//!
//! ```
//!
//! ## Usage - Application owners
//! Application authors will need to add this library as a a direct dependency in order to customize the execution strategy.
//! By default, the strategy is just a non-op.
//!
//! You can customize the strategy using the [`SyncExecutorBuilder`] or [`initialize_tokio()`].
//!
//! ### Simple example
//!
//! ```ignore
//! [dependencies]
//! // enables `tokio` feature by default => spawn_blocking strategy
//! vacation = { version = "0.1" }
//! ```
//!
//! And then call the `initialize_tokio()` helper that uses some sensible defaults:
//! ```ignore
//! use vacation::initialize_tokio;
//!
//! #[tokio::main]
//! async fn main() {
//!     initialize_tokio().unwrap();
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
//! use vacation::{
//!     global_sync_strategy_builder, CustomExecutorSyncClosure,
//! };
//!
//! static THREADPOOL: OnceLock<ThreadPool> = OnceLock::new();
//!
//! fn initialize_strategy() {
//!     THREADPOOL.set(rayon::ThreadPoolBuilder::default().build().unwrap());
//!
//!     let custom_closure: CustomExecutorSyncClosure =
//!         Box::new(|f| Box::new(async move { Ok(THREADPOOL.get().unwrap().spawn(f)) }));
//!
//!     global_sync_strategy_builder()
//!         // probably no need for max concurrency as rayon already is defaulting to a thread per core
//!         // and using a task queue
//!         .initialize_custom_executor(custom_closure).unwrap();
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
    custom_executor::CustomExecutorSyncClosure, global_sync_strategy, initialize_tokio,
    SyncExecutorBuilder,
};

use executor::{get_global_sync_executor, ExecuteSync, SyncExecutor};

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
/// See [`SyncExecutorBuilder`] for more detail on each strategy.
///
/// # Examples
///
/// ```
/// use vacation::{
///     global_sync_strategy,
///     global_sync_strategy_builder,
///     GlobalStrategy,
///     ExecutorStrategy
/// };
///
/// # fn run() {
///
/// #[cfg(feature = "tokio")]
/// assert_eq!(global_sync_strategy(), GlobalStrategy::Default(ExecutorStrategy::SpawnBlocking));
///
/// #[cfg(not(feature = "tokio"))]
/// assert_eq!(global_sync_strategy(), GlobalStrategy::Default(ExecutorStrategy::CurrentContext));
///
/// global_sync_strategy_builder()
///         .initialize_current_context()
///         .unwrap();
///
/// assert_eq!(global_sync_strategy(), GlobalStrategy::Initialized(ExecutorStrategy::CurrentContext));
///
/// # }
/// ```
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
}

/// Initialize a builder to set the global sync function
/// executor strategy.
///
/// See [`SyncExecutorBuilder`] for details on strategies.
///
/// # Examples
///
/// ```
/// use vacation::global_sync_strategy_builder;
/// # fn run() {
/// global_sync_strategy_builder().max_concurrency(3).initialize_spawn_blocking().unwrap();
/// # }
/// ```
#[must_use = "doesn't do anything unless used"]
pub fn global_sync_strategy_builder() -> SyncExecutorBuilder {
    SyncExecutorBuilder::default()
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
/// If no strategy is configured, this library will fall back to a non-op `CurrentContext` strategy.
///
/// You can override these defaults by initializing a strategy via [`global_sync_strategy_builder()`]
/// and [`SyncExecutorBuilder`].
///
/// # Examples
///
/// ```
/// # async fn run() {
/// use vacation::{execute_sync, ChanceOfBlocking};
///
/// let closure = || {
///     std::thread::sleep(std::time::Duration::from_secs(1));
///     5
/// };
///
/// let res = execute_sync(closure, ChanceOfBlocking::High).await.unwrap();
/// assert_eq!(res, 5);
/// # }
///
/// ```
///
pub async fn execute_sync<F, R>(f: F, _chance_of_blocking: ChanceOfBlocking) -> Result<R, Error>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
    Error: Send + Sync + 'static,
{
    let executor = get_global_sync_executor();

    match executor {
        SyncExecutor::CurrentContext(executor) => executor.execute_sync(f).await,
        SyncExecutor::CustomExecutor(executor) => executor.execute_sync(f).await,
        #[cfg(feature = "tokio")]
        SyncExecutor::SpawnBlocking(executor) => executor.execute_sync(f).await,
    }
}

// tests are in /tests/ to allow separate initialization of oncelock across processes when using default cargo test runner
