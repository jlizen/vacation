use compute_heavy_future_executor::{global_sync_strategy_builder, Error};

#[test]
fn multiple_initialize_err() {
    global_sync_strategy_builder()
        .initialize_current_context()
        .unwrap();

    assert!(matches!(
        global_sync_strategy_builder().initialize_current_context(),
        Err(Error::AlreadyInitialized(_))
    ));
}
