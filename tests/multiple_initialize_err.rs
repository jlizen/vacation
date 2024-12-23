use compute_heavy_future_executor::{error::Error, global_strategy_builder};

#[test]
fn multiple_initialize_err() {
    global_strategy_builder()
        .initialize_current_context()
        .unwrap();

    assert!(matches!(
        global_strategy_builder().initialize_current_context(),
        Err(Error::AlreadyInitialized(_))
    ));
}
