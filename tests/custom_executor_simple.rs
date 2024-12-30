use std::time::Duration;

use vacation::{
    execute_sync, global_sync_strategy, global_sync_strategy_builder, ChanceOfBlocking,
    CustomExecutorSyncClosure, ExecutorStrategy, GlobalStrategy,
};

#[tokio::test]
async fn custom_executor_simple() {
    let custom_closure: CustomExecutorSyncClosure = Box::new(|f| {
        Box::new(async move {
            f();
            Ok(())
        })
    });

    global_sync_strategy_builder()
        .initialize_custom_executor(custom_closure)
        .unwrap();

    let closure = || {
        std::thread::sleep(Duration::from_millis(15));
        5
    };

    let res = execute_sync(closure, ChanceOfBlocking::High).await.unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_sync_strategy(),
        GlobalStrategy::Initialized(ExecutorStrategy::CustomExecutor)
    );
}
