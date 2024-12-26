pub(crate) mod current_context;
pub(crate) mod custom_executor;
#[cfg(feature = "tokio")]
pub(crate) mod spawn_blocking;

use std::sync::OnceLock;

use current_context::CurrentContextExecutor;
use custom_executor::{CustomExecutor, CustomExecutorSyncClosure};

use crate::{Error, ExecutorStrategy, GlobalStrategy};

pub(crate) trait ExecuteSync {
    /// Accepts a sync function and processes it to completion.
    async fn execute_sync<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static;
}

fn set_sync_strategy(strategy: SyncExecutor) -> Result<(), Error> {
    COMPUTE_HEAVY_SYNC_EXECUTOR_STRATEGY
        .set(strategy)
        .map_err(|_| {
            Error::AlreadyInitialized(COMPUTE_HEAVY_SYNC_EXECUTOR_STRATEGY.get().unwrap().into())
        })?;

    log::info!(
        "initialized compute-heavy future executor strategy - {:#?}",
        global_sync_strategy()
    );

    Ok(())
}

/// Get the currently initialized sync strategy,
/// or the default strategy for the current feature in case no strategy has been loaded.
///
/// See [`SyncExecutorBuilder`] for details on strategies.
///
/// # Examples
///
/// ```
/// use compute_heavy_future_executor::{
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
pub fn global_sync_strategy() -> GlobalStrategy {
    match COMPUTE_HEAVY_SYNC_EXECUTOR_STRATEGY.get() {
        Some(strategy) => GlobalStrategy::Initialized(strategy.into()),
        None => GlobalStrategy::Default(<&SyncExecutor>::default().into()),
    }
}

pub(crate) fn get_global_sync_executor() -> &'static SyncExecutor {
    COMPUTE_HEAVY_SYNC_EXECUTOR_STRATEGY
        .get()
        .unwrap_or_else(|| <&SyncExecutor>::default())
}

/// The stored strategy used to spawn compute-heavy futures.
static COMPUTE_HEAVY_SYNC_EXECUTOR_STRATEGY: OnceLock<SyncExecutor> = OnceLock::new();

/// The fallback strategy used in case no strategy is explicitly set
static DEFAULT_COMPUTE_HEAVY_SYNC_EXECUTOR_STRATEGY: OnceLock<SyncExecutor> = OnceLock::new();

#[non_exhaustive]
pub(crate) enum SyncExecutor {
    /// A non-op strategy that runs the function in the current context
    CurrentContext(current_context::CurrentContextExecutor),
    /// User-provided closure
    CustomExecutor(custom_executor::CustomExecutor),
    /// tokio task::spawn_blocking
    #[cfg(feature = "tokio")]
    SpawnBlocking(spawn_blocking::SpawnBlockingExecutor),
}

impl Default for &SyncExecutor {
    fn default() -> Self {
        DEFAULT_COMPUTE_HEAVY_SYNC_EXECUTOR_STRATEGY.get_or_init(|| {
            let core_count = num_cpus::get();

            #[cfg(feature = "tokio")]
            {
                log::info!("Defaulting to SpawnBlocking strategy for compute-heavy future executor \
                with max concurrency of {core_count} until a strategy is initialized");

                SyncExecutor::SpawnBlocking(spawn_blocking::SpawnBlockingExecutor::new(Some(core_count)))
            }

            #[cfg(not(feature = "tokio"))]
            {
                log::warn!("Defaulting to CurrentContext (non-op) strategy for compute-heavy future executor \
                with max concurrency of {core_count} until a strategy is initialized.");
                SyncExecutor::CurrentContext(CurrentContextExecutor::new(Some(core_count)))
            }
        })
    }
}

impl ExecuteSync for SyncExecutor {
    async fn execute_sync<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        match self {
            SyncExecutor::CurrentContext(executor) => executor.execute_sync(f).await,
            SyncExecutor::CustomExecutor(executor) => executor.execute_sync(f).await,
            #[cfg(feature = "tokio")]
            SyncExecutor::SpawnBlocking(executor) => executor.execute_sync(f).await,
        }
    }
}

impl From<&SyncExecutor> for ExecutorStrategy {
    fn from(value: &SyncExecutor) -> Self {
        match value {
            SyncExecutor::CurrentContext(_) => Self::CurrentContext,
            SyncExecutor::CustomExecutor(_) => Self::CustomExecutor,
            #[cfg(feature = "tokio")]
            SyncExecutor::SpawnBlocking(_) => Self::SpawnBlocking,
        }
    }
}

/// A builder to replace the default sync executor strategy
/// with a caller-provided strategy.
///
/// # Examples
///
/// ```
/// use compute_heavy_future_executor::global_sync_strategy_builder;
///
/// # fn run() {
/// global_sync_strategy_builder()
///         .max_concurrency(10)
///         .initialize_current_context()
///         .unwrap();
/// # }
/// ```
#[must_use = "doesn't do anything unless used"]
#[derive(Default, Debug)]
pub struct SyncExecutorBuilder {
    max_concurrency: Option<usize>,
}

impl SyncExecutorBuilder {
    /// Set the max number of simultaneous futures processed by this executor.
    ///
    /// If this number is exceeded, the executor will wait to execute the
    /// input closure until a permit can be acquired.
    ///
    /// ## Default
    /// No maximum concurrency when strategies are manually built.
    ///
    /// For default strategies, the default concurrency limit will be the number of cpu cores.
    ///
    /// # Examples
    ///
    /// ```
    /// use compute_heavy_future_executor::global_sync_strategy_builder;
    ///
    /// # fn run() {
    /// global_sync_strategy_builder()
    ///         .max_concurrency(10)
    ///         .initialize_current_context()
    ///         .unwrap();
    /// # }
    pub fn max_concurrency(self, max_task_concurrency: usize) -> Self {
        Self {
            max_concurrency: Some(max_task_concurrency),
        }
    }

    /// Initializes a new (non-op) global strategy to wait in the current context.
    ///
    /// This is effectively a non-op wrapper that adds no special handling for the sync future
    /// besides optional concurrency control.
    ///
    /// This is the default if the `tokio` feature is disabled (with concurrency equal to cpu core count).
    ///
    /// # Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Examples
    ///
    /// ```
    /// use compute_heavy_future_executor::global_sync_strategy_builder;
    ///
    /// # async fn run() {
    /// global_sync_strategy_builder().initialize_current_context().unwrap();
    /// # }
    /// ```
    pub fn initialize_current_context(self) -> Result<(), Error> {
        let strategy =
            SyncExecutor::CurrentContext(CurrentContextExecutor::new(self.max_concurrency));
        set_sync_strategy(strategy)
    }

    /// Initializes a new global strategy to execute input closures by blocking on them inside the
    /// tokio blocking threadpool via Tokio's [`spawn_blocking`].
    ///
    /// Requires `tokio` feature.
    ///
    /// This is the default strategy if `tokio` feature is enabled, with a concurrency limit equal to the number of cpu cores.
    ///
    /// # Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Examples
    ///
    /// ```
    /// use compute_heavy_future_executor::global_sync_strategy_builder;
    ///
    /// # async fn run() {
    /// // this will include no concurrency limit when explicitly initialized
    /// // without a call to [`concurrency_limit()`]
    /// global_sync_strategy_builder().initialize_spawn_blocking().unwrap();
    /// # }
    /// ```
    /// [`spawn_blocking`]: tokio::task::spawn_blocking
    ///
    #[cfg(feature = "tokio")]
    pub fn initialize_spawn_blocking(self) -> Result<(), Error> {
        use spawn_blocking::SpawnBlockingExecutor;

        let strategy =
            SyncExecutor::SpawnBlocking(SpawnBlockingExecutor::new(self.max_concurrency));
        set_sync_strategy(strategy)
    }

    /// Accepts a closure that will accept an arbitrary closure and call it. The input
    /// function will be implicitly wrapped in a oneshot channel to avoid input/output types.
    ///
    /// Intended for injecting arbitrary runtimes/strategies or customizing existing ones.
    ///
    /// For instance, you could delegate to a [`Rayon threadpool`] or use Tokio's [`block_in_place`].
    /// See `tests/custom_executor_strategy.rs` for a `Rayon` example.
    ///
    /// # Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// # Examples
    ///
    /// ```
    /// use compute_heavy_future_executor::global_sync_strategy_builder;
    /// use compute_heavy_future_executor::CustomExecutorSyncClosure;
    ///
    /// # async fn run() {
    /// // caution: this will panic if used outside of tokio multithreaded runtime
    /// // this is a kind of dangerous strategy, read up on `block in place's` limitations
    /// // before using this approach
    /// let closure: CustomExecutorSyncClosure = Box::new(|f| {
    ///     Box::new(async move { Ok(tokio::task::block_in_place(move || f())) })
    /// });
    ///
    /// global_sync_strategy_builder().initialize_custom_executor(closure).unwrap();
    /// # }
    ///
    /// ```
    ///
    /// [`Rayon threadpool`]: https://docs.rs/rayon/latest/rayon/struct.ThreadPool.html
    /// [`block_in_place`]: https://docs.rs/tokio/latest/tokio/task/fn.block_in_place.html
    pub fn initialize_custom_executor(
        self,
        closure: CustomExecutorSyncClosure,
    ) -> Result<(), Error> {
        let strategy =
            SyncExecutor::CustomExecutor(CustomExecutor::new(closure, self.max_concurrency));
        set_sync_strategy(strategy)
    }
}
