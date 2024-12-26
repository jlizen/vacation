#[cfg(not(feature = "tokio"))]
#[tokio::test]
async fn default_to_current_context() {
    use std::time::Duration;

    use compute_heavy_future_executor::{
        execute_sync, global_sync_strategy, ExecutorStrategy, GlobalStrategy,
    };

    // this is a tokio test but we haven't enabled the tokio config flag

    let closure = || {
        std::thread::sleep(Duration::from_millis(5));
        5
    };

    let res = execute_sync(closure).await.unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_sync_strategy(),
        GlobalStrategy::Default(ExecutorStrategy::CurrentContext)
    );
}
