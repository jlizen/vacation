use compute_heavy_future_executor::initialize_current_context_strategy;

#[tokio::test]
#[should_panic]
async fn multiple_initializes_panic() {
    initialize_current_context_strategy();
    initialize_current_context_strategy();
}
