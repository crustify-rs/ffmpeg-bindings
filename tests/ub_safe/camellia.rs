use ffmpeg::libavutil::camellia::{Camellia, context_size};

#[test]
fn reinitialize_across_reborrows() {
    let mut context = {
        let key = vec![0x42; 16].into_boxed_slice();
        Camellia::new(&key).unwrap()
    }; // C may not retain the key.
    for len in [24, 32, 16, 32, 24] {
        let key = vec![0xab; len].into_boxed_slice();
        let mut outer = context.as_mut();
        outer.as_mut().init(&key).unwrap();
        let _shared = outer.as_ref();
        outer.init(&key).unwrap();
    }
}

#[test]
fn concurrent_constructors_serialize_global_table() {
    let threads: Vec<_> = (0..8)
        .map(|byte| {
            std::thread::spawn(move || {
                for len in [16, 24, 32, 16, 32] {
                    let mut context = Camellia::new(&vec![byte; len]).unwrap();
                    context.as_mut().init(&[byte; 24]).unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

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
