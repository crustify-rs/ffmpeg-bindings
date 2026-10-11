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
