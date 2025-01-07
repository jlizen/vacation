# vacation
Vacation: Give your (runtime) workers a break!

Better handling for compute-heavy segments of work that might block your async worker threads.
 
## Overview
When library authors write async APIs, they by default don't have a good way to handle long-running sync segments.

An application author can use selective handling such as `tokio::task::spawn_blocking()` along with concurrency control to delegate sync segments to blocking threads. Or, they might send the work to a `rayon` threadpool.

But, library authors generally don't have this flexibility. As, they generally want to be agnostic across runtime. Or, even if they are `tokio`-specific, they generally don't want to call `tokio::task::spawn_blocking()` as it is
suboptimal without extra configuration (concurrency control) as well as highly opinionated to send the work across threads.

This library solves this problem by providing libray authors a static, globally scoped strategy that they can delegate blocking sync work to without drawing any conclusions about handling.

And then, the applications using the library can tune handling based on their preferred approach.

## Usage - Library Authors
### When working with async/await
The simplest way for a library author to use this library is by delegating
work via [`execute()`] and awaiting it.

This requires no feature flags:
```ignore
[dependencies]
vacation = "0.1"
```

And then wrap any sync work by passing it as a closure to a global `execute()` call:

```
fn sync_work(input: String)-> u8 {
    std::thread::sleep(std::time::Duration::from_secs(5));
    println!("{input}");
    5
}
pub async fn a_future_that_has_blocking_sync_work() -> u8 {
    // relies on application-specified strategy for translating execute into a future that won't
    // block the current worker thread
    vacation::execute(move || { sync_work("foo".to_string()) }, vacation::ChanceOfBlocking::High, "example.operation").await.unwrap()
}
```

### Integrating with manually implemented futures
If your library is instead manually implementing the [`std::future::Future`] trait, you might instead
want to enable the `future` feature flag:

```ignore
[dependencies]
vacation = { version = "0.1", features = ["future"] }
```

This enables the [`future::FutureBuilder`] api along with the two types of `Vacation` futures it can generate:
- [`future::OffloadFirst`] - delegate work to vacation, and then process the results into an inner future and poll the inner future to completion
- [`future::OffloadWithFuture`] - poll the inner future, while also using a callback to retrieve any vacation work and polling it alongisde the inner future 

## Usage - Application owners
Application authors will need to add this library as a a direct dependency in order to customize the execution strategy
beyond the default no-op.

Application authors can also call [`execute()`] if there are application-layer compute-heavy segments in futures.

### Simple example

```ignore
[dependencies]
# tokio feature adds the spawn_blocking executor
vacation = { version = "0.1", features = ["tokio"] }
```

And then call the `install_tokio_strategy()` helper that uses some sensible defaults (spawn blocking w/ concurrency control equal to cpu cores):
```
#[tokio::main]
async fn main() {
# #[cfg(feature="tokio")]
#  {
    vacation::install_tokio_strategy().unwrap();

    // if wanting to delegate work to vacation:
    let vacation_future = vacation::execute(|| {
        // represents compute heavy work
        std::thread::sleep(std::time::Duration::from_millis(500));
        5
    }, vacation::ChanceOfBlocking::High, "example.operation");
    
    assert_eq!(vacation_future.await.unwrap(), 5);
#    }
}
```

### Builder example
To customize a bit more, you can select from a cookie cutter strategy and configure concurrency, via `vacation::init()`:
```ignore
[dependencies]
# tokio feature adds spawn_blocking executor
vacation = { version = "0.1", features = ["tokio"] }
```

```
#[tokio::main]
async fn main() {
# #[cfg(feature="tokio")]
# {
    vacation::init().max_concurrency(5).spawn_blocking().install();
# }
}
```

### Rayon example
Or, you can add an alternate strategy, for instance a custom closure using Rayon.

```ignore
[dependencies]
vacation = "0.1"
// used for example with custom executor
rayon = "1"
```

```
use std::sync::OnceLock;
use rayon::ThreadPool;

static THREADPOOL: OnceLock<ThreadPool> = OnceLock::new();

fn initialize_strategy() {
    THREADPOOL.set(rayon::ThreadPoolBuilder::default().build().unwrap());

    let custom_closure = |input: vacation::CustomClosureInput| {
        Box::new(async move { Ok(THREADPOOL.get().unwrap().spawn(input.work)) }) as vacation::CustomClosureOutput
    };

    vacation::init()
        // probably no need for max concurrency as rayon already is defaulting to a thread per core
        // and using a task queue
        .custom_executor(custom_closure).install().unwrap();
}
```

## Feature flags

This library uses feature flags to reduce the dependency tree and amount of compiled code. Feature enablement does not change any runtime behavior
without corresponding code calling the feature-gated APIs.

The features are:
- `tokio` - enables the [`ExecutorStrategy::SpawnBlocking`] strategy, which is a version of tokio's `spawn_blocking` with concurrency control baked in
- `future` - wrapper future types, intended for library authors that have manual future implementations