use compute_heavy_future_executor::{
    initialize_custom_executor_strategy, spawn_compute_heavy_future, CustomExecutorClosure,
};

#[tokio::test]
async fn custom_strategy_legal_closure() {
    let closure: CustomExecutorClosure = Box::new(|fut| {
        Box::new(async move {
            let res = fut.await;
            Ok(res)
        })
    });

    initialize_custom_executor_strategy(closure);

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}
