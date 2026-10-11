use ffmpeg::libavutil::ripemd::{DigestBits, Ripemd, context_size};

#[test]
fn all_digest_forms_construct_reborrow_reset_and_drop() {
    let sizes = [
        DigestBits::Bits128,
        DigestBits::Bits160,
        DigestBits::Bits256,
        DigestBits::Bits320,
    ];
    for bits in sizes {
        for _ in 0..32 {
            let mut context = Ripemd::new(bits).unwrap();
            let mut outer = context.as_mut();
            for other in sizes {
                let mut inner = outer.as_mut();
                inner.init(other).unwrap();
                let shared = inner.as_ref();
                let _shared_copy = shared;
            }
            outer.init(bits).unwrap();
        }
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
