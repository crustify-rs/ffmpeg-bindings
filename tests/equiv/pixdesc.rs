//! Equivalence of `libavutil/pixdesc.h`'s safe API with the C library.

/// [`ComponentDescriptor`] against C's `AVComponentDescriptor`: the safe
/// getters against C's fields for every component of every pixel format
/// descriptor, and descriptors rebuilt through the safe setters, in
/// `av_malloc`-owned storage, against C's own table, as C's readers
/// (`av_read_image_line2`, `av_get_padded_bits_per_pixel`) observe them
/// after each component is written.
///
/// [`ComponentDescriptor`]: ffmpeg::libavutil::pixdesc::ComponentDescriptor
mod component_descriptor {
    use core::ffi::{CStr, c_int, c_void};
    use core::ptr;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::alloc_cell_array_zeroed;
    use ffmpeg::libavutil::pixdesc::{
        ComponentDescriptor, ComponentDescriptorMut, ComponentDescriptorRef,
    };

    /// Pixels read per line.
    const W: usize = 8;
    /// Bytes per image row; wide enough for 8 pixels of a 32-byte step plus
    /// C's largest component offset.
    const LINESIZE: usize = 512;
    /// Rows per plane; C reads row 1, so a negative offset stays in bounds.
    const ROWS: usize = 3;

    /// Copies of every descriptor in C's table, in C's order.
    fn c_table() -> Vec<ffi::AVPixFmtDescriptor> {
        let mut out = Vec::new();
        let mut p = ptr::null();
        loop {
            // SAFETY: `p` is NULL or the previous descriptor
            // `av_pix_fmt_desc_next` returned, a pointer into C's static
            // table, as it requires.
            p = unsafe { ffi::av_pix_fmt_desc_next(p) };
            if p.is_null() {
                return out;
            }
            // SAFETY: `p` points to a live, initialised descriptor in C's
            // static, never-written table; the copy is a plain value whose
            // pointer fields still address that static table.
            out.push(unsafe { p.read() });
        }
    }

    fn name(d: &ffi::AVPixFmtDescriptor) -> String {
        // SAFETY: every table descriptor's `name` is a static NUL-terminated
        // string.
        unsafe { CStr::from_ptr(d.name) }
            .to_string_lossy()
            .into_owned()
    }

    /// Four planes of pseudo-random bytes, so a component read from the
    /// wrong plane, offset, step, shift or depth reads different values.
    fn image() -> [Vec<u8>; 4] {
        let mut x = 0x2545_f491_u32;
        core::array::from_fn(|_| {
            (0..LINESIZE * ROWS)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x as u8
                })
                .collect()
        })
    }

    /// What C observes of component `c` of `desc`: the `W` values
    /// `av_read_image_line2` reads from row 1 of `img`, and the format's
    /// padded bits per pixel.
    fn c_observes(
        desc: &ffi::AVPixFmtDescriptor,
        c: usize,
        img: &[Vec<u8>; 4],
    ) -> ([u32; W], c_int) {
        let mut data: [*const u8; 4] = core::array::from_fn(|i| img[i].as_ptr());
        let linesize = [LINESIZE as c_int; 4];
        let mut dst = [0u32; W];
        // SAFETY: `desc` is a live descriptor whose components are each
        // either C's own table entry for its format, addressing planes 0..4
        // with C's offsets, steps and depths, or all-zero, whose `depth` of 0
        // makes C return before reading; every plane holds
        // `ROWS` rows of `LINESIZE` bytes, enough for `W` pixels of row 1 at
        // the table's widest step and offset. `dst` holds `W` 4-byte
        // elements and `read_pal_component` is 0, so `data[1]` is never
        // indexed as a palette. `av_get_padded_bits_per_pixel` reads
        // `desc`'s first `nb_components` components, whose planes are 0..4.
        unsafe {
            ffi::av_read_image_line2(
                dst.as_mut_ptr().cast::<c_void>(),
                data.as_mut_ptr(),
                linesize.as_ptr(),
                desc,
                0,
                1,
                c as c_int,
                W as c_int,
                0,
                4,
            );
            (dst, ffi::av_get_padded_bits_per_pixel(desc))
        }
    }

    fn c_fields(d: &ffi::AVComponentDescriptor) -> [i32; 5] {
        [d.plane, d.step, d.offset, d.shift, d.depth]
    }

    fn rust_fields(d: ComponentDescriptorRef<'_>) -> [i32; 5] {
        [d.plane(), d.step(), d.offset(), d.shift(), d.depth()]
    }

    /// Writes `from`'s fields into `to` one setter at a time.
    fn copy_fields(from: ComponentDescriptorRef<'_>, mut to: ComponentDescriptorMut<'_>) {
        to.set_plane(from.plane());
        to.set_step(from.step());
        to.set_offset(from.offset());
        to.set_shift(from.shift());
        to.set_depth(from.depth());
    }

    #[test]
    fn getters_read_what_c_stores_for_every_component() {
        let table = c_table();
        assert!(table.len() > 200, "C has {} descriptors", table.len());
        let mut negative_offsets = 0;
        for desc in &table {
            for raw in &desc.comp {
                let d = ComponentDescriptor::from_raw(*raw);
                assert_eq!(rust_fields(d.as_ref()), c_fields(raw), "{}", name(desc));
                assert_eq!(rust_fields(d.as_ref().get().as_ref()), c_fields(raw));
                let back = d.into_raw();
                assert_eq!(c_fields(&back), c_fields(raw));
                negative_offsets += usize::from(raw.offset < 0);
            }
        }
        // C's big-endian 16-bit packed RGB formats store `offset = -1`.
        assert!(negative_offsets > 0);
    }

    #[test]
    fn components_written_through_setters_read_like_c_s_table() {
        let img = image();
        let mut observed_reads = 0;
        for c_desc in c_table() {
            let nb = usize::from(c_desc.nb_components);
            // C side: C's descriptor with every component cleared, then
            // restored by plain C-layout assignment, one component at a
            // time.
            let mut c_side = c_desc;
            c_side.comp = [ffi::AVComponentDescriptor::default(); 4];
            // Rust side: an independently owned descriptor whose components
            // come from an `av_malloc`-owned array written by the safe
            // setters.
            let mut rust_side = c_desc;
            let mut comps = alloc_cell_array_zeroed::<ComponentDescriptor>(4).unwrap();
            for i in 0..4 {
                rust_side.comp[i] = comps.as_handles().get(i).unwrap().get().into_raw();
            }
            let check = |c_side: &ffi::AVPixFmtDescriptor,
                         rust_side: &ffi::AVPixFmtDescriptor,
                         step: &str| {
                for c in 0..4 {
                    assert_eq!(
                        c_observes(c_side, c, &img),
                        c_observes(rust_side, c, &img),
                        "{} component {c} after {step}",
                        name(&c_desc)
                    );
                }
            };
            check(&c_side, &rust_side, "clearing");
            for c in 0..nb {
                c_side.comp[c] = c_desc.comp[c];
                let source = ComponentDescriptor::from_raw(c_desc.comp[c]);
                {
                    let mut handles = comps.as_handles_mut();
                    copy_fields(source.as_ref(), handles.get_mut(c).unwrap());
                }
                let written = comps.as_handles().get(c).unwrap();
                assert_eq!(rust_fields(written), c_fields(&c_side.comp[c]));
                rust_side.comp[c] = written.get().into_raw();
                check(&c_side, &rust_side, &format!("writing component {c}"));
                observed_reads += usize::from(c_observes(&c_side, c, &img).0 != [0; W]);
            }
            // Finally the whole component array in one `set` each, from
            // the values the Rust array holds.
            let mut whole = c_desc;
            for (i, h) in comps.as_handles().iter().enumerate() {
                let mut v = ComponentDescriptor::new();
                v.as_mut().set(h.get());
                whole.comp[i] = v.into_raw();
            }
            check(&c_side, &whole, "setting whole components");
        }
        // Not vacuous: most components read non-zero pixels.
        assert!(observed_reads > 500, "{observed_reads}");
    }
}
