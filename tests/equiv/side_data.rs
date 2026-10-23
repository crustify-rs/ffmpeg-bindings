//! Equivalence of `libavutil/side_data.c`'s side data arrays with C's: the
//! array routines, the entries they hold, and the descriptor and name lookups.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#![expect(
    clippy::too_many_lines,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]

/// `SideDataType::descriptor` and `SideDataType::name` against
/// `av_frame_side_data_desc` and `av_frame_side_data_name` over every value
/// up to past the table's end and at the edges of `unsigned`: the same
/// entries, the same name pointers, the same properties (buffer-frame wave 1,
/// batch 1).
mod side_data_lookups {
    use core::ffi::CStr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::SideDataType;

    /// Every value through one past the largest named type, plus the edges.
    fn values() -> impl Iterator<Item = u32> {
        (0..=64).chain([
            1000,
            i32::MAX as u32,
            i32::MAX as u32 + 1,
            u32::MAX - 1,
            u32::MAX,
        ])
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn descriptors_are_cs_table_entries() {
        let mut named = 0;
        for raw in values() {
            let t = ffi::AVFrameSideDataType(raw);
            // SAFETY: C takes any value and returns NULL or an entry of its
            // static table.
            let c = unsafe { ffi::av_frame_side_data_desc(t) };
            let rust = SideDataType::from_raw(t).descriptor();
            assert_eq!(rust.is_some(), !c.is_null(), "{raw}");
            let Some(d) = rust else { continue };
            named += 1;
            // SAFETY: a non-NULL entry of the static table, whose name is a
            // non-NULL string literal for every entry C returns.
            let (c_name, c_props) = (unsafe { (*c).name }, unsafe { (*c).props });
            let name = d.name().expect("every returned entry is named");
            assert_eq!(name.as_ptr(), c_name, "{raw}: the same literal");
            assert_eq!(d.props().bits(), c_props, "{raw}");
            let copy = d.get();
            assert_eq!(copy.as_ref().name(), Some(name));
            assert_eq!(copy.into_raw().props, c_props);
        }
        assert!(named >= 38, "{named} named entries");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn names_are_cs_strings() {
        for raw in values() {
            let t = ffi::AVFrameSideDataType(raw);
            // SAFETY: C takes any value and returns NULL or a literal.
            let c = unsafe { ffi::av_frame_side_data_name(t) };
            let rust = SideDataType::from_raw(t).name();
            assert_eq!(rust.map_or(core::ptr::null(), CStr::as_ptr), c, "{raw}");
            let desc_name = SideDataType::from_raw(t)
                .descriptor()
                .and_then(|d| d.name());
            assert_eq!(rust, desc_name, "{raw}");
        }
    }
}

/// [`SideData`] and [`SideDataArray`] against C's side data arrays, as C
/// observes them (buffer-frame wave 2, batch 0): a Rust array and a C array
/// driven through the same random sequences of additions and field writes,
/// compared after every step by C's own reading of both (the entries' fields,
/// payload bytes, metadata, buffer counts and writability), by C's
/// `av_frame_side_data_get_c` and `av_frame_side_data_clone` over the Rust
/// array, and by the buffers and dictionaries each side gave back, down to
/// what the arrays' release leaves of them.
///
/// [`SideData`]: ffmpeg_bindings::libavutil::libavutil::frame::SideData
/// [`SideDataArray`]: ffmpeg_bindings::libavutil::libavutil::side_data::SideDataArray
mod side_data {
    use core::ffi::{CStr, c_int};
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::buffer::{BufferReference, BufferReferenceBox};
    use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{SideDataRef, SideDataType};
    use ffmpeg_bindings::libavutil::libavutil::side_data::{SideDataArray, SideDataFlags};

    use super::side_data_fixture::{SAMPLE, Spec};

    /// What C sees of one entry.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct Seen {
        pub(super) kind: u32,
        pub(super) bytes: Vec<u8>,
        pub(super) metadata: Pairs,
        count: c_int,
        writable: bool,
        window: bool,
    }

    /// C's reading of a dictionary, in iteration order.
    pub(super) fn c_pairs(m: *const ffi::AVDictionary) -> Pairs {
        let mut out = Vec::new();
        let mut e = ptr::null();
        loop {
            // SAFETY: `m` is NULL or a live dictionary; `e` is NULL or its
            // previous entry.
            e = unsafe { ffi::av_dict_iterate(m, e) };
            if e.is_null() {
                return out;
            }
            // SAFETY: as the comment below states, for this part of the operation.
            let key_2 = unsafe { (*e).key };
            // SAFETY: as the comment below states, for this part of the operation.
            let from_ptr_2 = unsafe { CStr::from_ptr(key_2) };
            // SAFETY: as the comment below states, for this part of the operation.
            let value_2 = unsafe { (*e).value };
            // SAFETY: a live entry, whose key and value are C strings.
            let (k, v) = unsafe { (from_ptr_2, CStr::from_ptr(value_2)) };
            out.push((k.to_bytes().into(), v.to_bytes().into()));
        }
    }

    /// C's reading of every entry of the array `sd`/`nb`.
    ///
    /// # Safety
    ///
    /// `sd`/`nb` must be a live array of live entries.
    unsafe fn c_seen(sd: *const *mut ffi::AVFrameSideData, nb: c_int) -> Vec<Seen> {
        (0..nb as usize)
            .map(|i| {
                // For the unsafe operations below: entry `i` of the caller's array, and its buffer.
                {
                    // SAFETY: as the comment below states, for this part of the operation.
                    let sd_add_3 = unsafe { sd.add(i) };
                    // SAFETY: as the comment above this block states.
                    let e = unsafe { *sd_add_3 };
                    // SAFETY: as the comment above this block states.
                    let (buf, data, size) = (unsafe { (*e).buf }, unsafe { (*e).data }, unsafe {
                        (*e).size
                    });
                    // SAFETY: as the comment above this block states.
                    Seen {
                        // SAFETY: as the comment above this statement states.
                        kind: unsafe { (*e).type_ }.0,
                        bytes: if size == 0 {
                            Vec::new()
                        } else {
                            // SAFETY: as the comment above this statement states.
                            unsafe { std::slice::from_raw_parts(data, size) }.to_vec()
                        },
                        // SAFETY: as the comment above this statement states.
                        metadata: c_pairs(unsafe { (*e).metadata }),
                        // SAFETY: as the comment above this statement states.
                        count: unsafe { ffi::av_buffer_get_ref_count(buf) },
                        // SAFETY: as the comment above this statement states.
                        writable: unsafe { ffi::av_buffer_is_writable(buf) } != 0,
                        // SAFETY: as the comment above this statement states.
                        window: data == unsafe { (*buf).data } && size == unsafe { (*buf).size },
                    }
                }
            })
            .collect()
    }

    /// C's reading of the Rust array, which it lends C for the call.
    pub(super) fn rust_seen(a: &mut SideDataArray) -> Vec<Seen> {
        let (sd, nb) = core::mem::take(a).into_raw_parts();
        // SAFETY: the array Rust held, live until it is taken back below.
        let seen = unsafe { c_seen(sd, nb) };
        // SAFETY: the same array, unchanged, given back.
        *a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
        seen
    }

    /// Key/value pairs as bytes, in iteration order.
    pub(super) type Pairs = Vec<(Vec<u8>, Vec<u8>)>;

    /// The Rust getters' reading of an entry, in [`Seen`]'s terms where the
    /// safe API has them.
    pub(super) fn getters(e: SideDataRef<'_>) -> (u32, Vec<u8>, Pairs) {
        let metadata = e.metadata().map_or_else(Vec::new, |m| {
            m.iter()
                .map(|p| (p.key().to_bytes().into(), p.value().to_bytes().into()))
                .collect()
        });
        (e.kind().into_raw().0, e.data().to_vec(), metadata)
    }

    /// An `AVFrameSideData *` array C grows and writes, freed on drop.
    pub(super) struct CArray {
        pub(super) sd: *mut *mut ffi::AVFrameSideData,
        pub(super) nb: c_int,
    }

    impl CArray {
        pub(super) fn new() -> Self {
            Self {
                sd: ptr::null_mut(),
                nb: 0,
            }
        }

        pub(super) fn entry(&self, i: usize) -> *mut ffi::AVFrameSideData {
            assert!(i < self.nb as usize);
            // SAFETY: as the comment below states, for this part of the operation.
            let sd_add_2 = unsafe { self.sd.add(i) };
            // SAFETY: `i < nb`.
            unsafe { *sd_add_2 }
        }

        pub(super) fn seen(&self) -> Vec<Seen> {
            // SAFETY: the array C built.
            unsafe { c_seen(self.sd, self.nb) }
        }

        /// `av_frame_side_data_add` of `buf`, which C takes on success;
        /// whether it added, and the reference it left the caller.
        pub(super) fn add(
            &mut self,
            kind: u32,
            buf: *mut ffi::AVBufferRef,
            flags: SideDataFlags,
        ) -> (bool, *mut ffi::AVBufferRef) {
            let mut slot = buf;
            // SAFETY: the array C built and a live reference.
            let e = unsafe {
                ffi::av_frame_side_data_add(
                    &raw mut self.sd,
                    &raw mut self.nb,
                    ffi::AVFrameSideDataType(kind),
                    &raw mut slot,
                    flags.bits(),
                )
            };
            (!e.is_null(), slot)
        }

        /// The field writes C callers make to replace entry `i`'s buffer,
        /// as `replace_side_data_from_buf` does without its metadata free;
        /// the old reference is the caller's.
        fn replace_buffer(
            &mut self,
            i: usize,
            buf: *mut ffi::AVBufferRef,
        ) -> *mut ffi::AVBufferRef {
            let e = self.entry(i);
            // SAFETY: `e` is a live entry: a read of its reference.
            let old = unsafe { (*e).buf };
            // SAFETY: `buf` is a live reference: reads of its window.
            let (data, size) = (unsafe { (*buf).data }, unsafe { (*buf).size });
            // SAFETY: as above; the entry takes over `buf`.
            unsafe { (*e).buf = buf };
            // SAFETY: as above; the entry's window is the new reference's.
            unsafe { (*e).data = data };
            // SAFETY: as above.
            unsafe { (*e).size = size };
            old
        }

        pub(super) fn replace_metadata(
            &mut self,
            i: usize,
            m: *mut ffi::AVDictionary,
        ) -> *mut ffi::AVDictionary {
            let e = self.entry(i);
            // SAFETY: a live entry, which takes over `m`.
            unsafe { core::mem::replace(&mut (*e).metadata, m) }
        }

        fn set_kind(&mut self, i: usize, kind: u32) {
            // SAFETY: a live entry.
            unsafe { (*self.entry(i)).type_ = ffi::AVFrameSideDataType(kind) }
        }

        /// Writes `byte` over entry `i`'s payload when its buffer is
        /// writable; whether it was.
        fn write(&mut self, i: usize, byte: u8) -> bool {
            let e = self.entry(i);
            // SAFETY: `e` is a live entry: a read of its reference.
            let buf = unsafe { (*e).buf };
            // SAFETY: `buf` is the entry's live reference.
            let ok = unsafe { ffi::av_buffer_is_writable(buf) } != 0;
            if ok {
                // SAFETY: as above: reads of the entry's window.
                let (data, size) = (unsafe { (*e).data }, unsafe { (*e).size });
                // SAFETY: the window's bytes are writable: the buffer is sole and not read-only.
                unsafe { ptr::write_bytes(data, byte, size) };
            }
            ok
        }
    }

    impl Drop for CArray {
        fn drop(&mut self) {
            // SAFETY: frees the array and every entry, as C built them.
            unsafe { ffi::av_frame_side_data_free(&raw mut self.sd, &raw mut self.nb) }
        }
    }

    /// A C reference holding `bytes`.
    pub(super) fn c_buffer(bytes: &[u8]) -> *mut ffi::AVBufferRef {
        // For the unsafe operations below: a fresh reference to `len` bytes, filled from `bytes`.
        {
            // SAFETY: as the comment above this block states.
            let b = unsafe { ffi::av_buffer_alloc(bytes.len()) };
            assert!(!b.is_null());
            // SAFETY: as the comment below states, for this part of the operation.
            let data_4 = unsafe { (*b).data };
            // SAFETY: as the comment above this block states.
            unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data_4, bytes.len()) };
            b
        }
    }

    /// A C dictionary of `pairs`, inserted as `Dict::from_entries` does.
    pub(super) fn c_dict(pairs: &[(&CStr, &CStr)]) -> *mut ffi::AVDictionary {
        let mut m = ptr::null_mut();
        for (k, v) in pairs {
            // SAFETY: C copies the strings into `m`.
            let r = unsafe {
                ffi::av_dict_set(
                    &raw mut m,
                    k.as_ptr(),
                    v.as_ptr(),
                    ffi::AV_DICT_MULTIKEY as c_int,
                )
            };
            assert!(r >= 0);
        }
        m
    }

    /// C's reading of a reference the caller holds: bytes and count.
    fn c_buf_seen(b: *const ffi::AVBufferRef) -> (Vec<u8>, c_int) {
        // SAFETY: `b` is a live reference: reads of its window.
        let (data, size) = (unsafe { (*b).data }, unsafe { (*b).size });
        let bytes = if size == 0 {
            Vec::new()
        } else {
            // SAFETY: as above; `size` initialised bytes at `data`, read while `b` lives.
            unsafe { std::slice::from_raw_parts(data, size) }.to_vec()
        };
        // SAFETY: as above.
        (bytes, unsafe { ffi::av_buffer_get_ref_count(b) })
    }

    fn rust_buf_seen(b: &BufferReferenceBox) -> (Vec<u8>, c_int) {
        c_buf_seen(b.as_c_ptr())
    }

    /// Asserts both arrays read alike, through C and through the getters,
    /// and that C's lookups and clones of the Rust array match its own.
    pub(super) fn compare(r: &mut SideDataArray, c: &CArray, step: &str) {
        let (rs, cs) = (rust_seen(r), c.seen());
        assert_eq!(rs, cs, "{step}");
        assert_eq!(r.len(), cs.len(), "{step}");
        for (e, s) in r.as_ref().iter().zip(&cs) {
            assert_eq!(
                getters(e),
                (s.kind, s.bytes.clone(), s.metadata.clone()),
                "{step}"
            );
        }
        let (sd, nb) = core::mem::take(r).into_raw_parts();
        for kind in KINDS {
            // SAFETY: the two live arrays, read for the call.
            let (rf, cf) = (
                unsafe {
                    ffi::av_frame_side_data_get_c(sd.cast(), nb, ffi::AVFrameSideDataType(kind))
                },
                unsafe {
                    ffi::av_frame_side_data_get_c(c.sd.cast(), c.nb, ffi::AVFrameSideDataType(kind))
                },
            );
            let index =
                |p: *const ffi::AVFrameSideData, sd: *mut *mut ffi::AVFrameSideData, nb: c_int| {
                    (0..nb as usize).position(|i| {
                        // SAFETY: `i < nb` entries of a live array.
                        let at = unsafe { sd.add(i) };
                        // SAFETY: as above; the entry is initialised.
                        ptr::eq(unsafe { *at }, p)
                    })
                };
            assert_eq!(
                index(rf, sd, nb),
                index(cf, c.sd, c.nb),
                "{step}: get_c({kind})"
            );
        }
        let (mut rc, mut cc) = (CArray::new(), CArray::new());
        for i in 0..nb as usize {
            // SAFETY: as the comment below states, for this part of the operation.
            let sd_add = unsafe { sd.add(i) };
            // SAFETY: entry `i < nb` of C's live array.
            let c_add = unsafe { c.sd.add(i) };
            // SAFETY: entry `i` of each live array, initialised.
            let (rs, cs) = (unsafe { *sd_add }, unsafe { *c_add });
            // SAFETY: each entry is live, cloned by C into a fresh array; a non-MULTI
            // duplicate is refused alike.
            let (x, y) = (
                unsafe { ffi::av_frame_side_data_clone(&raw mut rc.sd, &raw mut rc.nb, rs, 0) },
                unsafe { ffi::av_frame_side_data_clone(&raw mut cc.sd, &raw mut cc.nb, cs, 0) },
            );
            assert_eq!(x, y, "{step}: clone {i}");
        }
        assert_eq!(rc.seen(), cc.seen(), "{step}: clones");
        drop((rc, cc));
        // SAFETY: the Rust array, unchanged, given back.
        *r = unsafe { SideDataArray::from_raw_parts(sd, nb) };
    }

    /// The kinds the sequences use: `MULTI`, two unique named ones, and one
    /// C does not name.
    pub(super) const KINDS: [u32; 4] = [
        SideDataType::SEI_UNREGISTERED.into_raw().0,
        SideDataType::AFD.into_raw().0,
        SideDataType::ICC_PROFILE.into_raw().0,
        1000,
    ];

    pub(super) fn xorshift(x: &mut u64) -> u64 {
        *x ^= *x << 13;
        *x ^= *x >> 7;
        *x ^= *x << 17;
        *x
    }

    pub(super) const FLAGS: [SideDataFlags; 4] = [
        SideDataFlags::NONE,
        SideDataFlags::UNIQUE,
        SideDataFlags::REPLACE,
        SideDataFlags::REPLACE,
    ];

    pub(super) const PAIRS: [&[(&CStr, &CStr)]; 4] = [
        &[],
        &[(c"k", c"v")],
        &[(c"a", c"1"), (c"b", c"2")],
        &[(c"dup", c"1"), (c"dup", c"2"), (c"z", c"")],
    ];

    /// Random sequences of every operation, compared after each step.
    fn run(seed: u64, steps: usize) {
        let mut x = seed;
        let (mut r, mut c) = (SideDataArray::new(), CArray::new());
        // Buffers each side kept: given back by an addition or a
        // replacement, or a second reference taken to share an entry's.
        let (mut r_kept, mut c_kept): (Vec<BufferReferenceBox>, Vec<*mut ffi::AVBufferRef>) =
            (Vec::new(), Vec::new());
        let mut r_dicts: Vec<Dict> = Vec::new();
        let mut c_dicts: Vec<*mut ffi::AVDictionary> = Vec::new();
        for step in 0..steps {
            let v = xorshift(&mut x);
            let len = c.nb as usize;
            let i = (v >> 8) as usize % len.max(1);
            let payload: Vec<u8> = (0..(v >> 20) % 70).map(|k| (k as u8) ^ (v as u8)).collect();
            let what = if len == 0 { 0 } else { v % 8 };
            let label = format!("seed {seed} step {step} op {what}");
            match what {
                0 => {
                    let kind = KINDS[(v >> 4) as usize % KINDS.len()];
                    let flags = FLAGS[(v >> 6) as usize % FLAGS.len()];
                    let rb = BufferReference::alloc(payload.len())
                        .unwrap()
                        .copy_from_slice(&payload);
                    let kind_r = SideDataType::from_raw(ffi::AVFrameSideDataType(kind));
                    let ra = r.add(kind_r, rb, flags).map(drop);
                    let (ca, cb) = c.add(kind, c_buffer(&payload), flags);
                    assert_eq!(ra.is_ok(), ca, "{label}");
                    match ra {
                        Ok(()) => assert!(cb.is_null()),
                        Err(back) => {
                            assert_eq!(rust_buf_seen(&back), c_buf_seen(cb), "{label}");
                            r_kept.push(back);
                            c_kept.push(cb);
                        }
                    }
                }
                1 => {
                    let kind = KINDS[(v >> 4) as usize % KINDS.len()] ^ ((v >> 40) as u32 & 1);
                    r.as_mut()
                        .get_mut(i)
                        .unwrap()
                        .set_kind(SideDataType::from_raw(ffi::AVFrameSideDataType(kind)));
                    c.set_kind(i, kind);
                }
                2 => {
                    let rb = BufferReference::alloc(payload.len())
                        .unwrap()
                        .copy_from_slice(&payload);
                    let old = r.as_mut().get_mut(i).unwrap().replace_buffer(rb);
                    let cold = c.replace_buffer(i, c_buffer(&payload));
                    assert_eq!(rust_buf_seen(&old), c_buf_seen(cold), "{label}");
                    r_kept.push(old);
                    c_kept.push(cold);
                }
                3 => {
                    let rb = BufferReference::alloc(payload.len())
                        .unwrap()
                        .copy_from_slice(&payload);
                    r.as_mut().get_mut(i).unwrap().set_buffer(rb);
                    let cold = c.replace_buffer(i, c_buffer(&payload));
                    // SAFETY: the old reference, which the caller now holds.
                    unsafe { ffi::av_buffer_unref(&mut { cold }) };
                }
                4 => {
                    let pairs = PAIRS[(v >> 4) as usize % PAIRS.len()];
                    let old = r
                        .as_mut()
                        .get_mut(i)
                        .unwrap()
                        .replace_metadata(Dict::from_entries(pairs.iter().copied()).unwrap());
                    let cold = c.replace_metadata(i, c_dict(pairs));
                    let old_pairs = old
                        .iter()
                        .map(|p| (p.key().to_bytes().to_vec(), p.value().to_bytes().to_vec()))
                        .collect::<Vec<_>>();
                    assert_eq!(old_pairs, c_pairs(cold), "{label}");
                    r_dicts.push(old);
                    c_dicts.push(cold);
                }
                5 => {
                    let taken = r.as_mut().get_mut(i).unwrap().take_metadata();
                    let cold = c.replace_metadata(i, ptr::null_mut());
                    assert_eq!(taken.len(), c_pairs(cold).len(), "{label}");
                    r_dicts.push(taken);
                    c_dicts.push(cold);
                }
                6 => {
                    let byte = (v >> 30) as u8;
                    let mut entries = r.as_mut();
                    let mut e = entries.get_mut(i).unwrap();
                    let wrote = match e.data_mut() {
                        Some(bytes) => {
                            bytes.fill(byte);
                            true
                        }
                        None => false,
                    };
                    assert_eq!(wrote, c.write(i, byte), "{label}");
                }
                _ => {
                    let copy = r.as_ref().get(i).unwrap().buffer().try_clone().unwrap();
                    // SAFETY: a read of a live entry's reference.
                    let cbuf = unsafe { (*c.entry(i)).buf };
                    // SAFETY: a live reference, to which C adds one.
                    let ccopy = unsafe { ffi::av_buffer_ref(cbuf) };
                    assert_eq!(rust_buf_seen(&copy), c_buf_seen(ccopy), "{label}");
                    r_kept.push(copy);
                    c_kept.push(ccopy);
                }
            }
            compare(&mut r, &c, &label);
            if step % 16 == 15 && !r_kept.is_empty() {
                // Release the oldest kept reference on both sides, which may
                // make an entry writable again.
                drop(r_kept.remove(0));
                let mut b = c_kept.remove(0);
                // SAFETY: the kept reference, released once.
                unsafe { ffi::av_buffer_unref(&raw mut b) };
                compare(&mut r, &c, &format!("{label} release"));
            }
        }
        // What the arrays' release leaves of the references both sides kept.
        drop(r);
        drop(c);
        for (rb, cb) in r_kept.iter().zip(&c_kept) {
            assert_eq!(
                rust_buf_seen(rb),
                c_buf_seen(*cb),
                "seed {seed}: kept after release"
            );
        }
        for mut b in c_kept {
            // SAFETY: each kept reference, released once.
            unsafe { ffi::av_buffer_unref(&raw mut b) };
        }
        for mut m in c_dicts {
            // SAFETY: each kept dictionary, freed once.
            unsafe { ffi::av_dict_free(&raw mut m) };
        }
        drop(r_dicts);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn random_sequences_match_c_after_every_step() {
        for seed in 1..=40u64 {
            run(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15), 120);
        }
    }

    /// The sample entries built through the safe API read, look up and clone
    /// as the same entries C builds and fills.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn safely_built_entries_read_as_c_built_ones() {
        let mut r = super::side_data_fixture::sample();
        let mut c = CArray::new();
        for spec in &SAMPLE {
            let (ok, slot) = c.add(
                spec.kind.into_raw().0,
                c_buffer(spec.payload),
                SideDataFlags::NONE,
            );
            assert!(ok && slot.is_null());
            let n = c.nb as usize - 1;
            let _ = c.replace_metadata(n, c_dict(spec.metadata));
        }
        compare(&mut r, &c, "sample");
        let specs: Vec<Spec<'_>> = SAMPLE.iter().rev().copied().collect();
        let mut r = super::side_data_fixture::array(&specs);
        let mut c = CArray::new();
        for spec in &specs {
            let (ok, _) = c.add(
                spec.kind.into_raw().0,
                c_buffer(spec.payload),
                SideDataFlags::NONE,
            );
            assert!(ok);
            let n = c.nb as usize - 1;
            let _ = c.replace_metadata(n, c_dict(spec.metadata));
        }
        compare(&mut r, &c, "reversed sample");
    }

    /// C's own adders, given the Rust array, change it as they change C's,
    /// and its release frees what they left.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn c_adders_on_the_rust_array_match_c_arrays() {
        let mut r = super::side_data_fixture::sample();
        let mut c = CArray::new();
        for spec in &SAMPLE {
            c.add(
                spec.kind.into_raw().0,
                c_buffer(spec.payload),
                SideDataFlags::NONE,
            );
            let n = c.nb as usize - 1;
            let _ = c.replace_metadata(n, c_dict(spec.metadata));
        }
        for (k, flags) in [(0u32, 0), (8, 2), (8, 1), (13, 3), (36, 0), (1000, 2)] {
            let (mut sd, mut nb) = core::mem::take(&mut r).into_raw_parts();
            // SAFETY: the Rust array, lent to C for the call, and a fresh
            // C buffer of 5 bytes C fills nothing in.
            let e = unsafe {
                ffi::av_frame_side_data_new(
                    &raw mut sd,
                    &raw mut nb,
                    ffi::AVFrameSideDataType(k),
                    5,
                    flags,
                )
            };
            if !e.is_null() {
                // SAFETY: as the comment below states, for this part of the operation.
                let data_3 = unsafe { (*e).data };
                // SAFETY: the entry C returned, whose fresh payload is written
                // in full before the array goes back to Rust.
                unsafe { ptr::write_bytes(data_3, k as u8, 5) };
            }
            // SAFETY: the array C changed, every payload initialised.
            r = unsafe { SideDataArray::from_raw_parts(sd, nb) };
            // SAFETY: as above, on C's own array.
            let ce = unsafe {
                ffi::av_frame_side_data_new(
                    &raw mut c.sd,
                    &raw mut c.nb,
                    ffi::AVFrameSideDataType(k),
                    5,
                    flags,
                )
            };
            assert_eq!(e.is_null(), ce.is_null(), "new({k}, {flags})");
            if !ce.is_null() {
                // SAFETY: as the comment below states, for this part of the operation.
                let data_2 = unsafe { (*ce).data };
                // SAFETY: as above.
                unsafe { ptr::write_bytes(data_2, k as u8, 5) };
            }
            compare(&mut r, &c, &format!("new({k}, {flags})"));
        }
        // C's clones of a third array's entries, which write every field of
        // an entry they replace, into both.
        let mut src = CArray::new();
        for (k, pairs) in [
            (8u32, PAIRS[1]),
            (13, PAIRS[2]),
            (20, PAIRS[3]),
            (1000, PAIRS[0]),
        ] {
            let (ok, _) = src.add(k, c_buffer(&[k as u8; 9]), SideDataFlags::NONE);
            assert!(ok);
            let n = src.nb as usize - 1;
            let _ = src.replace_metadata(n, c_dict(pairs));
        }
        for flags in [0, 2, 1, 3, 2] {
            for i in 0..src.nb as usize {
                let (mut sd, mut nb) = core::mem::take(&mut r).into_raw_parts();
                let entry = src.entry(i);
                // SAFETY: the Rust array, lent to C for the call, and a live
                // source entry, whose buffer C shares and metadata it copies.
                let x = unsafe {
                    ffi::av_frame_side_data_clone(&raw mut sd, &raw mut nb, entry, flags)
                };
                // SAFETY: the array C changed, its entries' fields copied
                // from initialised entries.
                r = unsafe { SideDataArray::from_raw_parts(sd, nb) };
                // SAFETY: as above, on C's own array.
                let y = unsafe {
                    ffi::av_frame_side_data_clone(&raw mut c.sd, &raw mut c.nb, src.entry(i), flags)
                };
                assert_eq!(x, y, "clone({i}, {flags})");
                compare(&mut r, &c, &format!("clone({i}, {flags})"));
            }
        }
        drop(src);
        compare(&mut r, &c, "source freed");
    }
}

/// Side data arrays built through the safe API for the `side_data` tests.
#[path = "../common/side_data.rs"]
mod side_data_fixture;

/// The side data array routines against C's (buffer-frame wave 3, batch 1):
/// `SideDataArrayRef::find` (`av_frame_side_data_get_c`),
/// `SideDataArrayMut::find_mut` (`av_frame_side_data_get`), `remove`,
/// `remove_by_props`, `add_clone` (`av_frame_side_data_clone`) and
/// `alloc_zeroed` / `alloc_from_slice` (`av_frame_side_data_new`, then a
/// fill), with the owner's twins, on a Rust array and a C array driven
/// through the same random sequences, compared after every step by the
/// `side_data` suite's `compare`: C's reading of both arrays, their getters,
/// C's lookups and clones of them. Clones come from a Rust and a C source
/// array, compared too, and replaced halfway so that clones outlive them.
mod side_data_array_routines {
    use core::ffi::c_int;
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{SideDataProps, SideDataType};
    use ffmpeg_bindings::libavutil::libavutil::side_data::{SideDataArray, SideDataFlags};

    use super::side_data::{
        CArray, FLAGS, KINDS, PAIRS, c_buffer, c_dict, compare, getters, rust_seen, xorshift,
    };
    use super::side_data_fixture::{SAMPLE, array};

    /// Property filters: none, each named one, combinations, a bit no
    /// descriptor has, and every bit (C's `int` -1).
    const PROPS: [u32; 8] = [
        0,
        SideDataProps::GLOBAL.bits(),
        SideDataProps::MULTI.bits(),
        SideDataProps::SIZE_DEPENDENT.bits() | SideDataProps::COLOR_DEPENDENT.bits(),
        SideDataProps::CHANNEL_DEPENDENT.bits(),
        SideDataProps::all().bits(),
        1 << 30,
        u32::MAX,
    ];

    fn kind(raw: u32) -> SideDataType {
        SideDataType::from_raw(ffi::AVFrameSideDataType(raw))
    }

    /// The index of C's entry `e` in `c`.
    fn index(c: &CArray, e: *const ffi::AVFrameSideData) -> Option<usize> {
        (0..c.nb as usize).find(|&i| ptr::eq(c.entry(i), e))
    }

    /// The source arrays clones come from: [`SAMPLE`] built through the
    /// safe API, and the same entries built by C.
    fn sources() -> (SideDataArray, CArray) {
        let mut c = CArray::new();
        for spec in &SAMPLE {
            let (ok, _) = c.add(
                spec.kind.into_raw().0,
                c_buffer(spec.payload),
                SideDataFlags::NONE,
            );
            assert!(ok);
            let n = c.nb as usize - 1;
            let _ = c.replace_metadata(n, c_dict(spec.metadata));
        }
        (array(&SAMPLE), c)
    }

    /// `av_frame_side_data_new` on C's array, with its payload then written
    /// as `fill` says, as the wrappers do.
    fn c_new(
        c: &mut CArray,
        k: u32,
        size: usize,
        flags: SideDataFlags,
        fill: Option<&[u8]>,
    ) -> Option<usize> {
        // For the unsafe operations below: the array C built; the payload of an entry C returns is
        // written in full before anything reads it.
        let e = {
            // SAFETY: as the comment above this block states.
            let e = unsafe {
                ffi::av_frame_side_data_new(
                    &raw mut c.sd,
                    &raw mut c.nb,
                    ffi::AVFrameSideDataType(k),
                    size,
                    flags.bits(),
                )
            };
            if !e.is_null() {
                // SAFETY: as the comment above this block states: a read of the new entry's
                // payload pointer.
                let data = unsafe { (*e).data };
                match fill {
                    // SAFETY: as above; the payload holds `size` bytes, and so does `bytes`.
                    Some(bytes) => unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data, size) },
                    // SAFETY: as above.
                    None => unsafe { ptr::write_bytes(data, 0, size) },
                }
            }
            e
        };
        (!e.is_null()).then(|| index(c, e).expect("C returned an entry of the array"))
    }

    /// Random sequences of every routine, compared after each step.
    fn run(seed: u64, steps: usize) {
        let mut x = seed;
        let (mut r, mut c) = (SideDataArray::new(), CArray::new());
        let (mut r_src, mut c_src) = sources();
        for step in 0..steps {
            let v = xorshift(&mut x);
            let k = KINDS[(v >> 4) as usize % KINDS.len()];
            let flags = FLAGS[(v >> 6) as usize % FLAGS.len()];
            let flags = if v >> 50 & 1 == 1 {
                flags | SideDataFlags::UNIQUE
            } else {
                flags
            };
            let owner = v >> 51 & 1 == 1;
            let payload: Vec<u8> = (0..(v >> 20) % 70).map(|n| (n as u8) ^ (v as u8)).collect();
            let what = v % 8;
            let label = format!("seed {seed} step {step} op {what} kind {k} flags {flags:?}");
            match what {
                0 | 1 => {
                    let fill = (what == 1).then_some(&payload[..]);
                    let got = match (fill, owner) {
                        (None, true) => r
                            .alloc_zeroed(kind(k), payload.len(), flags)
                            .map(|e| getters(e.as_ref())),
                        (None, false) => r
                            .as_mut()
                            .alloc_zeroed(kind(k), payload.len(), flags)
                            .map(|e| getters(e.as_ref())),
                        (Some(b), true) => r
                            .alloc_from_slice(kind(k), b, flags)
                            .map(|e| getters(e.as_ref())),
                        (Some(b), false) => r
                            .as_mut()
                            .alloc_from_slice(kind(k), b, flags)
                            .map(|e| getters(e.as_ref())),
                    };
                    let want = c_new(&mut c, k, payload.len(), flags, fill).map(|i| {
                        let s = c.seen().swap_remove(i);
                        (s.kind, s.bytes, s.metadata)
                    });
                    assert_eq!(got, want, "{label}");
                }
                2 => {
                    let j = (v >> 12) as usize % SAMPLE.len();
                    let got = r.as_mut().add_clone(r_src.as_ref().get(j).unwrap(), flags);
                    // SAFETY: C's array and a live entry of C's source.
                    let want = unsafe {
                        ffi::av_frame_side_data_clone(
                            &raw mut c.sd,
                            &raw mut c.nb,
                            c_src.entry(j),
                            flags.bits(),
                        )
                    };
                    assert_eq!(got, if want < 0 { Err(want) } else { Ok(()) }, "{label}");
                }
                3 => {
                    r.as_mut().remove(kind(k));
                    // SAFETY: C's array.
                    unsafe {
                        ffi::av_frame_side_data_remove(
                            &raw mut c.sd,
                            &raw mut c.nb,
                            ffi::AVFrameSideDataType(k),
                        );
                    };
                }
                4 => {
                    let props = PROPS[(v >> 12) as usize % PROPS.len()];
                    r.as_mut()
                        .remove_by_props(SideDataProps::from_bits_retain(props));
                    // SAFETY: C's array.
                    unsafe {
                        ffi::av_frame_side_data_remove_by_props(
                            &raw mut c.sd,
                            &raw mut c.nb,
                            props as c_int,
                        );
                    };
                }
                5 => {
                    let got = r.as_ref().find(kind(k)).map(getters);
                    // SAFETY: C's array, read for the call.
                    let e = unsafe {
                        ffi::av_frame_side_data_get_c(
                            c.sd.cast(),
                            c.nb,
                            ffi::AVFrameSideDataType(k),
                        )
                    };
                    let want = (!e.is_null()).then(|| {
                        let s = c.seen().swap_remove(index(&c, e).unwrap());
                        (s.kind, s.bytes, s.metadata)
                    });
                    assert_eq!(got, want, "{label}");
                }
                6 => {
                    // Marks the entry found with another kind, which tells
                    // in the comparison which one it was.
                    let mark = KINDS[(v >> 12) as usize % KINDS.len()] ^ 2;
                    let found = if owner {
                        r.find_mut(kind(k)).map(|mut e| e.set_kind(kind(mark)))
                    } else {
                        r.as_mut()
                            .find_mut(kind(k))
                            .map(|mut e| e.set_kind(kind(mark)))
                    };
                    // SAFETY: C's array; the entry C returns is one of its
                    // own, written as the array's holder may.
                    let e = unsafe {
                        ffi::av_frame_side_data_get(c.sd, c.nb, ffi::AVFrameSideDataType(k))
                    };
                    if !e.is_null() {
                        // SAFETY: as above.
                        unsafe { (*e.cast_mut()).type_ = ffi::AVFrameSideDataType(mark) };
                    }
                    assert_eq!(found.is_some(), !e.is_null(), "{label}");
                }
                _ => {
                    let byte = (v >> 30) as u8;
                    let wrote = r
                        .as_mut()
                        .find_mut(kind(k))
                        .and_then(|mut e| e.data_mut().map(|d| d.fill(byte)))
                        .is_some();
                    // For the unsafe operations below: C's array; a payload is written only when
                    // its buffer is sole and not read-only.
                    let c_wrote = {
                        // SAFETY: as the comment above this block states.
                        let e = unsafe {
                            ffi::av_frame_side_data_get(c.sd, c.nb, ffi::AVFrameSideDataType(k))
                        };
                        let ok = !e.is_null() && {
                            // SAFETY: as the comment above this block states: `e` is a live entry
                            // of C's array; a read of its reference.
                            let buf = unsafe { (*e).buf };
                            // SAFETY: as above, a live reference.
                            unsafe { ffi::av_buffer_is_writable(buf) != 0 }
                        };
                        if ok {
                            // SAFETY: as above: reads of the entry's window.
                            let (data, size) = (unsafe { (*e).data }, unsafe { (*e).size });
                            // SAFETY: as above; the buffer is sole and not read-only.
                            unsafe { ptr::write_bytes(data, byte, size) };
                        }
                        ok
                    };
                    assert_eq!(wrote, c_wrote, "{label}");
                }
            }
            compare(&mut r, &c, &label);
            assert_eq!(rust_seen(&mut r_src), c_src.seen(), "{label}: sources");
            if step == steps / 2 {
                // Clones outlive their sources, whose buffers they share.
                (r_src, c_src) = sources();
                compare(&mut r, &c, &format!("{label}: sources replaced"));
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn random_sequences_match_c_after_every_step() {
        for seed in 1..=40u64 {
            run(seed.wrapping_mul(0x2545_F491_4F6C_DD1D), 120);
        }
    }

    /// Whether this is the isolated child of test `name`; in the parent, runs
    /// the child, asserts it passed, and returns `false`.
    fn isolated(name: &str) -> bool {
        const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";
        if std::env::var_os(ISOLATED).is_some() {
            return true;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1", "--nocapture"])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated run failed: {status}");
        false
    }

    /// Clones and allocations failing at `av_max_alloc` limits, in an
    /// isolated child as the limit is process-wide: each limit fails a
    /// different allocation of C's clone (the new reference, the pointer
    /// block, the entry, the metadata copy, whose failure removes the entry
    /// again) and of its replacement (the metadata copy, the reference), and
    /// the payload of an allocation. Both sides fail alike, with the arrays
    /// compared once the limit is lifted.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn failures_at_the_av_max_alloc_limit_match_c() {
        let name = "frame::side_data_array_routines::failures_at_the_av_max_alloc_limit_match_c";
        if !isolated(name) {
            return;
        }
        let set_max = ffmpeg_bindings::libavutil::libavutil::mem::set_max_alloc;
        let key = std::ffi::CString::new(vec![b'k'; 150]).unwrap();
        let long: &[(&core::ffi::CStr, &core::ffi::CStr)] = &[(&key, c"v")];
        // Source entries: a unique kind and a MULTI one, each with and
        // without metadata whose key C copies in one 151-byte block.
        let specs = [
            (SideDataType::AFD, long),
            (SideDataType::AFD, &[][..]),
            (SideDataType::SEI_UNREGISTERED, long),
            (SideDataType::SEI_UNREGISTERED, &[][..]),
        ];
        for limit in [0, 16, 23, 24, 32, 39, 40, 47, 48, 64, 100, 150, 151, 200] {
            for (j, &(k, pairs)) in specs.iter().enumerate() {
                for flags in [SideDataFlags::NONE, SideDataFlags::REPLACE] {
                    let label = format!("limit {limit} source {j} flags {flags:?}");
                    let (mut r_src, mut c_src) = (SideDataArray::new(), CArray::new());
                    let mut e = r_src
                        .alloc_from_slice(k, b"src", SideDataFlags::NONE)
                        .unwrap();
                    e.set_metadata(
                        ffmpeg_bindings::libavutil::libavutil::dict::Dict::from_entries(
                            pairs.iter().copied(),
                        )
                        .unwrap(),
                    );
                    let i = c_new(
                        &mut c_src,
                        k.into_raw().0,
                        3,
                        SideDataFlags::NONE,
                        Some(b"src"),
                    )
                    .unwrap();
                    let _ = c_src.replace_metadata(i, c_dict(pairs));
                    // Destinations holding an entry of each kind already.
                    let (mut r, mut c) = (SideDataArray::new(), CArray::new());
                    for k2 in [SideDataType::AFD, SideDataType::SEI_UNREGISTERED] {
                        r.alloc_from_slice(k2, b"old", SideDataFlags::NONE).unwrap();
                        c_new(
                            &mut c,
                            k2.into_raw().0,
                            3,
                            SideDataFlags::NONE,
                            Some(b"old"),
                        )
                        .unwrap();
                    }
                    set_max(limit);
                    let got = r.as_mut().add_clone(r_src.as_ref().get(0).unwrap(), flags);
                    // SAFETY: C's array and a live entry of C's source.
                    let want = unsafe {
                        ffi::av_frame_side_data_clone(
                            &raw mut c.sd,
                            &raw mut c.nb,
                            c_src.entry(0),
                            flags.bits(),
                        )
                    };
                    set_max(i32::MAX as usize);
                    assert_eq!(
                        got,
                        if want < 0 { Err(want) } else { Ok(()) },
                        "{label}: clone"
                    );
                    compare(&mut r, &c, &format!("{label}: clone"));
                    // An allocation with no entry to replace, which C
                    // reaches itself.
                    let size = limit / 2 + 8;
                    set_max(limit);
                    let got = r
                        .alloc_zeroed(SideDataType::ICC_PROFILE, size, SideDataFlags::NONE)
                        .is_some();
                    let want = c_new(
                        &mut c,
                        SideDataType::ICC_PROFILE.into_raw().0,
                        size,
                        SideDataFlags::NONE,
                        None,
                    )
                    .is_some();
                    set_max(i32::MAX as usize);
                    assert_eq!(got, want, "{label}: alloc");
                    compare(&mut r, &c, &format!("{label}: alloc"));
                    assert_eq!(rust_seen(&mut r_src), c_src.seen(), "{label}: source");
                }
            }
        }
    }

    /// Clones replacing an entry in place, which the wrapper runs itself
    /// rather than through C's leaking path (buffer-frame wave-3 symbol
    /// review): sources whose metadata C's flags-0 copy merges (keys equal
    /// but for case, exact duplicates) or keeps, over entries with and
    /// without metadata, sharing the source's buffer or not, and of a kind
    /// with no name; then, in an isolated child, every `av_max_alloc` limit
    /// up to a full copy for sources whose copy C cannot leave part way
    /// (one entry once merged), so that both fail at the metadata copy or
    /// the new reference alike.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn in_place_clones_match_c() {
        let sources: [&[(&core::ffi::CStr, &core::ffi::CStr)]; 5] = [
            &[],
            &[(c"Key", c"1"), (c"KEY", c"2"), (c"x", c"y")],
            &[(c"dup", c"1"), (c"dup", c"2")],
            &[(c"a", c"1"), (c"b", c"2"), (c"c", c"")],
            &[(c"k", c"v"), (c"K", c"w")],
        ];
        let limited = std::env::var_os("CRUSTIFY_EQUIV_ISOLATED").is_some();
        let name = "frame::side_data_array_routines::in_place_clones_match_c";
        let limits: Vec<usize> = if limited {
            (0..=64).collect()
        } else {
            vec![i32::MAX as usize]
        };
        for &k in &[SideDataType::AFD.into_raw().0, 1000] {
            for (j, pairs) in sources.iter().enumerate() {
                // Distinct entries copy one by one, so a limit could fail C
                // part way; those run unlimited only.
                let merged = c_pairs_count(pairs) <= 1;
                for &limit in limits.iter().filter(|_| merged || !limited) {
                    for (shared, old) in [(false, &PAIRS[0]), (false, &PAIRS[2]), (true, &PAIRS[1])]
                    {
                        let label = format!("kind {k} source {j} limit {limit} shared {shared}");
                        let (mut r_src, mut c_src) = (SideDataArray::new(), CArray::new());
                        let mut e = r_src
                            .alloc_from_slice(kind(k), b"source", SideDataFlags::NONE)
                            .unwrap();
                        e.set_metadata(
                            ffmpeg_bindings::libavutil::libavutil::dict::Dict::from_entries(
                                pairs.iter().copied(),
                            )
                            .unwrap(),
                        );
                        let i =
                            c_new(&mut c_src, k, 6, SideDataFlags::NONE, Some(b"source")).unwrap();
                        let _ = c_src.replace_metadata(i, c_dict(pairs));
                        let (mut r, mut c) = (SideDataArray::new(), CArray::new());
                        let mut e = r
                            .alloc_from_slice(kind(k), b"old", SideDataFlags::NONE)
                            .unwrap();
                        e.set_metadata(
                            ffmpeg_bindings::libavutil::libavutil::dict::Dict::from_entries(
                                old.iter().copied(),
                            )
                            .unwrap(),
                        );
                        if shared {
                            let b = r_src.as_ref().get(0).unwrap().buffer().try_clone().unwrap();
                            e.set_buffer(b);
                        }
                        let i = c_new(&mut c, k, 3, SideDataFlags::NONE, Some(b"old")).unwrap();
                        let _ = c.replace_metadata(i, c_dict(old));
                        if shared {
                            // SAFETY: a read of the reference of C's source's live entry.
                            let src_buf = unsafe { (*c_src.entry(0)).buf };
                            // SAFETY: a live reference, to which C adds one.
                            let b = unsafe { ffi::av_buffer_ref(src_buf) };
                            assert!(!b.is_null());
                            let e = c.entry(i);
                            // For the unsafe operations below: a live entry of C's array, which
                            // takes over `b` and its window, as
                            // `set_buffer` does.
                            {
                                // SAFETY: `e` is to a live object, as the block below relies on, so
                                // the place of its `buf` field is
                                // in bounds; nothing is accessed here.
                                let buf_ptr = unsafe { &raw mut (*e).buf };
                                // SAFETY: as the comment above this block states.
                                unsafe { ffi::av_buffer_unref(buf_ptr) };
                                // SAFETY: as the comment above this block states.
                                unsafe { (*e).buf = b };
                                // SAFETY: `b` is a live reference: reads of its window.
                                let (data, size) = (unsafe { (*b).data }, unsafe { (*b).size });
                                // SAFETY: as the comment above this block states.
                                unsafe { (*e).data = data };
                                // SAFETY: as the comment above this block states.
                                unsafe { (*e).size = size };
                            }
                        }
                        compare(&mut r, &c, &label);
                        let set_max = ffmpeg_bindings::libavutil::libavutil::mem::set_max_alloc;
                        set_max(limit);
                        let got = r
                            .as_mut()
                            .add_clone(r_src.as_ref().get(0).unwrap(), SideDataFlags::REPLACE);
                        // SAFETY: C's array and a live entry of C's source.
                        let want = unsafe {
                            ffi::av_frame_side_data_clone(
                                &raw mut c.sd,
                                &raw mut c.nb,
                                c_src.entry(0),
                                SideDataFlags::REPLACE.bits(),
                            )
                        };
                        set_max(i32::MAX as usize);
                        assert_eq!(got, if want < 0 { Err(want) } else { Ok(()) }, "{label}");
                        compare(&mut r, &c, &format!("{label}: clone"));
                        assert_eq!(rust_seen(&mut r_src), c_src.seen(), "{label}: source");
                        drop((r_src, c_src));
                        compare(&mut r, &c, &format!("{label}: source gone"));
                    }
                }
            }
        }
        if !limited {
            assert!(!isolated(name));
        }
    }

    /// The number of entries C's flags-0 copy of `pairs` keeps: keys equal
    /// but for ASCII case merge.
    fn c_pairs_count(pairs: &[(&core::ffi::CStr, &core::ffi::CStr)]) -> usize {
        let mut keys: Vec<Vec<u8>> = pairs
            .iter()
            .map(|(k, _)| k.to_bytes().to_ascii_uppercase())
            .collect();
        keys.sort();
        keys.dedup();
        keys.len()
    }

    /// Each flag combination of `alloc_*` and `add_clone` over an array
    /// that holds an entry of every kind the sequences use, each with
    /// metadata, against the same calls in C.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_flag_over_present_kinds_matches_c() {
        let all = [
            SideDataFlags::NONE,
            SideDataFlags::UNIQUE,
            SideDataFlags::REPLACE,
            SideDataFlags::UNIQUE | SideDataFlags::REPLACE,
        ];
        let (r_src, c_src) = sources();
        for flags in all {
            for (n, &k) in KINDS.iter().enumerate() {
                let (mut r, mut c) = (SideDataArray::new(), CArray::new());
                for (m, &k2) in KINDS.iter().enumerate() {
                    let pairs = PAIRS[m % PAIRS.len()];
                    let mut e = r
                        .alloc_from_slice(kind(k2), &[m as u8; 3], SideDataFlags::NONE)
                        .unwrap();
                    e.set_metadata(
                        ffmpeg_bindings::libavutil::libavutil::dict::Dict::from_entries(
                            pairs.iter().copied(),
                        )
                        .unwrap(),
                    );
                    let i = c_new(&mut c, k2, 3, SideDataFlags::NONE, Some(&[m as u8; 3])).unwrap();
                    let _ = c.replace_metadata(i, c_dict(pairs));
                }
                let label = format!("kind {k} flags {flags:?}");
                compare(&mut r, &c, &label);
                let got = r
                    .alloc_zeroed(kind(k), 5, flags)
                    .map(|e| getters(e.as_ref()));
                let want = c_new(&mut c, k, 5, flags, None).map(|i| {
                    let s = c.seen().swap_remove(i);
                    (s.kind, s.bytes, s.metadata)
                });
                assert_eq!(got, want, "{label}: alloc");
                compare(&mut r, &c, &format!("{label}: alloc"));
                for j in [n, SAMPLE.len() - 1 - n] {
                    let got = r.as_mut().add_clone(r_src.as_ref().get(j).unwrap(), flags);
                    // SAFETY: C's array and a live entry of C's source.
                    let want = unsafe {
                        ffi::av_frame_side_data_clone(
                            &raw mut c.sd,
                            &raw mut c.nb,
                            c_src.entry(j),
                            flags.bits(),
                        )
                    };
                    assert_eq!(
                        got,
                        if want < 0 { Err(want) } else { Ok(()) },
                        "{label}: clone {j}"
                    );
                    compare(&mut r, &c, &format!("{label}: clone {j}"));
                }
            }
        }
    }
}
