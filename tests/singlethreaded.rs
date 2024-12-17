#![cfg(not(compute_heavy_tokio_multithreaded))]

#[cfg(test)]
mod singlethreaded {
    use compute_heavy_tokio_executor::test;

    #[test]
    fn test_singlethreaded() {
        assert_eq!(test(), "singlethreaded");
    }
}
