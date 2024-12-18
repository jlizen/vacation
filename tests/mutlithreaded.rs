#![allow(unexpected_cfgs)]
#![cfg(compute_heavy_tokio_executor_multithreaded)]

#[cfg(test)]
mod multithreaded {
    use compute_heavy_tokio_executor::test;

    #[test]
    fn test_multithreaded() {
        assert_eq!(test(), "multithreaded");
    }
}
