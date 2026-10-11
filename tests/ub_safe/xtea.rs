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
