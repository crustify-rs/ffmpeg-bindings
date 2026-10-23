//! UB tests for `libavutil/opt.h`'s safe public API.

/// `OptionType`: an open value type, whose every operation is defined for
/// every `c_uint`, crossing threads by value.
mod option_type {
    use ffmpeg_bindings::libavutil::libavutil::opt::OptionType;

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
            let raw: ffmpeg_bindings::libavutil::libavutil::ffi::AVOptionType = ty.into();
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

    use ffmpeg_bindings::libavutil::libavutil::opt::{InvalidSeparator, OptionArrayDef, OptionArrayDefRef};

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

    use ffmpeg_bindings::libavutil::libavutil::log::{Class, ClassCategory};
    use ffmpeg_bindings::libavutil::libavutil::opt::{BareObject, SearchFlags, SerializeFlags, parse_key_value};

    static CLASS: Class = Class::new(c"crustify-opt-ub", ClassCategory::NA);

    const NOT_FOUND: i32 = -i32::from_le_bytes([0xF8, b'O', b'P', b'T']);
    const EINVAL: i32 = -22;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

    use ffmpeg_bindings::libavutil::libavutil::channel_layout::ChannelLayoutVal;
    use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
    use ffmpeg_bindings::libavutil::libavutil::log::{Class, ClassCategory};
    use ffmpeg_bindings::libavutil::libavutil::mem::HeapString;
    use ffmpeg_bindings::libavutil::libavutil::opt::{BareObject, ObjectRef, PotentialChildren, SearchFlags};
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;
    use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;

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
    #[cfg_attr(miri, ignore = "calls C")]
    fn a_bare_object_reads_nothing() {
        let mut a = BareObject::new(CLASS.as_ref()).unwrap();
        for flags in [SearchFlags::NONE, SearchFlags::CHILDREN] {
            every_getter(a.as_object(), flags);
            every_getter(a.as_object_mut().as_ref(), flags);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    use ffmpeg_bindings::libavutil::libavutil::channel_layout::ChannelLayout;
    use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
    use ffmpeg_bindings::libavutil::libavutil::log::{Class, ClassCategory};
    use ffmpeg_bindings::libavutil::libavutil::opt::{BareObject, SearchFlags};
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;
    use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;

    static CLASS: Class = Class::new(c"crustify-opt-typed-ub", ClassCategory::NA);

    const NOT_FOUND: i32 = -i32::from_le_bytes([0xF8, b'O', b'P', b'T']);

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

/// Options wave 2, batch 0: lookups, the option walk and range queries on
/// the object and class built without `unsafe` (a [`BareObject`] of a
/// [`Class::new`] class), which find nothing and fail every query without a
/// list; and on `cpuflags`, the C class `av_parse_cpu_caps` evaluates flags
/// against, which a log callback reaches through `Record::class` while C's
/// failed parse logs in that class's context: its lookups, range queries
/// and evaluators run inside the callback, which their own failures
/// re-enter, and the flags agree with `cpu::parse_cpu_caps`.
///
/// [`BareObject`]: ffmpeg_bindings::libavutil::libavutil::opt::BareObject
/// [`Class::new`]: ffmpeg_bindings::libavutil::libavutil::log::Class::new
mod lookups {
    use core::ffi::CStr;
    use std::cell::{Cell, RefCell};

    use ffmpeg_bindings::libavutil::libavutil::cpu;
    use ffmpeg_bindings::libavutil::libavutil::log::{self, Callback, Class, ClassCategory, ClassRef, Record};
    use ffmpeg_bindings::libavutil::libavutil::opt::{BareObject, EvalContext, RangeFlags, SearchFlags};

    use crate::log::{LogState, isolated};

    static CLASS: Class = Class::new(c"crustify-opt-lookups", ClassCategory::NA);

    const EINVAL: i32 = -22;
    const ENOMEM: i32 = -12;
    const ENOSYS: i32 = -38;

    fn range_flags() -> [RangeFlags; 4] {
        [
            RangeFlags::NONE,
            RangeFlags::CHILDREN,
            RangeFlags::MULTI_COMPONENT,
            RangeFlags::CHILDREN | RangeFlags::MULTI_COMPONENT,
        ]
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn bare_objects_and_classes_find_nothing() {
        let bare = BareObject::new(CLASS.as_ref()).unwrap();
        let obj = bare.as_object();
        let class = CLASS.as_ref();
        for name in [c"", c"x", c"flags", c"crustify-opt-lookups"] {
            for unit in [None, Some(c"flags"), Some(c"")] {
                for opt_flags in [0, 1, -1] {
                    for flags in [SearchFlags::NONE, SearchFlags::CHILDREN] {
                        assert!(obj.find(name, unit, opt_flags, flags).is_none());
                        assert!(obj.find_with_target(name, unit, opt_flags, flags).is_none());
                        assert!(class.find(name, unit, opt_flags, flags).is_none());
                    }
                }
            }
            for flags in range_flags() {
                assert_eq!(obj.option_ranges(name, flags).err(), Some(ENOMEM));
                assert_eq!(obj.default_option_ranges(name, flags).err(), Some(ENOMEM));
                assert_eq!(class.option_ranges(name, flags).err(), Some(ENOMEM));
                assert_eq!(class.default_option_ranges(name, flags).err(), Some(ENOMEM));
            }
        }
        assert_eq!(obj.options().count(), 0);
        let mut walk = obj.options();
        assert!(walk.next().is_none() && walk.next().is_none());
        assert!(format!("{walk:?}").contains("crustify-opt-lookups"));
    }

    /// Flag strings for the `cpuflags` class: constants, signs, numbers,
    /// errors, and operands outside the `int64_t` range, which both sides
    /// refuse.
    const TOKENS: [&CStr; 14] = [
        c"sse2",
        c"+sse2+avx",
        c"-sse2",
        c"mmx-mmx",
        c"nosuch",
        c"+1e300",
        c"0x10",
        c"",
        c"all",
        c"none",
        c"+max",
        c"sse2+",
        c"-min",
        c"pentium2-cmov",
    ];

    /// What the probe saw in the `cpuflags` class: its lookups, ranges and
    /// each token's evaluation from 0.
    #[derive(Debug, Default)]
    struct Seen {
        found: Vec<bool>,
        ranges: Vec<Option<i32>>,
        flags: Vec<(Option<i32>, i32)>,
        others: Vec<Option<i32>>,
    }

    thread_local! {
        /// Set while this thread probes the next `cpuflags` message.
        static ARMED: Cell<bool> = const { Cell::new(false) };
        static SEEN: RefCell<Option<Seen>> = const { RefCell::new(None) };
    }

    /// Probes the class of the first `cpuflags` message an armed thread
    /// logs; the messages its own evaluations log re-enter it disarmed.
    struct Probe;

    impl Callback for Probe {
        fn log(record: Record<'_>) {
            if !ARMED.with(Cell::get) {
                return;
            }
            let Some(class) = record.class() else { return };
            if class.name() != Some(c"cpuflags") {
                return;
            }
            ARMED.with(|a| a.set(false));
            let seen = probe(class);
            SEEN.with(|s| *s.borrow_mut() = Some(seen));
        }
    }

    fn probe(class: ClassRef<'_>) -> Seen {
        let mut seen = Seen::default();
        let flags = class.find(c"flags", None, 0, SearchFlags::NONE).unwrap();
        for (name, unit) in [
            (c"sse2", Some(c"flags")),
            (c"sse2", None),
            (c"flags", None),
            (c"flags", Some(c"flags")),
            (c"nosuch", Some(c"flags")),
        ] {
            for search in [SearchFlags::NONE, SearchFlags::CHILDREN] {
                seen.found.push(class.find(name, unit, 0, search).is_some());
            }
        }
        for key in [c"flags", c"sse2", c"nosuch"] {
            seen.ranges
                .push(class.option_ranges(key, RangeFlags::MULTI_COMPONENT).err());
            seen.ranges
                .push(class.default_option_ranges(key, RangeFlags::NONE).err());
        }
        let ctx = EvalContext::from(class);
        for token in TOKENS {
            let mut f = 0;
            let r = flags.eval_flags(ctx, token, &mut f).err();
            seen.flags.push((r, f));
            seen.others.push(flags.eval_int(ctx, token).err());
            seen.others.push(flags.eval_rational(ctx, token).err());
        }
        seen
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn cpuflags_class_lookups_and_evaluations_agree_with_parse_cpu_caps() {
        if !isolated(
            "opt::lookups::cpuflags_class_lookups_and_evaluations_agree_with_parse_cpu_caps",
        ) {
            return;
        }
        let _s = LogState::lock();
        ARMED.with(|a| a.set(true));
        log::set_callback::<Probe>();
        let mut f = 0;
        assert!(cpu::parse_cpu_caps(&mut f, c"no-such-flag").is_err());
        log::set_default_callback();
        ARMED.with(|a| a.set(false));
        let seen = SEEN.with(|s| s.borrow_mut().take()).expect("the probe ran");
        let found = [
            true, true, false, false, true, true, false, false, false, false,
        ];
        assert_eq!(seen.found, found);
        // A flags option and a constant have no ranges; an unknown key is
        // C's `AVERROR(ENOMEM)`.
        let ranges = [ENOSYS, ENOSYS, ENOMEM, ENOMEM, ENOMEM, ENOMEM];
        assert_eq!(seen.ranges, ranges.map(Some));
        assert!(seen.others.iter().all(|&r| r == Some(EINVAL)));
        for (token, (r, flags)) in TOKENS.iter().zip(seen.flags) {
            let mut caps = 0u32;
            let c = cpu::parse_cpu_caps(&mut caps, token).err();
            assert_eq!((r, flags as u32), (c, caps), "{token:?}");
        }
    }
}

/// Flags values longer than C's 255-byte token buffer, evaluated in C's
/// `cpuflags` class: C parses each cut piece as an expression of its own, so
/// a piece that starts inside what the whole value reads as one number
/// (`0...0ld(1e300)`) must be refused on its own (options wave-2 review,
/// `ub_safe/eval-flags-token-split-hides-evaluator-call`).
mod flag_token_split {
    use std::cell::{Cell, RefCell};
    use std::ffi::CString;

    use ffmpeg_bindings::libavutil::libavutil::cpu;
    use ffmpeg_bindings::libavutil::libavutil::log::{self, Callback, ClassRef, Record};
    use ffmpeg_bindings::libavutil::libavutil::opt::SearchFlags;

    use crate::log::{LogState, isolated};

    const EINVAL: i32 = -22;
    /// `AV_CPU_FLAG_MMX` and `AV_CPU_FLAG_SSE2`.
    const MMX: i32 = 0x1;
    const SSE2: i32 = 0x10;

    /// An optional `+`, then `0*` and 253 zeros (one 255-byte token that
    /// evaluates to 0), then `tail`, which C evaluates as the next token.
    fn split(cmd: bool, tail: &str) -> CString {
        let mut v = if cmd { b"+0*".to_vec() } else { b"0*".to_vec() };
        v.extend([b'0'; 253]);
        v.extend(tail.as_bytes());
        CString::new(v).unwrap()
    }

    /// Each value's result and flags, from `MMX`.
    type Seen = Vec<(Result<(), i32>, i32)>;

    thread_local! {
        static ARMED: Cell<bool> = const { Cell::new(false) };
        static SEEN: RefCell<Seen> = const { RefCell::new(Vec::new()) };
    }

    /// Evaluates the values in the class of the first `cpuflags` message an
    /// armed thread logs.
    struct Probe;

    impl Callback for Probe {
        fn log(record: Record<'_>) {
            if !ARMED.with(Cell::get) {
                return;
            }
            let Some(class) = record.class() else { return };
            if class.name() != Some(c"cpuflags") {
                return;
            }
            ARMED.with(|a| a.set(false));
            let seen = probe(class);
            SEEN.with(|s| *s.borrow_mut() = seen);
        }
    }

    fn probe(class: ClassRef<'_>) -> Seen {
        let flags = class.find(c"flags", None, 0, SearchFlags::NONE).unwrap();
        [
            split(false, "ld(1e300)"),
            split(true, "ld(1e300)"),
            split(false, "print(1,1e300)"),
            split(false, "1"),
            split(false, "ld(1)"),
            split(true, "+sse2"),
        ]
        .iter()
        .map(|v| {
            let mut f = MMX;
            (flags.eval_flags(class, v, &mut f), f)
        })
        .collect()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn cut_tokens_are_refused_on_their_own() {
        if !isolated("opt::flag_token_split::cut_tokens_are_refused_on_their_own") {
            return;
        }
        let _s = LogState::lock();
        ARMED.with(|a| a.set(true));
        log::set_callback::<Probe>();
        let mut f = 0;
        assert!(cpu::parse_cpu_caps(&mut f, c"no-such-flag").is_err());
        log::set_default_callback();
        ARMED.with(|a| a.set(false));
        let seen = SEEN.with(RefCell::take);
        // Refused before C runs: the flags are untouched.
        let refused = (Err(EINVAL), MMX);
        // The second token replaces what the first wrote, or adds to it.
        let accepted = [(Ok(()), 1), (Ok(()), 0), (Ok(()), MMX | SSE2)];
        assert_eq!(seen[..3], [refused; 3]);
        assert_eq!(seen[3..], accepted);
    }
}
