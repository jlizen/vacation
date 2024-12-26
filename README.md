# compute-heavy-future-executor
Experimental crate that adds additional executor patterns to use with frequently blocking futures.

Today, when library authors are write async APIs, they don't have a good way to handle long-running sync segments.

An application author can use selective handling such as `tokio::task::spawn_blocking()` along with concurrency control to delegate sync segments to blocking threads. Or, they might send the work to a `rayon` threadpool.

But, library authors generally don't have this flexibility. As, they generally want to be agnostic across runtime. Or, even if they are `tokio`-specific, they generally don't want to call `tokio::task::spawn_blocking()` as it is
suboptimal without extra configuration (concurrency control) as well as highly opinionated to send the work across threads.

This library aims to solve this problem by providing libray authors a static, globally scoped strategy that they can delegate blocking sync work to without drawing any conclusions about handling.

And then, the applications using the library can either rely on the default strategy that this package provides, or tune them with their preferred approach.

## Usage - Library Authors
For library authors, it's as simple as adding a dependency enabling `compute-heavy-future-executor` (perhaps behind a feature flag).

The below will default to 'current context' execution (ie non-op) unless the caller enables the tokio feature.
```
[dependencies]
compute-heavy-future-executor = { version = "0.1", default-features = false }
```

Meanwhile to be slightly more opinionated, the below will enable usage of `spawn_blocking` with concurrency control
by default unless the caller opts out:
```
[dependencies]
compute-heavy-future-executor = { version = "0.1" }
```

And then wrap any sync work by passing it as a closure to a global `execute_sync()` call:

```
use compute_heavy_future_executor::execute_sync;

fn sync_work(input: String)-> u8 {
    std::thread::sleep(std::time::Duration::from_secs(5));
    println!("{input}");
    5
}
pub async fn a_future_that_has_blocking_sync_work() -> u8 {
    // relies on caller-specified strategy for translating execute_sync into a future that won't
    // block the current worker thread
    execute_sync(move || { sync_work("foo".to_string()) }).await.unwrap()
}

```

## Usage - Application owners
Application authors can benefit from this crate with no application code changes, if you are using
a library that is itself using this crate.

If you want to customize the strategy beyond defaults, they can add
`compute-heavy-future-executor` to their dependencies:

```
[dependencies]
// enables tokio and therefore spawn_blocking strategy by default
compute-heavy-future-executor = { version = "0.1" }
// used for example with custom executor
rayon = "1"
```

And then configure your global strategy as desired. For instance, see below for usage of rayon
instead of `spawn_blocking()`.

```
use std::sync::OnceLock;
use rayon::ThreadPool;

use compute_heavy_future_executor::{
    global_sync_strategy_builder, CustomExecutorSyncClosure,
};

static THREADPOOL: OnceLock<ThreadPool> = OnceLock::new();

fn initialize_strategy() {
    THREADPOOL.set(|| rayon::ThreadPoolBuilder::default().build().unwrap());

    let custom_closure: CustomExecutorSyncClosure =
        Box::new(|f| Box::new(async move { Ok(THREADPOOL.get().unwrap().spawn(f)) }));

    global_sync_strategy_builder()
        // probably no need for max concurrency as rayon already is defaulting to a thread per core
        // and using a task queue
        .initialize_custom_executor(custom_closure).unwrap();
}

```