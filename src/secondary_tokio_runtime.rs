use std::{future::Future, pin::Pin};

use tokio::{
    select,
    sync::mpsc::Sender,
};

use crate::{error::Error, ComputeHeavyFutureExecutor};

const DEFAULT_SECONDARY_EXECUTOR_NICENESS: i8 = 10;

fn default_secondary_executor_thread_count() -> usize {
    num_cpus::get()
}

type BackgroundFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub(crate) struct SecondaryTokioRuntimeExecutor {
    tx: Sender<BackgroundFuture>,
}

impl SecondaryTokioRuntimeExecutor {
    pub(crate) fn new(niceness: Option<i8>, thread_count: Option<usize>) -> Self {
        // channel is only for routing work to new task::spawn so should be very quick
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        let niceness = niceness.unwrap_or(DEFAULT_SECONDARY_EXECUTOR_NICENESS);

        // please https://github.com/rust-lang/rfcs/issues/671
        if !(-20..=19).contains(&niceness) {
            panic!("SecondaryTokioRuntimeExecutor initialized with a niceness outside of -20..=19 : {niceness}");
        }

        let thread_count =
            thread_count.unwrap_or_else(|| default_secondary_executor_thread_count());

        std::thread::Builder::new()
    .name("compute-heavy-executor".to_string())
    .spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .thread_name("compute-heavy-executor-pool-thread")
            .worker_threads(thread_count)
            .on_thread_start(move || unsafe {
                // Reduce thread pool thread niceness, so they are lower priority
                // than the foreground executor and don't interfere with I/O tasks
                #[cfg(target_os = "linux")]
                {
                    *libc::__errno_location() = 0;
                    if libc::nice(niceness.into()) == -1 && *libc::__errno_location() != 0 {
                        let error = std::io::Error::last_os_error();
                        log::error!("failed to set threadpool niceness of secondary compute-heavy tokio executor: {}", error);
                    }
                }
            })
            .enable_all()
            .build()
            .unwrap_or_else(|e| panic!("cpu heavy runtime failed_to_initialize: {}", e));

        rt.block_on(async {
            log::debug!("starting secondary compute-heavy tokio executor");
            while let Some(work) = rx.recv().await {
                tokio::task::spawn(work);
            }
        });
    })
    .unwrap_or_else(|e| panic!("secondary compute-heavy runtime thread failed_to_initialize: {}", e));

        Self { tx }
    }
}

impl ComputeHeavyFutureExecutor for SecondaryTokioRuntimeExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: std::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let (mut response_tx, response_rx) = tokio::sync::oneshot::channel();

        let background_future = Box::pin(async move {
            select!(
                _ = response_tx.closed() => {
                    // receiver already dropped, don't need to do anything
                    // cancel the background future
                }
                result = fut => {
                    // if this fails, the receiver already dropped, so we don't need to do anything
                    let _ = response_tx.send(result);
                }
            )
        });

        match self.tx.send(Box::pin(background_future)).await {
            Ok(_) => (),
            Err(err) => {
                panic!("secondary compute-heavy runtime channel cannot be reached: {err}")
            }
        }

        response_rx.await.map_err(|err| {
            Error::JoinError(format!(
                "error awaiting response from secondary tokio runtime: {err}"
            ))
        })
    }
}
