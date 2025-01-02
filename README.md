# vacation
 Vacation: Give your (runtime) workers a break! 
 
## Overview

Today, when library authors write async APIs, they don't have a good way to handle long-running sync segments.

An application author can use selective handling such as `tokio::task::spawn_blocking()` along with concurrency control to delegate sync segments to blocking threads. Or, they might send the work to a `rayon` threadpool.

But, library authors generally don't have this flexibility. As, they generally want to be agnostic across runtime. Or, even if they are `tokio`-specific, they generally don't want to call `tokio::task::spawn_blocking()` as it is
suboptimal without extra configuration (concurrency control) as well as highly opinionated to send the work across threads.

This library solves this problem by providing libray authors a static, globally scoped strategy that they can delegate blocking sync work to without drawing any conclusions about handling.

And then, the applications using the library can tune handling based on their preferred approach.

## Usage - Library Authors
For library authors, it's as simple as adding a dependency enabling `vacation` (perhaps behind a feature flag).

```ignore
[dependencies]
vacation = { version = "0.1", default-features = false }
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
    vacation::execute(move || { sync_work("foo".to_string()) }, vacation::ChanceOfBlocking::High).await.unwrap()
}

```

## Usage - Application owners
Application authors will need to add this library as a a direct dependency in order to customize the execution strategy.
By default, the strategy is just a non-op.

### Simple example

```ignore
[dependencies]
// enables `tokio` feature by default => spawn_blocking strategy
vacation = { version = "0.1" }
```

And then call the `install_tokio_strategy()` helper that uses some sensible defaults:
```
#[tokio::main]
async fn main() {
    vacation::install_tokio_strategy().unwrap();
}
```

### Rayon example
Or, you can add an alternate strategy, for instance a custom closure using Rayon.

```ignore
[dependencies]
vacation = { version = "0.1", default-features = false }
// used for example with custom executor
rayon = "1"
```

```
use std::sync::OnceLock;
use rayon::ThreadPool;

static THREADPOOL: OnceLock<ThreadPool> = OnceLock::new();

fn initialize_strategy() {
    THREADPOOL.set(rayon::ThreadPoolBuilder::default().build().unwrap());

    let custom_closure = |f: vacation::CustomClosureInput| {
        Box::new(async move { Ok(THREADPOOL.get().unwrap().spawn(f)) }) as vacation::CustomClosureOutput
    };

    vacation::init()
        // probably no need for max concurrency as rayon already is defaulting to a thread per core
        // and using a task queue
        .custom_executor(custom_closure).install().unwrap();
}
```