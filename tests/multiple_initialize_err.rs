use compute_heavy_future_executor::{error::Error, global_strategy};

#[test]
fn multiple_initialize_err() {
    global_strategy()
        .unwrap()
        .initialize_current_context()
        .unwrap();

    assert!(matches!(
        global_strategy(),
        Err(Error::AlreadyInitialized(_))
    ));
}
