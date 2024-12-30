use vacation::{init, Error};

#[test]
fn multiple_initialize_err() {
    init().execute_directly().install().unwrap();

    assert!(matches!(
        init().execute_directly().install(),
        Err(Error::AlreadyInitialized(_))
    ));
}
