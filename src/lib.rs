pub fn test()-> &'static str {
    #[cfg(compute_heavy_tokio_multithreaded)]
    return "multithreaded";

    #[cfg(not(compute_heavy_tokio_multithreaded))]
    return "singlethreaded"
}