use ffibox::CZeroable;
use ffmpeg::libavutil::rc4::{Rc4, Rc4Box};

#[test]
fn inline_and_heap_reborrows() {
    let mut inline = Rc4::from_key(&[0]).unwrap();
    let mut boxed = Rc4::new(b"a nonempty key").unwrap();
    for mut handle in [inline.as_mut(), boxed.as_mut()] {
        handle.set_x(i32::MIN);
        handle.set_y(i32::MAX);
        {
            let mut inner = handle.as_mut();
            let mut bytes = inner.state_mut();
            assert_eq!(bytes.len(), 256);
            assert!(bytes.set_elem(0, 0));
            assert!(bytes.set_elem(255, 255));
            assert!(!bytes.set_elem(256, 1));
            assert!(!bytes.copy_from_slice(&[0; 255]));
            let mut nested = bytes.as_mut();
            assert!(nested.set_elem(128, 7));
            assert_eq!(nested.as_ref().elem(128), Some(7));
        }
        let shared = handle.as_ref();
        let copy = shared;
        assert_eq!(copy.x(), i32::MIN);
        assert_eq!(copy.y(), i32::MAX);
        assert_eq!(copy.state().elem(255), Some(255));
        assert_eq!(copy.state().elem(256), None);
        handle.init(b"reset").unwrap();
        assert_eq!(handle.as_ref().x(), 1);
    }
}

#[test]
fn every_rc4_owner_and_copy_is_independent() {
    for _ in 0..32 {
        let mut original = Rc4::new(&[255]).unwrap();
        let mut copy = original.clone();
        let fallible_copy = original.try_clone().unwrap();
        original.as_mut().set_state(&[9; 256]);
        copy.as_mut().set_x(42);
        drop(original);
        assert_eq!(copy.as_ref().x(), 42);
        assert_eq!(fallible_copy.as_ref().x(), 1);
        drop(copy);
        drop(fallible_copy);
    }
    let mut zeroed = Rc4Box::new().unwrap();
    assert!(zeroed.as_ref().state().elems().all(|x| x == 0));
    zeroed.as_mut().init(b"key").unwrap();
    let mut generic = Rc4Box::new_zeroed().unwrap();
    assert_eq!(generic.as_ref().x(), 0);
    generic.as_mut().init(b"key").unwrap();
    let inline = Rc4::zeroed();
    assert_eq!(inline.as_ref().y(), 0);
}

#[test]
fn allocation_failure_is_contained_in_a_child_process() {
    const CHILD: &str = "CRUSTIFY_CRYPTO_BATCH6_ALLOC_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // Only this exact test runs in the child, so this process-global
        // allocation limit cannot interfere with other tests or threads.
        let held = Rc4::new(b"key").unwrap();
        ffmpeg::libavutil::mem::set_max_alloc(1);
        assert!(held.try_clone().is_none());
        assert_eq!(held.as_ref().x(), 1);
        assert!(Rc4Box::new().is_none());
        assert!(matches!(Rc4::new(b"key"), Err(-12)));
        use ffmpeg::libavutil::ripemd::{DigestBits, Ripemd};
        assert!(Ripemd::new(DigestBits::Bits160).is_none());
        ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);
        return;
    }
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "rc4::allocation_failure_is_contained_in_a_child_process",
        ])
        .env(CHILD, "1")
        .status()
        .unwrap();
    assert!(result.success());
}

#[test]
fn zeroed_inline_field_projection_without_ffi() {
    let mut context = Rc4::zeroed();
    assert_eq!(context.as_ref().x(), 0);
    assert_eq!(context.as_ref().y(), 0);
    {
        let mut handle = context.as_mut();
        handle.set_x(i32::MAX);
        handle.set_y(i32::MIN);
        let mut bytes = handle.state_mut();
        assert!(bytes.copy_from_slice(&[255; 256]));
        assert!(bytes.set_elem(0, 7));
        assert!(!bytes.set_elem(256, 7));
    }
    let handle = context.as_ref();
    assert_eq!(handle.x(), i32::MAX);
    assert_eq!(handle.y(), i32::MIN);
    let bytes = handle.state();
    let (left, right) = bytes.split_at(128).unwrap();
    assert_eq!(left.elem(0), Some(7));
    assert_eq!(right.elem(127), Some(255));
    assert!(bytes.slice(256..257).is_none());
    let mut output = [0; 256];
    assert!(bytes.copy_to_slice(&mut output));
    assert_eq!(output[255], 255);
}

#[test]
fn independent_thread_local_contexts_do_not_share_state() {
    use ffmpeg::libavutil::ripemd::{DigestBits, Ripemd};
    let threads: Vec<_> = (0..8u8)
        .map(|key| {
            std::thread::spawn(move || {
                for _ in 0..32 {
                    // Owners are constructed and released on this thread;
                    // none of the !Send/!Sync C handles crosses a thread.
                    let mut cipher = Rc4::new(&[key]).unwrap();
                    let inline = Rc4::from_key(&[key]).unwrap();
                    assert_eq!(cipher.as_ref().y(), inline.as_ref().y());
                    cipher.as_mut().set_x(i32::from(key));
                    assert_eq!(cipher.as_ref().x(), i32::from(key));
                    let mut hash = Ripemd::new(DigestBits::Bits320).unwrap();
                    hash.as_mut().init(DigestBits::Bits128).unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
pub(super) fn batch2_foreign_writes_across_reborrows() {
    let mut inline = Rc4::from_key(b"key").unwrap();
    let mut heap = Rc4::new(b"key").unwrap();
    for mut parent in [inline.as_mut(), heap.as_mut()] {
        for len in [0, 1, 255, 256, 513] {
            let mut data = vec![0x6a; len + 1];
            let input = data.clone();
            parent.as_mut().init(b"key").unwrap();
            parent.as_mut().crypt_in_place(&mut data[1..]).unwrap();
            parent.as_mut().init(b"key").unwrap();
            parent.as_mut().crypt_in_place(&mut data[1..]).unwrap();
            assert_eq!(data, input);
            parent.as_mut().keystream(&mut data[1..]).unwrap();
            let mut output = vec![0; len + 1];
            parent.as_mut().crypt(&mut output[1..], &data[1..]).unwrap();
            data.fill(0);
            parent.as_mut().crypt_in_place(&mut output[1..]).unwrap();
        }
    }
}

#[test]
fn batch2_independent_thread_local_crypto_operations() {
    let threads: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                batch2_foreign_writes_across_reborrows();
                super::ripemd::batch2_foreign_writes_across_reborrows();
                super::sha::batch2_foreign_writes_across_reborrows();
                super::sha512::batch2_foreign_writes_across_reborrows();
                super::tea::batch2_foreign_writes_across_reborrows();
                super::twofish::batch2_foreign_writes_across_reborrows();
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}
