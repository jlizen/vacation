#[cfg(feature = "tokio")]
#[tokio::test]
async fn custom_strategy_legal_closure_tokio_spawn() {
    use compute_heavy_future_executor::{
        initialize_custom_executor_strategy, spawn_compute_heavy_future, CustomExecutorClosure,
    };

    let closure: CustomExecutorClosure = Box::new(|fut| {
        Box::new(async move {
            let handle = tokio::task::spawn(async move { fut.await });
            handle.await.map_err(|err| err.into())
        })
    });
    initialize_custom_executor_strategy(closure).await;

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}
