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

#[test]
pub(super) fn batch2_foreign_writes_across_reborrows() {
    let data: Vec<u8> = (0..1025).map(|i| i as u8).collect();
    for algorithm in [
        DigestBits::Bits128,
        DigestBits::Bits160,
        DigestBits::Bits256,
        DigestBits::Bits320,
    ] {
        let mut owner = Ripemd::new(algorithm).unwrap();
        for length in [0, 1, 55, 56, 63, 64, 65, 111, 112, 127, 128, 129, 1025] {
            let mut parent = owner.as_mut();
            parent.init(algorithm).unwrap();
            for chunk in data[..length].chunks(7) {
                let mut input = chunk.to_vec();
                parent.as_mut().update(&input);
                input.fill(0); // input is not retained across the next reborrow
            }
            parent.as_mut().update(&[]);
            let digest = parent.as_mut().finalize();
            assert!(digest[algorithm.digest_len()..].iter().all(|&b| b == 0));
            // Repeated final/update remains memory-safe, though not a new message.
            parent.as_mut().finalize();
            parent.as_mut().update(b"after final");
        }
    }
}
