#[cfg(feature = "tokio_block_in_place")]
#[tokio::test]
async fn block_in_place_wrong_runtime() {
    use compute_heavy_future_executor::{global_strategy_builder, Error};

    let res = global_strategy_builder()
        .unwrap()
        .initialize_block_in_place();

    assert!(matches!(res, Err(Error::InvalidConfig(_))));
}
