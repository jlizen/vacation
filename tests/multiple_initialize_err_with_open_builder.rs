use compute_heavy_future_executor::{error::Error, global_strategy_builder};

#[test]
fn multiple_initialize_err_with_open_builder() {
    let builder = global_strategy_builder().unwrap(); // not yet initialized

    global_strategy_builder()
        .unwrap()
        .initialize_current_context()
        .unwrap();

    assert!(matches!(
        builder.initialize_current_context(),
        Err(Error::AlreadyInitialized(_))
    ));
}
