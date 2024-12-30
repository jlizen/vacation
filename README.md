# vacation
 Vacation: give your (runtime) aworkers a break! 
 
## Overview

Today, when library authors are write async APIs, they don't have a good way to handle long-running sync segments.

An application author can use selective handling such as `tokio::task::spawn_blocking()` along with concurrency control to delegate sync segments to blocking threads. Or, they might send the work to a `rayon` threadpool.

But, library authors generally don't have this flexibility. As, they generally want to be agnostic across runtime. Or, even if they are `tokio`-specific, they generally don't want to call `tokio::task::spawn_blocking()` as it is
suboptimal without extra configuration (concurrency control) as well as highly opinionated to send the work across threads.

This library solves this problem by providing libray authors a static, globally scoped strategy that they can delegate blocking sync work to without drawing any conclusions about handling.

And then, the applications using the library can tune handling based on their preferred approach.

## Usage - Library Authors
For library authors, it's as simple as adding a dependency enabling `vacation` (perhaps behind a feature flag).

```
[dependencies]
vacation = { version = "0.1", default-features = false }
```

And then wrap any sync work by passing it as a closure to a global `execute_sync()` call:

```
use vacation::execute_sync;

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
Application authors will need to add this library as a a direct dependency in order to customize the execution strategy.
By default, the strategy is just a non-op.

### Simple example

```
[dependencies]
// enables `tokio` feature by default => spawn_blocking strategy
vacation = { version = "0.1" }
```

And then call the `initialize_tokio()` helper that uses some sensible defaults:
```
use vacation::initialize_tokio;

#[tokio::main]
async fn main() {
    initialize_tokio().unwrap();
}
```

### Rayon example
Or, you can add an alternate strategy, for instance a custom closure using Rayon.

```
[dependencies]
vacation = { version = "0.1", default-features = false }
// used for example with custom executor
rayon = "1"
```

```
use std::sync::OnceLock;
use rayon::ThreadPool;

use vacation::{
    global_sync_strategy_builder, CustomExecutorSyncClosure,
};

static THREADPOOL: OnceLock<ThreadPool> = OnceLock::new();

fn initialize_strategy() {
    THREADPOOL.setrayon::ThreadPoolBuilder::default().build().unwrap());

    let custom_closure: CustomExecutorSyncClosure =
        Box::new(|f| Box::new(async move { Ok(THREADPOOL.get().unwrap().spawn(f)) }));

    global_sync_strategy_builder()
        // probably no need for max concurrency as rayon already is defaulting to a thread per core
        // and using a task queue
        .initialize_custom_executor(custom_closure).unwrap();
}
```