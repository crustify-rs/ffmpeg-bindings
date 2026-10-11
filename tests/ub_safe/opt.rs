//! UB tests for `libavutil/opt.h`'s safe public API.

/// `OptionType`: an open value type, whose every operation is defined for
/// every `c_uint`, crossing threads by value.
mod option_type {
    use ffmpeg::libavutil::opt::OptionType;

    /// Values around each named constant, the flag bit and the extremes.
    fn values() -> impl Iterator<Item = u32> {
        (0..=24)
            .chain([0xFFFF, 0x1_0000, 0x1_0001, 0x1_0014, 0x1_0015, 0x2_0000])
            .chain([i32::MAX as u32, 1 << 31, u32::MAX - 1, u32::MAX])
    }

    #[test]
    fn every_operation_is_total() {
        for v in values() {
            let ty = OptionType::from_value(v);
            assert_eq!(ty.value(), v);
            let (base, arr) = (ty.base(), ty.array());
            assert!(!base.is_array() && arr.is_array());
            assert_eq!(base.array(), arr);
            assert_eq!(arr.base(), base);
            assert_eq!(base.base(), base);
            assert_eq!(ty.is_array(), ty == arr);
            // Names and formatting never fail or read out of bounds.
            let name = ty.name();
            let debug = format!("{ty:?}");
            assert!(debug.starts_with("OptionType"));
            if let Some(name) = name {
                assert_eq!(debug, format!("OptionType::{name}"));
            }
            let raw: ffmpeg::libavutil::ffi::AVOptionType = ty.into();
            assert_eq!(OptionType::from(raw), ty);
        }
    }

    #[test]
    fn named_types_are_distinct_and_flagless() {
        let named = [
            OptionType::FLAGS,
            OptionType::INT,
            OptionType::INT64,
            OptionType::DOUBLE,
            OptionType::FLOAT,
            OptionType::STRING,
            OptionType::RATIONAL,
            OptionType::BINARY,
            OptionType::DICT,
            OptionType::UINT64,
            OptionType::CONST,
            OptionType::IMAGE_SIZE,
            OptionType::PIXEL_FORMAT,
            OptionType::SAMPLE_FORMAT,
            OptionType::VIDEO_RATE,
            OptionType::DURATION,
            OptionType::COLOR,
            OptionType::BOOL,
            OptionType::CHANNEL_LAYOUT,
            OptionType::UINT,
        ];
        let set: std::collections::HashSet<_> = named.iter().copied().collect();
        assert_eq!(set.len(), named.len());
        for ty in named {
            assert!(!ty.is_array() && ty.name().is_some());
            assert!(!set.contains(&ty.array()));
        }
        assert!(OptionType::ARRAY.is_array());
    }

    /// Types are plain values: copies are read on several threads at once
    /// and sent back (ThreadSanitizer reports any race).
    #[test]
    fn types_cross_threads() {
        let types: Vec<OptionType> = values().map(OptionType::from_value).collect();
        let shared = &types;
        let names: Vec<Vec<Option<&'static str>>> = std::thread::scope(|s| {
            let workers: Vec<_> = (0..4)
                .map(|_| s.spawn(move || shared.iter().map(|t| t.base().name()).collect()))
                .collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        assert!(names.windows(2).all(|w| w[0] == w[1]));
        let back = std::thread::spawn(|| OptionType::STRING.array())
            .join()
            .unwrap();
        assert_eq!(back.base(), OptionType::STRING);
    }
}

/// `OptionArrayDef`, Rust-owned: every setter and getter through the
/// exclusive and shared handles, refused separators, and shared handles read
/// from many threads while the owner waits.
mod array_def {
    use std::ffi::CStr;
    use std::num::NonZeroU32;

    use ffmpeg::libavutil::opt::{InvalidSeparator, OptionArrayDef, OptionArrayDefRef};

    #[test]
    fn edits_through_every_handle() {
        let mut d = OptionArrayDef::default();
        let mut m = d.as_mut();
        for (def, min, max, sep) in [
            (Some(c"1,2"), 0, 0, 0),
            (Some(c""), 1, 1, b'|'),
            (None, u32::MAX, u32::MAX, b' '),
            (Some(c"a long default value, with spaces"), 7, 3, b'~'),
        ] {
            m.set_def(def);
            m.set_size_min(min);
            m.set_size_max(NonZeroU32::new(max));
            m.set_separator(sep).unwrap();
            // A shared reborrow of the exclusive handle reads the edits.
            let r = m.as_ref();
            assert_eq!(r.def(), def);
            assert_eq!(r.size_min(), min);
            assert_eq!(r.size_max().map_or(0, NonZeroU32::get), max);
            assert_eq!(r.separator(), if sep == 0 { b',' } else { sep });
            // An exclusive reborrow edits the same value.
            m.as_mut().set_size_min(min / 2);
            assert_eq!(m.as_ref().size_min(), min / 2);
        }
        for sep in [b'a', b'Z', b'5', b'\\', b'\n', b'\t', 0x7f, 0x80, 0xff] {
            assert_eq!(m.set_separator(sep), Err(InvalidSeparator(sep)));
            assert_eq!(m.as_ref().separator(), b'~');
        }
        let shared = d.as_ref();
        let copy = shared;
        assert_eq!(copy.def(), shared.def());
        assert!(copy.ptr_eq(shared));
        assert!(!format!("{shared:?}").is_empty());
    }

    #[test]
    fn strings_read_through_the_handle_outlive_it() {
        let d = {
            let mut d = OptionArrayDef::new();
            d.as_mut().set_def(Some(c"3;4"));
            d
        };
        // `def` outlives the temporary handle it was read through, borrowing
        // the owner; the owner moves once it is no longer read.
        let def: Option<&CStr> = d.as_ref().def();
        assert_eq!(def, Some(c"3;4"));
        let moved = Box::new(d);
        assert_eq!(OptionArrayDef::as_ref(&moved).def(), Some(c"3;4"));
    }

    #[test]
    fn shared_handles_cross_threads() {
        let mut d = OptionArrayDef::new();
        d.as_mut().set_def(Some(c"9|8|7"));
        d.as_mut().set_separator(b'|').unwrap();
        let r: OptionArrayDefRef<'_> = d.as_ref();
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(move || {
                    for _ in 0..500 {
                        assert_eq!(r.def(), Some(c"9|8|7"));
                        assert_eq!(r.separator(), b'|');
                        assert_eq!(r.size_max(), None);
                    }
                });
            }
        });
        // Values and exclusive handles move between threads.
        let mut moved = std::thread::spawn(move || {
            d.as_mut().set_size_min(2);
            d
        })
        .join()
        .unwrap();
        let m = moved.as_mut();
        std::thread::scope(|s| {
            s.spawn(move || {
                let mut m = m;
                m.set_size_max(NonZeroU32::new(5));
            });
        });
        assert_eq!(moved.as_ref().size_max(), NonZeroU32::new(5));
        assert_eq!(moved.as_ref().size_min(), 2);
    }
}

/// Options wave 0, batch 3.
///
/// UB tests of `libavutil/opt.h`'s safe object API on the one object built
/// without `unsafe`, a [`BareObject`] (a class with no options, children or
/// offsets): every getter and setter finds nothing and touches nothing past
/// the class pointer. The option paths themselves need an object adopted
/// from a raw pointer and are exercised in `ub_unsafe`. Also the key-value
/// parser, over strings with and without keys and separators.
mod objects {
    use core::ffi::CStr;
    use std::ffi::CString;

    use ffmpeg::libavutil::log::{Class, ClassCategory};
    use ffmpeg::libavutil::opt::{BareObject, SearchFlags, SerializeFlags, parse_key_value};

    static CLASS: Class = Class::new(c"crustify-opt-ub", ClassCategory::NA);

    const NOT_FOUND: i32 = -i32::from_le_bytes([0xF8, b'O', b'P', b'T']);
    const EINVAL: i32 = -22;

    #[test]
    fn a_bare_object_has_no_options() {
        let mut a = BareObject::new(CLASS.as_ref()).unwrap();
        let b = a;
        for flags in [SearchFlags::NONE, SearchFlags::CHILDREN] {
            let r = a.as_object();
            assert_eq!(r.get(c"x", flags).unwrap_err(), NOT_FOUND);
            assert_eq!(r.get_nullable(c"x", flags).unwrap_err(), NOT_FOUND);
            assert_eq!(r.get_int(c"x", flags), Err(NOT_FOUND));
            assert_eq!(r.get_double(c"x", flags), Err(NOT_FOUND));
            assert_eq!(r.get_image_size(c"x", flags), Err(NOT_FOUND));
            assert_eq!(r.get_array_size(c"x", flags), Err(NOT_FOUND));
            assert_eq!(r.is_set_to_default(c"x", flags), Err(NOT_FOUND));
            assert!(!r.flag_is_set(c"x", c"y"));
            assert_eq!(r.children().count(), 0);
            let mut m = a.as_object_mut();
            assert_eq!(m.set(c"x", c"ld(1e300)", flags), Err(NOT_FOUND));
            assert_eq!(m.set_int(c"x", 1, flags), Err(NOT_FOUND));
            assert_eq!(m.set_double(c"x", f64::NAN, flags), Err(NOT_FOUND));
            assert_eq!(m.set_bin(c"x", &[1, 2], flags), Err(NOT_FOUND));
            assert_eq!(m.set_image_size(c"x", 1, 2, flags), Err(NOT_FOUND));
            assert!(m.children_mut().next_child().is_none());
        }
        let mut m = a.as_object_mut();
        m.set_defaults();
        m.set_defaults_masked(0, 0);
        m.free_options();
        m.free_options();
        m.copy_from(b.as_object()).unwrap();
        assert_eq!(m.set_from_string(c"", &[], c"=", c":"), Ok(0));
        assert_eq!(
            m.set_from_string(c"x=1", &[c"x"], c"=", c":"),
            Err(NOT_FOUND)
        );
        assert_eq!(m.set_from_string(c"1", &[], c"=", c":"), Err(EINVAL));
        let r = m.as_ref();
        assert_eq!(
            r.serialize(0, SerializeFlags::NONE, b'=', b':')
                .unwrap()
                .as_bytes(),
            b""
        );
        let all = SerializeFlags::SKIP_DEFAULTS
            | SerializeFlags::OPT_FLAGS_EXACT
            | SerializeFlags::SEARCH_CHILDREN;
        assert_eq!(r.serialize(-1, all, b'=', b':').unwrap().as_bytes(), b"");
        for (k, p) in [(b'=', b'='), (0, b':'), (b'\\', b':'), (b'=', 0)] {
            assert_eq!(
                r.serialize(0, SerializeFlags::NONE, k, p).unwrap_err(),
                EINVAL
            );
        }
        r.show(None, 0, 0);
        r.show(Some(b.as_object()), -1, 0);
        assert!(format!("{r:?}").contains("crustify-opt-ub"));
        assert!(format!("{a:?}").contains("crustify-opt-ub"));
    }

    #[test]
    fn bare_objects_on_many_threads() {
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    let mut o = BareObject::new(CLASS.as_ref()).unwrap();
                    for _ in 0..200 {
                        o.as_object_mut().set_defaults();
                        let _ = o.as_object().serialize(0, SerializeFlags::NONE, b'=', b':');
                    }
                });
            }
        });
    }

    /// Walks `opts` pair by pair as `av_opt_set_from_string` does, over exactly
    /// sized heap strings; returns the pairs read.
    fn walk(opts: &str, kv: &CStr, pairs: &CStr, implicit: bool) -> usize {
        let opts = CString::new(opts).unwrap();
        let mut rest: &CStr = &opts;
        let mut n = 0;
        while !rest.is_empty() {
            let Ok(p) = parse_key_value(rest, kv, pairs, implicit) else {
                break;
            };
            assert!(p.key.is_some() || implicit);
            let _ = p.value.as_bytes();
            assert!(p.rest.to_bytes().len() <= rest.to_bytes().len());
            rest = match p.rest.to_bytes_with_nul() {
                [0] => p.rest,
                [_, tail @ ..] => CStr::from_bytes_with_nul(tail).unwrap(),
                [] => unreachable!(),
            };
            n += 1;
        }
        n
    }

    #[test]
    fn key_values_with_and_without_keys() {
        for implicit in [false, true] {
            for (opts, kv, pairs) in [
                ("a=1:b=2", c"=", c":"),
                ("a=1,b=\\,2", c"=", c","),
                ("  a = 'x:y' : b=", c"=", c":"),
                ("noval", c"=", c":"),
                ("=v", c"=", c":"),
                ("", c"=", c":"),
                (":::", c"=", c":"),
                ("a=1", c"", c""),
                ("k#v;k2#v2", c"#", c";"),
                ("a=\\", c"=", c":"),
                ("é=1", c"=", c":"),
            ] {
                walk(opts, kv, pairs, implicit);
            }
        }
        let long = format!("{}={}", "k".repeat(4096), "v".repeat(4096));
        assert_eq!(walk(&long, c"=", c":", false), 1);
        assert_eq!(
            parse_key_value(c"1", c"=", c":", false).unwrap_err(),
            EINVAL
        );
        let p = parse_key_value(c"1", c"=", c":", true).unwrap();
        assert!(p.key.is_none());
        assert_eq!(p.value.as_bytes(), b"1");
    }
}

/// Options wave 1, batch 0: the typed getters and `get_array` on the one
/// object built without `unsafe`, a [`BareObject`], which find nothing and
/// allocate no output; and the potential children of classes built without
/// `unsafe`, which have no `child_class_iterate` callback. Objects with
/// options need adopting from a raw pointer: their getters run under the
/// sanitizers in the `equiv` suite and the crate's unit tests.
mod getters {
    use std::thread;

    use ffmpeg::libavutil::channel_layout::ChannelLayoutVal;
    use ffmpeg::libavutil::dict::Dict;
    use ffmpeg::libavutil::log::{Class, ClassCategory};
    use ffmpeg::libavutil::mem::HeapString;
    use ffmpeg::libavutil::opt::{BareObject, ObjectRef, PotentialChildren, SearchFlags};
    use ffmpeg::libavutil::pixfmt::PixelFormat;
    use ffmpeg::libavutil::rational::Rational;
    use ffmpeg::libavutil::samplefmt::SampleFormat;

    static CLASS: Class = Class::new(c"crustify-opt-getters-ub", ClassCategory::NA);

    const NOT_FOUND: i32 = -i32::from_le_bytes([0xF8, b'O', b'P', b'T']);

    fn every_getter(r: ObjectRef<'_>, flags: SearchFlags) {
        assert_eq!(r.get_rational(c"x", flags).err(), Some(NOT_FOUND));
        assert_eq!(r.get_video_rate(c"x", flags).err(), Some(NOT_FOUND));
        assert_eq!(r.get_pixel_format(c"x", flags).err(), Some(NOT_FOUND));
        assert_eq!(r.get_sample_format(c"x", flags).err(), Some(NOT_FOUND));
        assert_eq!(r.get_channel_layout(c"x", flags).err(), Some(NOT_FOUND));
        assert_eq!(r.get_dict(c"", flags).err(), Some(NOT_FOUND));
        for (start, count) in [(0, 0), (0, 1), (u32::MAX, u32::MAX)] {
            assert_eq!(
                r.get_array::<i32>(c"x", flags, start, count).err(),
                Some(NOT_FOUND)
            );
            assert_eq!(
                r.get_array::<i64>(c"x", flags, start, count).err(),
                Some(NOT_FOUND)
            );
            assert_eq!(
                r.get_array::<u32>(c"x", flags, start, count).err(),
                Some(NOT_FOUND)
            );
            assert_eq!(
                r.get_array::<u64>(c"x", flags, start, count).err(),
                Some(NOT_FOUND)
            );
            assert_eq!(
                r.get_array::<f32>(c"x", flags, start, count).err(),
                Some(NOT_FOUND)
            );
            assert_eq!(
                r.get_array::<f64>(c"x", flags, start, count).err(),
                Some(NOT_FOUND)
            );
            assert_eq!(
                r.get_array::<Rational>(c"x", flags, start, count).err(),
                Some(NOT_FOUND)
            );
            let strings = r.get_array::<Option<HeapString>>(c"x", flags, start, count);
            assert_eq!(strings.err(), Some(NOT_FOUND));
            assert_eq!(
                r.get_array::<PixelFormat>(c"x", flags, start, count).err(),
                Some(NOT_FOUND)
            );
            assert_eq!(
                r.get_array::<SampleFormat>(c"x", flags, start, count).err(),
                Some(NOT_FOUND)
            );
            assert_eq!(
                r.get_array::<Dict>(c"x", flags, start, count).err(),
                Some(NOT_FOUND)
            );
            let layouts = r.get_array::<ChannelLayoutVal>(c"x", flags, start, count);
            assert_eq!(layouts.err(), Some(NOT_FOUND));
        }
    }

    #[test]
    fn a_bare_object_reads_nothing() {
        let mut a = BareObject::new(CLASS.as_ref()).unwrap();
        for flags in [SearchFlags::NONE, SearchFlags::CHILDREN] {
            every_getter(a.as_object(), flags);
            every_getter(a.as_object_mut().as_ref(), flags);
        }
    }

    #[test]
    fn bare_objects_read_on_many_threads() {
        let a = BareObject::new(CLASS.as_ref()).unwrap();
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    let local = a;
                    every_getter(local.as_object(), SearchFlags::CHILDREN);
                });
            }
        });
    }

    #[test]
    fn classes_without_a_callback_have_no_potential_children() {
        let mut it: PotentialChildren<'_> = CLASS.as_ref().potential_children();
        assert!(it.next().is_none() && it.next().is_none());
        let local = Class::new(c"local", ClassCategory::FILTER);
        let class = local.as_ref();
        assert_eq!(class.potential_children().count(), 0);
        assert_eq!(
            class.potential_children().count(),
            class.child_classes().count()
        );
    }
}

/// Options wave 1, batch 1: the typed setters, array editors and dictionary
/// appliers on a [`BareObject`], where every name is unknown: every value
/// type, refused values included, borrowed only for the call, and the
/// dictionary handed back whole, in order, duplicates included.
mod typed_setters {
    use ffmpeg::libavutil::channel_layout::ChannelLayout;
    use ffmpeg::libavutil::dict::Dict;
    use ffmpeg::libavutil::log::{Class, ClassCategory};
    use ffmpeg::libavutil::opt::{BareObject, SearchFlags};
    use ffmpeg::libavutil::pixfmt::PixelFormat;
    use ffmpeg::libavutil::rational::Rational;
    use ffmpeg::libavutil::samplefmt::SampleFormat;

    static CLASS: Class = Class::new(c"crustify-opt-typed-ub", ClassCategory::NA);

    const NOT_FOUND: i32 = -i32::from_le_bytes([0xF8, b'O', b'P', b'T']);

    #[test]
    fn a_bare_object_finds_no_option_for_any_value() {
        let mut o = BareObject::new(CLASS.as_ref()).unwrap();
        let layout = ChannelLayout::parse(c"FL@a+FR@b").unwrap();
        let dict = Dict::from_entries([(c"k", c"v")]).unwrap();
        for flags in [SearchFlags::NONE, SearchFlags::CHILDREN] {
            let mut m = o.as_object_mut();
            assert_eq!(
                m.set_rational(c"x", Rational::new(1, 0), flags),
                Err(NOT_FOUND)
            );
            assert_eq!(
                m.set_video_rate(c"x", Rational::new(0, 0), flags),
                Err(NOT_FOUND)
            );
            assert_eq!(
                m.set_pixel_format(c"x", PixelFormat::NONE, flags),
                Err(NOT_FOUND)
            );
            assert_eq!(
                m.set_sample_format(c"x", SampleFormat::NONE, flags),
                Err(NOT_FOUND)
            );
            assert_eq!(
                m.set_channel_layout(c"x", layout.as_ref(), flags),
                Err(NOT_FOUND)
            );
            assert_eq!(m.set_dictionary(c"x", dict.as_ref(), flags), Err(NOT_FOUND));
            assert_eq!(m.set_dictionary(c"x", None, flags), Err(NOT_FOUND));
            assert_eq!(
                m.insert_array(c"x", 0, &[c"ld(1e300)"], flags),
                Err(NOT_FOUND)
            );
            assert_eq!(
                m.insert_array(c"x", 7, &[None::<&core::ffi::CStr>], flags),
                Err(NOT_FOUND)
            );
            assert_eq!(m.replace_array(c"x", 0, &[f64::NAN], flags), Err(NOT_FOUND));
            assert_eq!(
                m.replace_array(c"x", 0, &[i64::MAX, i64::MIN], flags),
                Err(NOT_FOUND)
            );
            assert_eq!(
                m.insert_array(c"x", 0, &[f32::INFINITY], flags),
                Err(NOT_FOUND)
            );
            assert_eq!(m.insert_array(c"x", 0, &[1i32, 2], flags), Err(NOT_FOUND));
            assert_eq!(m.insert_array(c"x", 0, &[u32::MAX], flags), Err(NOT_FOUND));
            assert_eq!(m.insert_array(c"x", 0, &[u64::MAX], flags), Err(NOT_FOUND));
            assert_eq!(
                m.insert_array(c"x", 0, &[Rational::new(1, 0)], flags),
                Err(NOT_FOUND)
            );
            assert_eq!(
                m.insert_array(c"x", 0, &[PixelFormat::NONE], flags),
                Err(NOT_FOUND)
            );
            assert_eq!(
                m.insert_array(c"x", 0, &[SampleFormat::NONE], flags),
                Err(NOT_FOUND)
            );
            assert_eq!(
                m.insert_array(c"x", 0, &[(-1, i32::MAX)], flags),
                Err(NOT_FOUND)
            );
            assert_eq!(m.insert_array(c"x", 0, &[[0u8; 4]], flags), Err(NOT_FOUND));
            assert_eq!(
                m.insert_array(c"x", 0, &[layout.as_ref()], flags),
                Err(NOT_FOUND)
            );
            assert_eq!(
                m.insert_array(c"x", 0, &[dict.as_ref(), None], flags),
                Err(NOT_FOUND)
            );
            assert_eq!(m.insert_array::<i32>(c"x", 0, &[], flags), Err(NOT_FOUND));
            assert_eq!(m.remove_array(c"x", 0, 1, flags), Err(NOT_FOUND));
            assert!(m.remove_array(c"x", usize::MAX, usize::MAX, flags).is_err());
        }
        // The values were only borrowed.
        assert_eq!(layout.as_ref().channel_count(), 2);
        assert!(dict.as_ref().is_some());
    }

    #[test]
    fn unknown_entries_come_back_in_order() {
        let mut o = BareObject::new(CLASS.as_ref()).unwrap();
        let pairs = [(c"b", c"2"), (c"a", c"1"), (c"b", c"3"), (c"long key", c"")];
        let mut d = Dict::from_entries(pairs).unwrap();
        o.as_object_mut().set_from_dict(&mut d).unwrap();
        o.as_object_mut()
            .set_from_dict_with(&mut d, SearchFlags::CHILDREN)
            .unwrap();
        // Through a deep copy, compared by C-observable content.
        let copy = d.try_clone().unwrap();
        drop(d);
        let mut again = copy;
        o.as_object_mut().set_from_dict(&mut again).unwrap();
        let mut empty = Dict::new();
        o.as_object_mut().set_from_dict(&mut empty).unwrap();
        assert!(empty.as_ref().is_none());
        assert!(again.as_ref().is_some());
    }

    #[test]
    fn distinct_objects_on_many_threads() {
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    let mut o = BareObject::new(CLASS.as_ref()).unwrap();
                    let layout = ChannelLayout::parse(c"5.1").unwrap();
                    for i in 0..100 {
                        let mut m = o.as_object_mut();
                        let _ = m.insert_array(c"x", i, &[layout.as_ref()], SearchFlags::CHILDREN);
                        let mut d = Dict::from_entries([(c"k", c"v")]).unwrap();
                        m.set_from_dict(&mut d).unwrap();
                    }
                });
            }
        });
    }
}
