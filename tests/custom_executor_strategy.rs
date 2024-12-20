use compute_heavy_future_executor::{
    global_strategy, global_strategy_builder, spawn_compute_heavy_future, CurrentStrategy,
    CustomExecutorClosure, ExecutorStrategy,
};
#[tokio::test]
async fn custom_strategy() {
    let closure: CustomExecutorClosure = Box::new(|fut| {
        Box::new(async move {
            let res = fut.await;
            Ok(res)
        })
    });

    global_strategy_builder()
        .unwrap()
        .initialize_custom_executor(closure)
        .unwrap();

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_strategy(),
        CurrentStrategy::Initialized(ExecutorStrategy::CustomExecutor)
    );
}
