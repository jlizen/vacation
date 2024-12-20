#[cfg(feature = "tokio")]
#[tokio::test]
async fn custom_strategy_cancellable() {
    use std::time::Duration;

    use compute_heavy_future_executor::{
        global_strategy_builder, spawn_compute_heavy_future, CustomExecutorClosure,
    };
    use tokio::select;

    let closure: CustomExecutorClosure = Box::new(|fut| {
        Box::new(async move {
            tokio::task::spawn(async move { fut.await })
                .await
                .map_err(|err| err.into())
        })
    });
    global_strategy_builder()
        .unwrap()
        .initialize_custom_executor(closure)
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
