#[cfg(feature = "secondary_tokio_runtime")]
#[tokio::test]
#[should_panic]
async fn secondary_tokio_runtime_builder_disallowed_config() {
    use compute_heavy_future_executor::{error::Error, global_strategy_builder};

    let res = global_strategy_builder()
        .unwrap()
        .secondary_tokio_runtime_builder()
        .channel_size(10)
        .niceness(5);

    assert!(matches!(res, Err(Error::InvalidConfig(_))));
}
