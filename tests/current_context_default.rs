#[tokio::test]
async fn default_to_current_context() {
    use std::time::Duration;

    use vacation::{
        execute_sync, global_sync_strategy, ChanceOfBlocking, ExecutorStrategy, GlobalStrategy,
    };

    // this is a tokio test but we haven't enabled the tokio config flag

    let closure = || {
        std::thread::sleep(Duration::from_millis(5));
        5
    };

    let res = execute_sync(closure, ChanceOfBlocking::High).await.unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_sync_strategy(),
        GlobalStrategy::Default(ExecutorStrategy::CurrentContext)
    );
}
