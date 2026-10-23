use ffmpeg_bindings::libavutil::libavutil::aes_ctr::{AesCtr, AesCtrRef};

fn shared_views(view: AesCtrRef<'_>) {
    let _copy = view;
    let _second = view;
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn lifecycle_reborrows() {
    for seed in 0..32 {
        let mut owner = {
            let key = [seed; 16];
            AesCtr::new(&key).unwrap()
        }; // the constructor must copy, not retain, the stack key.
        shared_views(owner.as_ref());
        let mut handle = owner.as_mut();
        shared_views(handle.as_ref());
        {
            let mut reborrow = handle.as_mut();
            reborrow.init(&[seed + 1; 16]).unwrap();
            shared_views(reborrow.as_ref());
        }
        handle.init(&[seed + 2; 16]).unwrap();
        shared_views(handle.as_ref());
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn partial_stream_iv_changes_and_reborrows() {
    for seed in 0..16 {
        let mut owner = AesCtr::new(&[seed; 16]).unwrap();
        let nonce = [seed; 8];
        owner.as_mut().set_iv(&nonce);
        let snapshot = owner.as_ref().iv();
        let source = [0x5a; 133];
        let mut destination = [0x99; 133];
        for len in [0, 1, 14, 1, 16, 17, 31, 64, 129] {
            let mut outer = owner.as_mut();
            outer
                .as_mut()
                .crypt(&mut destination[1..1 + len], &source[2..2 + len])
                .unwrap();
            let _snapshot = outer.as_ref().iv();
            outer.crypt_in_place(&mut destination[2..2 + len]).unwrap();
            outer.increment_iv();
            outer.set_full_iv(&[0xff; 16]);
        }
        // The previous snapshot owns bytes and survives mutation and parent drop.
        owner.as_mut().increment_iv();
        assert_eq!(owner.as_ref().iv(), [0; 16]);
        owner.as_mut().set_random_iv();
        assert_eq!(&owner.as_ref().iv()[8..], &[0; 8]);
        drop(owner);
        assert_eq!(&snapshot[..8], &nonce);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn parallel_random_iv_and_seed_access() {
    let threads: Vec<_> = (0..4)
        .map(|seed| {
            std::thread::spawn(move || {
                let mut owner = AesCtr::new(&[seed; 16]).unwrap();
                for _ in 0..16 {
                    owner.as_mut().set_random_iv();
                    let _seed = ffmpeg_bindings::libavutil::libavutil::random_seed::get();
                    owner.as_mut().crypt_in_place(&mut [0; 17]).unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}
