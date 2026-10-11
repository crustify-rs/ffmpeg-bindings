use ffmpeg::libavutil::aes_ctr::{AesCtr, AesCtrRef};

fn shared_views(view: AesCtrRef<'_>) {
    let _copy = view;
    let _second = view;
}

#[test]
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
