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
