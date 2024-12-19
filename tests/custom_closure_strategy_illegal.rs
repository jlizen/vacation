use std::any::Any;

use compute_heavy_future_executor::{initialize_custom_executor_strategy, CustomExecutorClosure};

#[tokio::test]
#[should_panic]
async fn custom_strategy_illegal_closure() {
    // this closure overwrites the input type, causing a mismatch
    let closure: CustomExecutorClosure =
        Box::new(|_| Box::new(async move { Ok(Box::new("foo") as Box<dyn Any + Send>) }));
    // this should panic due to bad closure
    initialize_custom_executor_strategy(closure).await;
}
