//! Public safe layout, initialization, ownership, and reborrow tests.

use ffibox::CZeroable;
use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
use ffmpeg::libavutil::xtea::{Xtea, XteaBox, XteaMut};

const KEY: [u8; 16] = [
    0, 1, 2, 3, 4, 5, 6, 7, 128, 255, 254, 253, 252, 251, 250, 249,
];

fn replace(mut view: XteaMut<'_>) {
    view.set_key([u32::MAX; 16]);
}

#[test]
fn ffi_free_inline_shared_and_exclusive_reborrows() {
    let mut value = Xtea::zeroed();
    let a = value.as_ref();
    let b = a;
    assert_eq!(a.key(), b.key());
    assert_eq!(a.key(), [0; 16]);
    let mut exclusive = value.as_mut();
    replace(exclusive.as_mut());
    let snapshot = exclusive.as_ref().key();
    exclusive.set_key([0; 16]);
    assert_eq!(snapshot, [u32::MAX; 16]);
    assert_eq!(value.as_ref().key(), [0; 16]);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn initialization_and_foreign_writes_across_reborrows() {
    let mut value = Xtea::new(&KEY);
    let expected_be = [0x00010203, 0x04050607, 0x80fffefd, 0xfcfbfaf9];
    assert_eq!(value.as_ref().key()[..4], expected_be);
    assert_eq!(Xtea::new_le(&KEY).as_ref().key()[0], 0x03020100);
    value.as_mut().set_key([u32::MAX; 16]);
    {
        let mut exclusive = value.as_mut();
        exclusive.init(&KEY);
        let shared = exclusive.as_ref();
        assert_eq!(shared.key()[..4], expected_be);
        exclusive.as_mut().init_le(&KEY);
        assert_eq!(exclusive.as_ref().key()[0], 0x03020100);
    }
    assert_eq!(value.as_ref().key()[4..], [u32::MAX; 12]);
    // C must not retain the temporary key across a new exclusive reborrow.
    value.as_mut().init(&[0; 16]);
    assert_eq!(value.as_ref().key()[..4], [0; 4]);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_lifecycle_independent_clones_and_parent_drop() {
    let mut owner = XteaBox::new().unwrap();
    assert_eq!(owner.as_ref().key(), [0; 16]);
    owner.as_mut().init(&KEY);
    let copy = owner.clone();
    let another = copy.try_clone().unwrap();
    let snapshot = owner.as_ref().key();
    owner.as_mut().init_le(&KEY);
    drop(owner);
    assert_eq!(copy.as_ref().key(), snapshot);
    drop(copy);
    assert_eq!(another.as_ref().key(), snapshot);
    drop(HeapBox::<Xtea>::new_uninit().unwrap());
    let zero = HeapBox::<Xtea>::new_zeroed().unwrap();
    assert_eq!(zero.as_ref().key(), [0; 16]);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn arrays_and_borrowed_elements() {
    let mut array = alloc_cell_array_zeroed::<Xtea>(3).unwrap();
    {
        let mut views = array.as_handles_mut();
        assert!(views.get_mut(3).is_none());
        views.get_mut(0).unwrap().set_key([u32::MAX; 16]);
        views.get_mut(1).unwrap().init(&KEY);
        views.get_mut(2).unwrap().init_le(&KEY);
    }
    let views = array.as_handles();
    assert!(views.get(3).is_none());
    assert_eq!(views.get(0).unwrap().key(), [u32::MAX; 16]);
    assert_eq!(views.get(1).unwrap().key()[0], 0x00010203);
    assert_eq!(views.get(2).unwrap().key()[0], 0x03020100);
}

fn exercise_cipher(view: ffmpeg::libavutil::xtea::XteaRef<'_>) {
    use ffmpeg::libavutil::aes::Direction::{Decrypt, Encrypt};
    let snapshot = view.key();
    let shared = view;
    for le in [false, true] {
        for cbc in [false, true] {
            for blocks in [0, 1, 2, 33] {
                let len = blocks * 8;
                let input: Vec<u8> = (0..len + 2).map(|i| (i as u8).wrapping_mul(137)).collect();
                let mut output = vec![0xa5; len + 2];
                let initial_iv = [0xff; 8];
                let mut iv = initial_iv;
                let iv_arg = cbc.then_some(&mut iv);
                if le {
                    shared.crypt_le(&mut output[1..len + 1], &input[1..len + 1], iv_arg, Encrypt)
                } else {
                    shared.crypt(&mut output[1..len + 1], &input[1..len + 1], iv_arg, Encrypt)
                }
                .unwrap();
                assert_eq!(shared.key(), snapshot);
                let encrypted = output.clone();
                iv = initial_iv;
                let iv_arg = cbc.then_some(&mut iv);
                if le {
                    view.crypt_le_in_place(&mut output[1..len + 1], iv_arg, Decrypt)
                } else {
                    view.crypt_in_place(&mut output[1..len + 1], iv_arg, Decrypt)
                }
                .unwrap();
                assert_eq!(&output[1..len + 1], &input[1..len + 1]);
                assert_eq!(output[0], 0xa5);
                assert_eq!(output[len + 1], 0xa5);
                assert_eq!(
                    iv,
                    if cbc && len > 0 {
                        encrypted[len - 7..len + 1].try_into().unwrap()
                    } else {
                        initial_iv
                    }
                );
                // Reborrow the same Rust buffers after C returns; retained
                // foreign pointers must not exist across these writes.
                output.fill(0x80);
                iv.fill(0x40);
                let iv_arg = cbc.then_some(&mut iv);
                if le {
                    view.crypt_le_in_place(&mut output[1..len + 1], iv_arg, Encrypt)
                } else {
                    view.crypt_in_place(&mut output[1..len + 1], iv_arg, Encrypt)
                }
                .unwrap();
                assert_eq!(output[0], 0x80);
                assert_eq!(output[len + 1], 0x80);
            }
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn cipher_inline_shared_handles_and_foreign_buffer_reborrows() {
    let mut inline = Xtea::new(&KEY);
    exercise_cipher(inline.as_ref());
    inline.as_mut().init_le(&[0xff; 16]);
    exercise_cipher(inline.as_mut().as_ref());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn cipher_heap_array_and_clone_keepalive() {
    let mut owner = XteaBox::new().unwrap();
    owner.as_mut().init(&KEY);
    exercise_cipher(owner.as_ref());
    let clone = owner.clone();
    drop(owner);
    exercise_cipher(clone.as_ref());
    let mut array = alloc_cell_array_zeroed::<Xtea>(2).unwrap();
    array.as_handles_mut().get_mut(1).unwrap().init_le(&KEY);
    exercise_cipher(array.as_handles().get(0).unwrap());
    exercise_cipher(array.as_handles_mut().get_mut(1).unwrap().as_ref());
}

#[test]
fn cipher_compile_fail() {
    trybuild::TestCases::new().compile_fail("tests/ub_safe/compile_fail/xtea_cipher_*.rs");
}
