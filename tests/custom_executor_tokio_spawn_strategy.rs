#[cfg(feature = "tokio")]
#[tokio::test]
async fn custom_strategy_tokio_spawn() {
    use compute_heavy_future_executor::{
        global_strategy_builder, spawn_compute_heavy_future, CustomExecutorClosure,
    };

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

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}
