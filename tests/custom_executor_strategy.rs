use compute_heavy_future_executor::{
    global_strategy, spawn_compute_heavy_future, CustomExecutorClosure,
};

#[tokio::test]
async fn custom_strategy_legal_closure() {
    let closure: CustomExecutorClosure = Box::new(|fut| {
        Box::new(async move {
            let res = fut.await;
            Ok(res)
        })
    });

    global_strategy()
        .unwrap()
        .initialize_custom_executor(closure)
        .unwrap();

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}
