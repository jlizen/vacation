#[tokio::test]
async fn default_to_execute_directly() {
    use std::time::Duration;

    use vacation::{execute, global_strategy, ChanceOfBlocking, ExecutorStrategy, GlobalStrategy};

    // this is a tokio test but we haven't enabled the tokio config flag

    let closure = || {
        std::thread::sleep(Duration::from_millis(5));
        5
    };

    let res = execute(closure, ChanceOfBlocking::High, "test.operation")
        .await
        .unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_strategy(),
        GlobalStrategy::Default(ExecutorStrategy::ExecuteDirectly)
    );

    // make sure we can continue to call it without failures due to repeat initialization
    let res = execute(closure, ChanceOfBlocking::High, "test.operation")
        .await
        .unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_strategy(),
        GlobalStrategy::Default(ExecutorStrategy::ExecuteDirectly)
    );
}
