use std::time::Duration;

use vacation::{
    execute, global_strategy, init, ChanceOfBlocking, CustomClosureInput, CustomClosureOutput,
    ExecuteContext, ExecutorStrategy, GlobalStrategy,
};

#[tokio::test]
async fn custom_simple() {
    let custom_closure = |input: CustomClosureInput| {
        Box::new(async move {
            (input.work)();
            Ok(())
        }) as CustomClosureOutput
    };

    init().custom_executor(custom_closure).install().unwrap();

    let closure = || {
        std::thread::sleep(Duration::from_millis(15));
        5
    };

    let res = execute(
        closure,
        ExecuteContext {
            chance_of_blocking: ChanceOfBlocking::High,
            namespace: "test.operation",
        },
    )
    .await
    .unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_strategy(),
        GlobalStrategy::Initialized(ExecutorStrategy::Custom)
    );
}
