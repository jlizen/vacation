use compute_heavy_future_executor::{
    global_strategy, global_strategy_builder, spawn_compute_heavy_future, CurrentStrategy,
    ExecutorStrategy,
};

#[tokio::test]
async fn current_context_strategy() {
    global_strategy_builder()
        .unwrap()
        .initialize_current_context()
        .unwrap();

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_strategy(),
        CurrentStrategy::Initialized(ExecutorStrategy::CurrentContext)
    );
}
