use ffibox::CZeroable;
use ffmpeg::libavutil::des::{Des, DesBox};

#[test]
fn inline_heap_fields_and_reborrows() {
    #[repr(align(8))]
    struct KeyStorage([u8; 25]);
    let unaligned = KeyStorage([0xA5; 25]);
    let mut inline = Des::new(&[1; 8], false).unwrap();
    let mut heap = Des::new_boxed(&[2; 24], true).unwrap();
    let mut zeroed = Des::zeroed();
    let mut allocated = DesBox::new().unwrap();
    let mut allocated_zeroed = DesBox::new_zeroed().unwrap();
    for mut handle in [
        inline.as_mut(),
        heap.as_mut(),
        zeroed.as_mut(),
        allocated.as_mut(),
        allocated_zeroed.as_mut(),
    ] {
        let shared = handle.as_ref();
        let copy = shared;
        assert_eq!(shared.round_keys(), copy.round_keys());
        for flag in [i32::MIN, -1, 0, 1, i32::MAX] {
            handle.as_mut().set_triple_des(flag);
            handle.as_mut().set_round_keys([[u64::MAX; 16]; 3]);
            assert_eq!(handle.as_ref().triple_des(), flag);
            assert_eq!(handle.as_ref().round_keys(), [[u64::MAX; 16]; 3]);
        }
        handle.as_mut().init(&unaligned.0[1..25], false).unwrap();
        handle.as_mut().init(&unaligned.0[1..9], true).unwrap();
        assert_eq!(handle.as_ref().triple_des(), 0);
    }
    // The copied field snapshot owns its integers and survives its parent.
    let keys = heap.as_ref().round_keys();
    drop(heap);
    assert_ne!(keys[0], [0; 16]);
}

#[test]
fn repeated_construction_and_destruction() {
    for _ in 0..128 {
        let mut owner = Des::new_boxed(&[0; 24], false).unwrap();
        owner.as_mut().init(&[255; 8], true).unwrap();
        assert_eq!(owner.as_ref().triple_des(), 0);
    }
}

#[test]
fn rust_only_layout_projection() {
    // This safe public-API path deliberately makes no foreign call, so Miri
    // can validate layout projection and lifetimes as well as the sanitizers.
    let mut inline = Des::zeroed();
    assert_eq!(inline.as_ref().round_keys(), [[0; 16]; 3]);
    let mut exclusive = inline.as_mut();
    for flag in [i32::MIN, 0, i32::MAX] {
        exclusive.as_mut().set_triple_des(flag);
        exclusive.as_mut().set_round_keys([[flag as u64; 16]; 3]);
        let shared = exclusive.as_ref();
        let copied = shared;
        assert_eq!(copied.triple_des(), flag);
        assert_eq!(copied.round_keys(), [[flag as u64; 16]; 3]);
    }
}

#[test]
fn copies_and_clones_outlive_their_sources() {
    let mut clones = Vec::new();
    for len in [8, 24] {
        let mut source = Des::new_boxed(&vec![0x3C; len], false).unwrap();
        let first = source.clone();
        {
            let mut handle = source.as_mut();
            handle.as_mut().set_round_keys([[u64::MAX; 16]; 3]);
            handle.init(&[0x99; 8], true).unwrap();
        }
        let second = source.clone();
        drop(source); // Parent first: clones own separate av_memdup blocks.
        assert_ne!(first.as_ref().round_keys(), second.as_ref().round_keys());
        clones.push(first.clone());
        clones.push(first);
        clones.push(second);
    }
    let allocated = DesBox::new().unwrap();
    clones.push(allocated.clone());
    drop(allocated);
    for clone in &mut clones {
        let mut copy = clone.clone();
        copy.as_mut().set_triple_des(-1);
        clone.as_mut().init(&[0x01; 24], false).unwrap();
        assert_eq!(copy.as_ref().triple_des(), -1);
    }
    // Inline copies are independent values that need no teardown.
    let mut inline = Des::new(&[0x42; 24], false).unwrap();
    let copy = inline;
    inline.as_mut().set_triple_des(0);
    assert_eq!(copy.as_ref().triple_des(), 1);
}

#[test]
fn crypt_foreign_writes_and_borrowed_inputs() {
    let mut inline = Des::zeroed();
    inline.as_mut().set_round_keys([[u64::MAX; 16]; 3]);
    for flag in [i32::MIN, -1, 0, 1, i32::MAX] {
        inline.as_mut().set_triple_des(flag);
        let mut output = [0xFF; 8];
        inline
            .as_ref()
            .crypt_in_place(&mut output, None, false)
            .unwrap();
        inline
            .as_ref()
            .crypt_in_place(&mut output, None, true)
            .unwrap();
        let _ = inline.as_ref().mac(&output).unwrap();
    }
    for key_len in [8, 24] {
        let owner = Des::new_boxed(&vec![0x35; key_len], false).unwrap();
        let shared = owner.as_ref();
        let second = shared;
        for blocks in [0, 1, 2, 17] {
            let mut input = vec![0xAA; blocks * 8 + 1];
            let mut output = vec![0; blocks * 8 + 1];
            let mut iv = [0x25; 8];
            shared
                .crypt(&mut output[1..], &input[1..], Some(&mut iv), false)
                .unwrap();
            input.fill(0); // no retained foreign pointer across this Rust reborrow
            drop(input);
            second
                .crypt_in_place(&mut output[1..], Some(&mut iv), true)
                .unwrap();
            second
                .crypt_zeros(&mut output[1..], Some(&mut iv), false)
                .unwrap();
            let _ = shared.mac(&output[1..]).unwrap();
            output.fill(7); // foreign writes ended before the next Rust borrow
        }
    }
}

#[test]
fn independent_crypt_contexts_on_threads() {
    let workers: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let context = Des::new(&[3; 24], false).unwrap();
                let mut data = [7; 80];
                for _ in 0..16 {
                    context
                        .as_ref()
                        .crypt_in_place(&mut data, None, false)
                        .unwrap();
                    context
                        .as_ref()
                        .crypt_in_place(&mut data, None, true)
                        .unwrap();
                    let _ = context.as_ref().mac(&data).unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}
