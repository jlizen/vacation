#[cfg(not(feature = "tokio"))]
#[tokio::test]
async fn default_to_current_context_tokio_single_threaded() {
    use compute_heavy_future_executor::{
        execute_compute_heavy_future, global_strategy, CurrentStrategy, ExecutorStrategy,
    };

    // this is a tokio test but we haven't enabled the tokio config flag

    let future = async { 5 };

    let res = execute_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_strategy(),
        CurrentStrategy::Default(ExecutorStrategy::CurrentContext)
    );
}
