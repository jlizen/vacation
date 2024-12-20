use std::time::Duration;

use compute_heavy_future_executor::{global_strategy_builder, spawn_compute_heavy_future};
use tokio::select;

#[tokio::test]
async fn current_context_strategy() {
    global_strategy_builder()
        .unwrap()
        .initialize_current_context()
        .unwrap();

    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
    let future = async move {
        {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let _ = tx.send(());
        }
    };

    select! {
        _ = tokio::time::sleep(Duration::from_millis(4)) => { },
        _ = spawn_compute_heavy_future(future) => {}
    }

    tokio::time::sleep(Duration::from_millis(8)).await;

    // future should have been cancelled when spawn compute heavy future was dropped
    assert_eq!(
        rx.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Closed)
    );
}
