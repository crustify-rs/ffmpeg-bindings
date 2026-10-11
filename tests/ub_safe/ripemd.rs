use ffmpeg::libavutil::ripemd::context_size;

#[test]
fn batch10_immutable_size_across_threads() {
    let size: &'static i32 = context_size();
    assert!(*size > 0);
    let workers: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(move || {
                for _ in 0..100 {
                    assert!(std::ptr::eq(size, context_size()));
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}
