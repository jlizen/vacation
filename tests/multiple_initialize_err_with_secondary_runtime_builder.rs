#[cfg(feature = "secondary_tokio_runtime")]
#[test]
fn multiple_initialize_err_with_secondary_runtime_builder() {
    use compute_heavy_future_executor::{error::Error, global_strategy};

    let builder = global_strategy().unwrap().secondary_tokio_runtime_builder(); // not yet initialized

    global_strategy()
        .unwrap()
        .initialize_current_context()
        .unwrap();

    assert!(matches!(
        builder.initialize(),
        Err(Error::AlreadyInitialized(_))
    ));
}
