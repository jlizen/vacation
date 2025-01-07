pub(crate) mod custom;
pub(crate) mod execute_directly;
#[cfg(feature = "tokio")]
pub(crate) mod spawn_blocking;

use std::{future::Future, sync::OnceLock};

use custom::{Custom, CustomClosure};
use execute_directly::ExecuteDirectly;

use crate::{CustomClosureInput, Error, ExecutorStrategy, GlobalStrategy};

fn set_global_strategy(strategy: Executor) -> Result<(), Error> {
    GLOBAL_EXECUTOR_STRATEGY
        .set(strategy)
        .map_err(|_| Error::AlreadyInitialized(GLOBAL_EXECUTOR_STRATEGY.get().unwrap().into()))?;

    log::info!(
        "initialized vacation synchronous executor strategy - {:#?}",
        global_strategy()
    );

    Ok(())
}

/// Get the currently initialized sync strategy,
/// or the default strategy for the current feature in case no strategy has been loaded.
///
/// See [`ExecutorBuilder`] for details on strategies.
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
pub fn global_strategy() -> GlobalStrategy {
    match GLOBAL_EXECUTOR_STRATEGY.get() {
        Some(strategy) => GlobalStrategy::Initialized(strategy.into()),
        None => GlobalStrategy::Default(get_default_strategy().into()),
    }
}

pub(crate) fn get_global_executor() -> &'static Executor {
    GLOBAL_EXECUTOR_STRATEGY
        .get()
        .unwrap_or_else(|| get_default_strategy())
}

pub(crate) fn get_default_strategy() -> &'static Executor {
    DEFAULT_GLOBAL_EXECUTOR_STRATEGY.get_or_init(|| {
        log::warn!(
            "Defaulting to ExecuteDirectly (non-op) strategy for vacation compute-heavy future executor"
        );
        Executor::ExecuteDirectly(ExecuteDirectly::new(None))
    })
}

/// The stored strategy used to spawn compute-heavy futures.
static GLOBAL_EXECUTOR_STRATEGY: OnceLock<Executor> = OnceLock::new();

/// The fallback strategy used in case no strategy is explicitly set
static DEFAULT_GLOBAL_EXECUTOR_STRATEGY: OnceLock<Executor> = OnceLock::new();

#[non_exhaustive]
pub(crate) enum Executor {
    /// A non-op strategy that runs the function in the current context
    ExecuteDirectly(execute_directly::ExecuteDirectly),
    /// User-provided closure
    Custom(custom::Custom),
    /// tokio task::spawn_blocking
    #[cfg(feature = "tokio")]
    SpawnBlocking(spawn_blocking::SpawnBlocking),
}

impl From<&Executor> for ExecutorStrategy {
    fn from(value: &Executor) -> Self {
        match value {
            Executor::ExecuteDirectly(_) => Self::ExecuteDirectly,
            Executor::Custom(_) => Self::Custom,
            #[cfg(feature = "tokio")]
            Executor::SpawnBlocking(_) => Self::SpawnBlocking,
        }
    }
}

/// Initialize a set of sensible defaults for a tokio runtime:
///
/// - [`ExecutorBuilder::spawn_blocking`] strategy
/// - Max concurrency equal to the cpu core count.
///
/// Stores the current tokio runtime to spawn tasks with. To use an alternate
/// runtime, use [`ExecutorBuilder::spawn_blocking_with_handle`].
///
/// Only available with the `tokio` feature.
///
/// # Panic
/// Calling this from outside a tokio runtime will panic.
///
/// # Errors
/// Returns an error if the global strategy is already initialized.
/// It can only be initialized once.
///
/// # Examples
///
/// ```
/// # fn run() {
/// vacation::install_tokio_strategy().unwrap();
/// # }
/// ```
#[cfg(feature = "tokio")]
pub fn install_tokio_strategy() -> Result<(), Error> {
    super::init()
        .max_concurrency(num_cpus::get())
        .spawn_blocking()
        .install()
}

/// A builder to replace the default sync executor strategy
/// with a caller-provided strategy.
///
/// # Examples
///
/// ```
/// # fn run() {
///
/// let cores = num_cpus::get();
///
/// vacation::init()
///         .max_concurrency(cores)
///         .execute_directly()
///         .install()
///         .unwrap();
/// # }
/// ```
#[must_use = "doesn't do anything unless used"]
#[derive(Default)]
pub struct ExecutorBuilder<Strategy> {
    pub(crate) max_concurrency: Option<usize>,
    pub(crate) strategy: Strategy,
}

impl<Strategy: std::fmt::Debug> std::fmt::Debug for ExecutorBuilder<Strategy> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutorBuilder")
            .field("max_concurrency", &self.max_concurrency)
            .field("strategy", &self.strategy)
            .finish()
    }
}

#[derive(Debug)]
pub struct NeedsStrategy;
pub enum HasStrategy {
    ExecuteDirectly,
    #[cfg(feature = "tokio")]
    SpawnBlocking(tokio::runtime::Handle),
    Custom(CustomClosure),
}

impl std::fmt::Debug for HasStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExecuteDirectly => write!(f, "ExecuteDirectly"),
            #[cfg(feature = "tokio")]
            Self::SpawnBlocking(handle) => f.debug_tuple("SpawnBlocking").field(handle).finish(),
            Self::Custom(_) => f.debug_tuple("Custom").finish(),
        }
    }
}

impl<Strategy> ExecutorBuilder<Strategy> {
    /// Set the max number of simultaneous futures processed by this executor.
    ///
    /// If this number is exceeded, the executor will wait to execute the
    /// input closure until a permit can be acquired.
    ///
    /// A good value tends to be the number of cpu cores on your machine.
    ///
    /// ## Default
    /// No maximum concurrency.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn run() {
    /// vacation::init()
    ///         .max_concurrency(10)
    ///         .execute_directly()
    ///         .install()
    ///         .unwrap();
    /// # }
    #[must_use = "doesn't do anything unless used with a strategy"]
    pub fn max_concurrency(self, max_task_concurrency: usize) -> Self {
        Self {
            max_concurrency: Some(max_task_concurrency),
            ..self
        }
    }
    /// Initializes a new (non-op) global strategy to wait in the current context.
    ///
    /// This is effectively a non-op wrapper that adds no special handling for the sync future
    /// besides optional concurrency control.
    ///
    /// This is the default strategy if nothing is initialized, with no max concurrency.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn run() {
    /// vacation::init().execute_directly().install().unwrap();
    /// # }
    /// ```
    #[must_use = "doesn't do anything unless install()-ed"]
    pub fn execute_directly(self) -> ExecutorBuilder<HasStrategy> {
        ExecutorBuilder::<HasStrategy> {
            strategy: HasStrategy::ExecuteDirectly,
            max_concurrency: self.max_concurrency,
        }
    }

    /// Initializes a new global strategy to execute input closures by blocking on them inside the
    /// tokio blocking threadpool via Tokio's [`spawn_blocking`].
    ///
    /// Stores the current tokio runtime to spawn tasks with. To use an alternate
    /// runtime, use [`ExecutorBuilder::spawn_blocking_with_handle`].
    ///
    /// Requires `tokio` feature.
    ///
    /// # Panic
    /// Calling this from outside a tokio runtime will panic.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn run() {
    /// // this will include no concurrency limit when explicitly initialized
    /// // without a call to [`concurrency_limit()`]
    /// vacation::init().spawn_blocking().install().unwrap();
    /// # }
    /// ```
    /// [`spawn_blocking`]: tokio::task::spawn_blocking
    ///
    #[must_use = "doesn't do anything unless install()-ed"]
    #[cfg(feature = "tokio")]
    pub fn spawn_blocking(self) -> ExecutorBuilder<HasStrategy> {
        ExecutorBuilder::<HasStrategy> {
            strategy: HasStrategy::SpawnBlocking(tokio::runtime::Handle::current()),
            max_concurrency: self.max_concurrency,
        }
    }

    /// Initializes a new global strategy to execute input closures by blocking on them inside the
    /// tokio blocking threadpool via Tokio's [`spawn_blocking`], on a specific runtime.
    ///
    /// Uses the provided tokio runtime handle to decide which runtime to `spawn_blocking` onto.
    ///
    /// Requires `tokio` feature.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn run() {
    /// // this will include no concurrency limit when explicitly initialized
    /// // without a call to [`concurrency_limit()`]
    /// let handle = tokio::runtime::Handle::current();
    /// vacation::init().spawn_blocking_with_handle(handle).install().unwrap();
    /// # }
    /// ```
    /// [`spawn_blocking`]: tokio::task::spawn_blocking
    ///
    #[must_use = "doesn't do anything unless install()-ed"]
    #[cfg(feature = "tokio")]
    pub fn spawn_blocking_with_handle(
        self,
        runtime_handle: tokio::runtime::Handle,
    ) -> ExecutorBuilder<HasStrategy> {
        ExecutorBuilder::<HasStrategy> {
            strategy: HasStrategy::SpawnBlocking(runtime_handle),
            max_concurrency: self.max_concurrency,
        }
    }

    /// Accepts a closure that will accept an arbitrary closure and call it. The input
    /// function will be implicitly wrapped in a oneshot channel to avoid input/output types.
    ///
    /// Intended for injecting arbitrary runtimes/strategies or customizing existing ones.
    ///
    /// For instance, you could delegate to a [`Rayon threadpool`] or use Tokio's [`block_in_place`].
    /// See `tests/custom_executor_strategy.rs` for a `Rayon` example.
    ///
    /// # Closure inputs
    /// Along with a type-erased input function containing work, the [`CustomClosureInput`] contains:
    /// - chance of blocking
    /// - operation namespace
    ///
    /// This allows conditional logical such as different prioritization. You can discard
    /// these inputs if you don't need them.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn run() {
    /// // caution: this will panic if used outside of tokio multithreaded runtime
    /// // this is a kind of dangerous strategy, read up on `block in place's` limitations
    /// // before using this approach
    /// let closure = |input: vacation::CustomClosureInput| {
    ///     Box::new(async move { Ok(tokio::task::block_in_place(move || (input.work)())) }) as vacation::CustomClosureOutput
    /// };
    ///
    /// vacation::init().custom_executor(closure).install().unwrap();
    /// # }
    ///
    /// ```
    ///
    /// [`Rayon threadpool`]: https://docs.rs/rayon/latest/rayon/struct.ThreadPool.html
    /// [`block_in_place`]: https://docs.rs/tokio/latest/tokio/task/fn.block_in_place.html
    #[must_use = "doesn't do anything unless install()-ed"]
    pub fn custom_executor<Closure>(self, closure: Closure) -> ExecutorBuilder<HasStrategy>
    where
        Closure: Fn(
                CustomClosureInput,
            ) -> Box<
                dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>
                    + Send
                    + 'static,
            > + Send
            + Sync
            + 'static,
    {
        ExecutorBuilder::<HasStrategy> {
            strategy: HasStrategy::Custom(Box::new(closure)),
            max_concurrency: self.max_concurrency,
        }
    }
}

impl ExecutorBuilder<HasStrategy> {
    /// Initializes the loaded configuration and stores it as a global strategy.
    ///
    /// # Error
    /// Returns an error if the global strategy is already initialized.
    /// It can only be initialized once.
    ///
    /// /// # Examples
    ///
    /// ```
    /// # async fn run() {
    /// vacation::init().execute_directly().install().unwrap();
    /// # }
    /// ```
    pub fn install(self) -> Result<(), Error> {
        let executor = match self.strategy {
            HasStrategy::ExecuteDirectly => {
                Executor::ExecuteDirectly(ExecuteDirectly::new(self.max_concurrency))
            }
            #[cfg(feature = "tokio")]
            HasStrategy::SpawnBlocking(handle) => Executor::SpawnBlocking(
                spawn_blocking::SpawnBlocking::new(handle, self.max_concurrency),
            ),
            HasStrategy::Custom(closure) => {
                Executor::Custom(Custom::new(closure, self.max_concurrency))
            }
        };

        set_global_strategy(executor)
    }
}
