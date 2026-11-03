# Re-plan of the landed items (planner run 5)

At the user's instruction, planner run 5 re-planned every module's waves, the items that had already landed included, as if no wave had run. Waves 0-45, the 23 translation and 23 review waves that ran, are kept as records. The `todo` waves 232-669 are dropped, and the re-plan is numbered from 672: wave 670 (the annotation pass's `bprint.rs` smoke batch) and wave 671 (the annotation pass), which the dashboard records, already used 670 and 671, though `schedule.json` has no entry for them. Starting at 670 would have reused both indices, and with them the batch ids and the wave branch, `crustify/waves/wave-671`, that the task.json files of `reviewer/run-52` to `run-128` name. The new waves' `depends_on` name only waves from 672 on.

Each landed item's wrapper exists in the tree. The binder of its new batch finds it and replaces the raw references (`ffi::...`) that the wrappers of the dependencies wrapped before it now make obsolete. Nothing here is a defect of the landed waves: they ran in the order the v2 subsystems planned.

## Counts

- Landed: 600 items in 84 batches of 23 translation waves (0-45, even indices).
- Re-scheduled: 572 of them, in 128 batches of 100 new translation waves, across 59 modules. The whole re-plan has 606 waves (672-1277): 303 translation waves, 389 wrap batches, each wave followed by its review wave, and 1337 items: every non-deprecated public-API unit of the scope except enums.
- Not re-scheduled: 28 landed items, for which the current rules schedule no batch: 25 enums, `FF_PAD_STRUCTURE` and the two raw-lifetime units `void` and `string` (see [Landed items with no new batch](#landed-items-with-no-new-batch)).
- Wrapped late: 11 landed items have public-signature dependencies that the re-plan wraps before them but that were not wrapped when they landed. There are 5 such dependencies, wrapped in 4 batches (see [Dependencies wrapped late](#dependencies-wrapped-late)). Every other landed item's public-signature dependencies had landed in an earlier wave or in its own batch, or are enums, which keep their generated `-sys` definitions, or `FILE` and `struct tm`, which glibc-bindings serves.

## Dependencies wrapped late

A dependency is one of the item's direct edges in the campaign-wide public-signature graph: the types its signature or fields name, and the callbacks. It counts when the re-plan wraps it in an earlier wave than the item, and when it landed after the item's wave, or in another batch of the same wave, whose binder ran concurrently. Edges wavefront cuts to break a cycle (`fallback`) are excluded: the re-plan reaches them raw too, until they are back-filled.

| landed item | ran in | new batch | dependency | dependency ran in | dependency's new batch |
|---|---|---|---|---|---|
| `AVOption` | wave-4/batch-2 | wave-794/batch-0 | `AVOptionArrayDef` | wave-20/batch-1 | wave-792/batch-0 |
| `AVOption` | wave-4/batch-2 | wave-794/batch-0 | `AVRational` | wave-4/batch-3 | wave-790/batch-0 |
| `AVOptionRanges` | wave-4/batch-2 | wave-794/batch-0 | `AVOptionRange` | wave-20/batch-2 | wave-792/batch-0 |
| `av_default_item_name` | wave-4/batch-10 | wave-800/batch-0 | `AVClass` | wave-6/batch-0 | wave-798/batch-0 |
| `av_log` | wave-4/batch-12 | wave-800/batch-0 | `AVClass` | wave-6/batch-0 | wave-798/batch-0 |
| `av_log_default_callback` | wave-4/batch-12 | wave-800/batch-0 | `AVClass` | wave-6/batch-0 | wave-798/batch-0 |
| `av_log_format_line` | wave-4/batch-12 | wave-800/batch-0 | `AVClass` | wave-6/batch-0 | wave-798/batch-0 |
| `av_log_format_line2` | wave-4/batch-12 | wave-800/batch-0 | `AVClass` | wave-6/batch-0 | wave-798/batch-0 |
| `av_log_once` | wave-4/batch-12 | wave-800/batch-0 | `AVClass` | wave-6/batch-0 | wave-798/batch-0 |
| `av_vlog` | wave-4/batch-14 | wave-800/batch-0 | `AVClass` | wave-6/batch-0 | wave-798/batch-0 |
| `av_default_get_category` | wave-6/batch-2 | wave-800/batch-0 | `AVClass` | wave-6/batch-0 | wave-798/batch-0 |
| `AVRegionOfInterest` | wave-32/batch-0 | wave-902/batch-1 | `AVFrameSideData` | wave-34/batch-0 | wave-900/batch-0 |

By dependency:

- `AVOptionArrayDef` (libavutil/opt.h), wrapped in wave-792/batch-0 (libavutil/opt.c), landed in wave-20/batch-1: `AVOption`.
- `AVRational` (libavutil/rational.h), wrapped in wave-790/batch-0 (libavutil/rational.c), landed in wave-4/batch-3: `AVOption`.
- `AVOptionRange` (libavutil/opt.h), wrapped in wave-792/batch-0 (libavutil/opt.c), landed in wave-20/batch-2: `AVOptionRanges`.
- `AVClass` (libavutil/log.h), wrapped in wave-798/batch-0 (libavutil/log.c), landed in wave-6/batch-0: `av_default_get_category`, `av_default_item_name`, `av_log`, `av_log_default_callback`, `av_log_format_line`, `av_log_format_line2`, `av_log_once`, `av_vlog`.
- `AVFrameSideData` (libavutil/frame.h), wrapped in wave-900/batch-0 (libavutil/frame.c), landed in wave-34/batch-0: `AVRegionOfInterest`.

`AVRational` and `AVOption` landed in two batches of wave 4. `AVOptionArrayDef` and `AVOptionRange` landed in wave 20, after `AVOption` and `AVOptionRanges`. `AVClass` landed in wave 6, after the log functions of waves 4 and 6. `AVFrameSideData` landed in wave 34, after `AVRegionOfInterest` (wave 32). In the re-plan, opt.c's four types are a pre-wave of log.c, because log.c's cut dependency on opt.c (`libavutil/36` -> `libavutil/52`) is reached through safe wrappers, not raw bindings. `AVRational` joins them through their closure over the public-signature graph, because rational.c runs after log.c.

## Landed items with no new batch

| item | kind | ran in | why no batch |
|---|---|---|---|
| `void` | raw-lifetime | wave-0/batch-0 | a raw-lifetime unit: the void and string lifecycle primitives are units of the foundational modules (mem.c, refstruct.c, avstring.c (foundational)) |
| `string` | raw-lifetime | wave-2/batch-0 | a raw-lifetime unit: the void and string lifecycle primitives are units of the foundational modules (mem.c, refstruct.c, avstring.c (foundational)) |
| `AVClassCategory` | type | wave-4/batch-0 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVClassStateFlags` | type | wave-4/batch-0 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVEscapeMode` | type | wave-4/batch-1 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVMediaType` | type | wave-4/batch-1 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVPictureType` | type | wave-4/batch-3 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVRounding` | type | wave-4/batch-4 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `FF_PAD_STRUCTURE` | type | wave-4/batch-4 | a type-minting macro (bprint.h:48), no type: it pads AVBPrint, whose own unit is scheduled |
| `AVAlphaMode` | type | wave-10/batch-0 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVChannel` | type | wave-10/batch-0 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVChannelOrder` | type | wave-10/batch-1 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVChromaLocation` | type | wave-10/batch-1 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVColorPrimaries` | type | wave-10/batch-2 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVColorRange` | type | wave-10/batch-2 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVColorSpace` | type | wave-10/batch-3 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVColorTransferCharacteristic` | type | wave-10/batch-3 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVMatrixEncoding` | type | wave-10/batch-4 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVPixelFormat` | type | wave-10/batch-5 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVSampleFormat` | type | wave-10/batch-6 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVOptionType` | type | wave-20/batch-2 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVCRCId` | type | wave-26/batch-2 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVHMACType` | type | wave-26/batch-4 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVActiveFormatDescription` | type | wave-30/batch-0 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVFrameSideDataType` | type | wave-30/batch-2 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVSideDataProps` | type | wave-30/batch-4 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVHWDeviceType` | type | wave-40/batch-0 | an enum: no batch wraps an enum; its generated -sys definition is used directly |
| `AVHWFrameTransferDirection` | type | wave-40/batch-0 | an enum: no batch wraps an enum; its generated -sys definition is used directly |

## Every landed item

Grouped by the module that owns it, in the re-plan's run order. A landed wave held items of several modules, so the landed waves have `module: null`.

### libavutil/mem.c (foundational)

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_calloc` | symbol | wave-4/batch-10 | wave-672/batch-0 |
| `av_dynarray2_add` | symbol | wave-4/batch-11 | wave-672/batch-0 |
| `av_dynarray_add` | symbol | wave-4/batch-11 | wave-672/batch-0 |
| `av_dynarray_add_nofree` | symbol | wave-4/batch-11 | wave-672/batch-0 |
| `av_fast_malloc` | symbol | wave-4/batch-11 | wave-672/batch-0 |
| `av_fast_mallocz` | symbol | wave-4/batch-11 | wave-672/batch-0 |
| `av_fast_realloc` | symbol | wave-4/batch-11 | wave-672/batch-0 |
| `av_free` | symbol | wave-4/batch-11 | wave-672/batch-0 |
| `av_freep` | symbol | wave-4/batch-11 | wave-672/batch-0 |
| `av_malloc` | symbol | wave-4/batch-12 | wave-672/batch-0 |
| `av_malloc_array` | symbol | wave-4/batch-12 | wave-672/batch-0 |
| `av_mallocz` | symbol | wave-4/batch-12 | wave-672/batch-0 |
| `av_max_alloc` | symbol | wave-4/batch-12 | wave-672/batch-0 |
| `av_memcpy_backptr` | symbol | wave-4/batch-12 | wave-672/batch-0 |
| `av_memdup` | symbol | wave-4/batch-12 | wave-672/batch-0 |
| `av_realloc` | symbol | wave-4/batch-13 | wave-672/batch-0 |
| `av_realloc_array` | symbol | wave-4/batch-13 | wave-672/batch-0 |
| `av_realloc_f` | symbol | wave-4/batch-13 | wave-672/batch-0 |
| `av_reallocp` | symbol | wave-4/batch-13 | wave-672/batch-0 |
| `av_reallocp_array` | symbol | wave-4/batch-13 | wave-672/batch-0 |
| `av_size_mult` | symbol | wave-4/batch-13 | wave-672/batch-0 |
| `av_strdup` | symbol | wave-4/batch-13 | wave-672/batch-0 |
| `av_strndup` | symbol | wave-4/batch-14 | wave-672/batch-0 |

### libavutil/refstruct.c (foundational)

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVRefStructOpaque` | type | wave-30/batch-3 | wave-674/batch-0 |
| `AVRefStructPool` | type | wave-30/batch-3 | wave-674/batch-0 |
| `av_refstruct_allocz` | symbol | wave-30/batch-5 | wave-674/batch-1 |
| `av_refstruct_exclusive` | symbol | wave-30/batch-5 | wave-674/batch-1 |
| `av_refstruct_ref` | symbol | wave-30/batch-5 | wave-674/batch-1 |
| `av_refstruct_ref_c` | symbol | wave-30/batch-5 | wave-674/batch-1 |
| `av_refstruct_replace` | symbol | wave-30/batch-5 | wave-674/batch-1 |
| `av_refstruct_unref` | symbol | wave-30/batch-5 | wave-674/batch-1 |
| `av_refstruct_alloc_ext` | symbol | wave-32/batch-1 | wave-676/batch-0 |
| `av_refstruct_alloc_ext_c` | symbol | wave-32/batch-1 | wave-676/batch-0 |
| `av_refstruct_pool_alloc` | symbol | wave-32/batch-1 | wave-676/batch-0 |
| `av_refstruct_pool_alloc_ext` | symbol | wave-32/batch-1 | wave-676/batch-0 |
| `av_refstruct_pool_alloc_ext_c` | symbol | wave-32/batch-1 | wave-676/batch-0 |
| `av_refstruct_pool_get` | symbol | wave-32/batch-1 | wave-676/batch-0 |
| `av_refstruct_pool_uninit` | symbol | wave-32/batch-1 | wave-676/batch-0 |

### libavutil/avstring.c (foundational)

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_append_path_component` | symbol | wave-4/batch-10 | wave-678/batch-0 |
| `av_asprintf` | symbol | wave-4/batch-10 | wave-678/batch-0 |
| `av_get_token` | symbol | wave-4/batch-11 | wave-678/batch-0 |
| `av_strlcpy` | symbol | wave-4/batch-14 | wave-678/batch-0 |

### libavutil/adler32.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_adler32_update` | symbol | wave-26/batch-10 | wave-680/batch-0 |

### libavutil/aes.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVAES` | type | wave-26/batch-0 | wave-682/batch-0 |
| `av_aes_size` | symbol | wave-26/batch-10 | wave-682/batch-1 |
| `av_aes_alloc` | symbol | wave-28/batch-0 | wave-684/batch-0 |
| `av_aes_crypt` | symbol | wave-28/batch-0 | wave-684/batch-0 |
| `av_aes_init` | symbol | wave-28/batch-0 | wave-684/batch-0 |

### libavutil/aes_ctr.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVAESCTR` | type | wave-26/batch-0 | wave-686/batch-0 |
| `av_aes_ctr_alloc` | symbol | wave-28/batch-0 | wave-688/batch-0 |
| `av_aes_ctr_crypt` | symbol | wave-28/batch-0 | wave-688/batch-0 |
| `av_aes_ctr_free` | symbol | wave-28/batch-0 | wave-688/batch-0 |
| `av_aes_ctr_get_iv` | symbol | wave-28/batch-0 | wave-688/batch-0 |
| `av_aes_ctr_increment_iv` | symbol | wave-28/batch-0 | wave-688/batch-0 |
| `av_aes_ctr_init` | symbol | wave-28/batch-0 | wave-688/batch-0 |
| `av_aes_ctr_set_full_iv` | symbol | wave-28/batch-0 | wave-688/batch-0 |
| `av_aes_ctr_set_iv` | symbol | wave-28/batch-0 | wave-688/batch-0 |
| `av_aes_ctr_set_random_iv` | symbol | wave-28/batch-0 | wave-688/batch-0 |

### libavutil/avsscanf.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_sscanf` | symbol | wave-4/batch-13 | wave-690/batch-0 |

### libavutil/avstring.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_basename` | symbol | wave-4/batch-10 | wave-692/batch-0 |
| `av_dirname` | symbol | wave-4/batch-11 | wave-692/batch-0 |
| `av_escape` | symbol | wave-6/batch-2 | wave-692/batch-0 |
| `av_isdigit` | symbol | wave-4/batch-11 | wave-692/batch-0 |
| `av_isgraph` | symbol | wave-4/batch-12 | wave-692/batch-0 |
| `av_isspace` | symbol | wave-4/batch-12 | wave-692/batch-0 |
| `av_isxdigit` | symbol | wave-4/batch-12 | wave-692/batch-0 |
| `av_match_list` | symbol | wave-4/batch-12 | wave-692/batch-0 |
| `av_match_name` | symbol | wave-4/batch-12 | wave-692/batch-0 |
| `av_strcasecmp` | symbol | wave-4/batch-13 | wave-692/batch-0 |
| `av_strireplace` | symbol | wave-4/batch-13 | wave-692/batch-0 |
| `av_stristart` | symbol | wave-4/batch-13 | wave-692/batch-0 |
| `av_stristr` | symbol | wave-4/batch-13 | wave-692/batch-0 |
| `av_strlcat` | symbol | wave-4/batch-14 | wave-692/batch-0 |
| `av_strlcatf` | symbol | wave-4/batch-14 | wave-692/batch-0 |
| `av_strncasecmp` | symbol | wave-4/batch-14 | wave-692/batch-0 |
| `av_strnlen` | symbol | wave-4/batch-14 | wave-692/batch-0 |
| `av_strnstr` | symbol | wave-4/batch-14 | wave-692/batch-0 |
| `av_strstart` | symbol | wave-4/batch-14 | wave-692/batch-0 |
| `av_strtok` | symbol | wave-4/batch-14 | wave-692/batch-0 |
| `av_tolower` | symbol | wave-4/batch-14 | wave-692/batch-0 |
| `av_toupper` | symbol | wave-4/batch-14 | wave-692/batch-0 |
| `av_utf8_decode` | symbol | wave-4/batch-14 | wave-692/batch-0 |

### libavutil/avutil.h

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_x_if_null` | symbol | wave-4/batch-14 | wave-694/batch-0 |

### libavutil/base64.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_base64_decode` | symbol | wave-26/batch-10 | wave-696/batch-0 |
| `av_base64_encode` | symbol | wave-26/batch-10 | wave-696/batch-0 |

### libavutil/blowfish.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVBlowfish` | type | wave-26/batch-1 | wave-698/batch-0 |
| `av_blowfish_alloc` | symbol | wave-28/batch-0 | wave-700/batch-0 |
| `av_blowfish_crypt` | symbol | wave-28/batch-0 | wave-700/batch-0 |
| `av_blowfish_crypt_ecb` | symbol | wave-28/batch-0 | wave-700/batch-0 |
| `av_blowfish_init` | symbol | wave-28/batch-0 | wave-700/batch-0 |

### libavutil/bprint.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVBPrint` | type | wave-6/batch-0 | wave-702/batch-0 |
| `ff_pad_helper_AVBPrint` | type | wave-6/batch-1 | wave-702/batch-0 |
| `av_bprint_append_data` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_bprint_chars` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_bprint_clear` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_bprint_escape` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_bprint_finalize` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_bprint_get_buffer` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_bprint_init` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_bprint_init_for_buffer` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_bprint_is_complete` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_bprint_strftime` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_bprintf` | symbol | wave-8/batch-0 | wave-704/batch-0 |
| `av_vbprintf` | symbol | wave-8/batch-0 | wave-704/batch-0 |

### libavutil/bswap.h

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_bswap16` | symbol | wave-4/batch-10 | wave-706/batch-0 |
| `av_bswap32` | symbol | wave-4/batch-10 | wave-706/batch-0 |
| `av_bswap64` | symbol | wave-4/batch-10 | wave-706/batch-0 |

### libavutil/buffer.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVBuffer` | type | wave-30/batch-1 | wave-708/batch-0 |
| `AVBufferPool` | type | wave-30/batch-1 | wave-708/batch-0 |
| `av_buffer_default_free` | symbol | wave-30/batch-5 | wave-708/batch-1 |
| `AVBufferRef` | type | wave-32/batch-0 | wave-710/batch-0 |
| `av_buffer_pool_uninit` | symbol | wave-32/batch-1 | wave-710/batch-1 |
| `av_buffer_alloc` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_allocz` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_create` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_get_opaque` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_get_ref_count` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_is_writable` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_make_writable` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_pool_buffer_get_opaque` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_pool_get` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_pool_init` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_pool_init2` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_realloc` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_ref` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_replace` | symbol | wave-34/batch-1 | wave-712/batch-0 |
| `av_buffer_unref` | symbol | wave-34/batch-1 | wave-712/batch-0 |

### libavutil/camellia.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVCAMELLIA` | type | wave-26/batch-1 | wave-714/batch-0 |
| `av_camellia_size` | symbol | wave-26/batch-10 | wave-714/batch-1 |
| `av_camellia_alloc` | symbol | wave-28/batch-0 | wave-716/batch-0 |
| `av_camellia_crypt` | symbol | wave-28/batch-0 | wave-716/batch-0 |
| `av_camellia_init` | symbol | wave-28/batch-0 | wave-716/batch-0 |

### libavutil/cast5.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVCAST5` | type | wave-26/batch-2 | wave-718/batch-0 |
| `av_cast5_size` | symbol | wave-26/batch-10 | wave-718/batch-1 |
| `av_cast5_alloc` | symbol | wave-28/batch-0 | wave-720/batch-0 |
| `av_cast5_crypt` | symbol | wave-28/batch-0 | wave-720/batch-0 |
| `av_cast5_crypt2` | symbol | wave-28/batch-0 | wave-720/batch-0 |
| `av_cast5_init` | symbol | wave-28/batch-0 | wave-720/batch-0 |

### libavutil/channel_layout.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVChannelCustom` | type | wave-12/batch-1 | wave-722/batch-0 |
| `av_channel_description` | symbol | wave-12/batch-3 | wave-722/batch-1 |
| `av_channel_description_bprint` | symbol | wave-14/batch-1 | wave-722/batch-1 |
| `av_channel_from_string` | symbol | wave-12/batch-3 | wave-722/batch-1 |
| `av_channel_name` | symbol | wave-12/batch-3 | wave-722/batch-1 |
| `av_channel_name_bprint` | symbol | wave-14/batch-1 | wave-722/batch-1 |
| `AVChannelLayout` | type | wave-14/batch-0 | wave-724/batch-0 |
| `av_channel_layout_ambisonic_order` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_channel_from_index` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_channel_from_string` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_check` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_compare` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_copy` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_custom_init` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_default` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_describe` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_describe_bprint` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_from_mask` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_from_string` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_index_from_channel` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_index_from_string` | symbol | wave-16/batch-1 | wave-726/batch-0 |
| `av_channel_layout_retype` | symbol | wave-16/batch-2 | wave-726/batch-1 |
| `av_channel_layout_standard` | symbol | wave-16/batch-2 | wave-726/batch-1 |
| `av_channel_layout_subset` | symbol | wave-16/batch-2 | wave-726/batch-1 |
| `av_channel_layout_uninit` | symbol | wave-16/batch-2 | wave-726/batch-1 |

### libavutil/common.h

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_ceil_log2_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clip64_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clip_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clip_int16_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clip_int8_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clip_intp2_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clip_uint16_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clip_uint8_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clip_uintp2_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clipd_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clipf_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_clipl_int32_c` | symbol | wave-4/batch-10 | wave-728/batch-0 |
| `av_parity_c` | symbol | wave-4/batch-12 | wave-728/batch-0 |
| `av_popcount64_c` | symbol | wave-4/batch-13 | wave-728/batch-0 |
| `av_popcount_c` | symbol | wave-4/batch-13 | wave-728/batch-0 |
| `av_sat_add32_c` | symbol | wave-4/batch-13 | wave-728/batch-0 |
| `av_sat_add64_c` | symbol | wave-4/batch-13 | wave-728/batch-0 |
| `av_sat_dadd32_c` | symbol | wave-4/batch-13 | wave-728/batch-0 |
| `av_sat_dsub32_c` | symbol | wave-4/batch-13 | wave-728/batch-0 |
| `av_sat_sub32_c` | symbol | wave-4/batch-13 | wave-728/batch-0 |
| `av_sat_sub64_c` | symbol | wave-4/batch-13 | wave-728/batch-0 |
| `av_zero_extend_c` | symbol | wave-4/batch-14 | wave-728/batch-0 |

### libavutil/cpu.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_cpu_count` | symbol | wave-4/batch-10 | wave-734/batch-0 |
| `av_cpu_force_count` | symbol | wave-4/batch-10 | wave-734/batch-0 |
| `av_cpu_max_align` | symbol | wave-4/batch-10 | wave-734/batch-0 |
| `av_force_cpu_flags` | symbol | wave-4/batch-11 | wave-734/batch-0 |
| `av_get_cpu_flags` | symbol | wave-4/batch-11 | wave-734/batch-0 |
| `av_parse_cpu_caps` | symbol | wave-4/batch-13 | wave-734/batch-0 |

### libavutil/crc.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_crc` | symbol | wave-26/batch-10 | wave-736/batch-0 |
| `av_crc_get_table` | symbol | wave-28/batch-0 | wave-736/batch-0 |
| `av_crc_init` | symbol | wave-26/batch-10 | wave-736/batch-0 |

### libavutil/des.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVDES` | type | wave-26/batch-3 | wave-738/batch-0 |
| `av_des_alloc` | symbol | wave-28/batch-0 | wave-740/batch-0 |
| `av_des_crypt` | symbol | wave-28/batch-1 | wave-740/batch-0 |
| `av_des_init` | symbol | wave-28/batch-1 | wave-740/batch-0 |
| `av_des_mac` | symbol | wave-28/batch-1 | wave-740/batch-0 |

### libavutil/dict.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVDictionary` | type | wave-20/batch-0 | wave-742/batch-0 |
| `AVDictionaryEntry` | type | wave-20/batch-0 | wave-742/batch-0 |
| `av_dict_copy` | symbol | wave-22/batch-0 | wave-744/batch-0 |
| `av_dict_count` | symbol | wave-22/batch-0 | wave-744/batch-0 |
| `av_dict_free` | symbol | wave-22/batch-0 | wave-744/batch-0 |
| `av_dict_get` | symbol | wave-22/batch-0 | wave-744/batch-0 |
| `av_dict_get_string` | symbol | wave-22/batch-0 | wave-744/batch-0 |
| `av_dict_iterate` | symbol | wave-22/batch-0 | wave-744/batch-0 |
| `av_dict_parse_string` | symbol | wave-22/batch-0 | wave-744/batch-0 |
| `av_dict_set` | symbol | wave-22/batch-0 | wave-744/batch-0 |
| `av_dict_set_int` | symbol | wave-22/batch-0 | wave-744/batch-0 |

### libavutil/error.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_make_error_string` | symbol | wave-4/batch-12 | wave-754/batch-0 |
| `av_strerror` | symbol | wave-4/batch-13 | wave-754/batch-0 |

### libavutil/eval.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVExpr` | type | wave-20/batch-1 | wave-756/batch-0 |
| `av_expr_parse_and_eval` | symbol | wave-20/batch-3 | wave-756/batch-1 |
| `av_strtod` | symbol | wave-20/batch-4 | wave-756/batch-1 |
| `av_expr_count_func` | symbol | wave-22/batch-0 | wave-758/batch-0 |
| `av_expr_count_vars` | symbol | wave-22/batch-0 | wave-758/batch-0 |
| `av_expr_eval` | symbol | wave-22/batch-0 | wave-758/batch-0 |
| `av_expr_free` | symbol | wave-22/batch-0 | wave-758/batch-0 |
| `av_expr_parse` | symbol | wave-22/batch-0 | wave-758/batch-0 |

### libavutil/file.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_file_map` | symbol | wave-4/batch-11 | wave-770/batch-0 |
| `av_file_unmap` | symbol | wave-4/batch-11 | wave-770/batch-0 |

### libavutil/hash.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVHashContext` | type | wave-26/batch-4 | wave-772/batch-0 |
| `av_hash_names` | symbol | wave-26/batch-10 | wave-772/batch-1 |
| `av_hash_alloc` | symbol | wave-28/batch-1 | wave-774/batch-0 |
| `av_hash_final` | symbol | wave-28/batch-1 | wave-774/batch-0 |
| `av_hash_final_b64` | symbol | wave-28/batch-1 | wave-774/batch-0 |
| `av_hash_final_bin` | symbol | wave-28/batch-1 | wave-774/batch-0 |
| `av_hash_final_hex` | symbol | wave-28/batch-1 | wave-774/batch-0 |
| `av_hash_freep` | symbol | wave-28/batch-1 | wave-774/batch-0 |
| `av_hash_get_name` | symbol | wave-28/batch-1 | wave-774/batch-0 |
| `av_hash_get_size` | symbol | wave-28/batch-1 | wave-774/batch-0 |
| `av_hash_init` | symbol | wave-28/batch-1 | wave-774/batch-0 |
| `av_hash_update` | symbol | wave-28/batch-1 | wave-774/batch-0 |

### libavutil/hmac.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVHMAC` | type | wave-26/batch-3 | wave-776/batch-0 |
| `av_hmac_alloc` | symbol | wave-28/batch-1 | wave-778/batch-0 |
| `av_hmac_calc` | symbol | wave-28/batch-1 | wave-778/batch-0 |
| `av_hmac_final` | symbol | wave-28/batch-1 | wave-778/batch-0 |
| `av_hmac_free` | symbol | wave-28/batch-1 | wave-778/batch-0 |
| `av_hmac_init` | symbol | wave-28/batch-1 | wave-778/batch-0 |
| `av_hmac_update` | symbol | wave-28/batch-1 | wave-778/batch-0 |

### libavutil/intfloat.h

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_intfloat32` | type | wave-4/batch-6 | wave-780/batch-0 |
| `av_intfloat64` | type | wave-4/batch-7 | wave-780/batch-0 |
| `av_double2int` | symbol | wave-4/batch-11 | wave-780/batch-1 |
| `av_float2int` | symbol | wave-4/batch-11 | wave-780/batch-1 |
| `av_int2double` | symbol | wave-4/batch-11 | wave-780/batch-1 |
| `av_int2float` | symbol | wave-4/batch-11 | wave-780/batch-1 |

### libavutil/intmath.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_log2` | symbol | wave-4/batch-12 | wave-782/batch-0 |
| `av_log2_16bit` | symbol | wave-4/batch-12 | wave-782/batch-0 |

### libavutil/intreadwrite.h

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_alias16` | type | wave-4/batch-5 | wave-784/batch-0 |
| `av_alias32` | type | wave-4/batch-5 | wave-784/batch-0 |
| `av_alias64` | type | wave-4/batch-6 | wave-784/batch-1 |
| `unaligned_16` | type | wave-4/batch-8 | wave-784/batch-1 |
| `unaligned_32` | type | wave-4/batch-8 | wave-784/batch-2 |
| `unaligned_64` | type | wave-4/batch-9 | wave-784/batch-2 |

### libavutil/rational.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVRational` | type | wave-4/batch-3 | wave-790/batch-0 |
| `av_add_q` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_cmp_q` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_d2q` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_div_q` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_find_nearest_q_idx` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_gcd_q` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_inv_q` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_make_q` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_mul_q` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_nearer_q` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_q2d` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_q2intfloat` | symbol | wave-6/batch-2 | wave-822/batch-0 |
| `av_reduce` | symbol | wave-4/batch-13 | wave-822/batch-0 |
| `av_sub_q` | symbol | wave-6/batch-2 | wave-822/batch-0 |

### libavutil/opt.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVOptionArrayDef` | type | wave-20/batch-1 | wave-792/batch-0 |
| `AVOptionRange` | type | wave-20/batch-2 | wave-792/batch-0 |
| `AVOption` | type | wave-4/batch-2 | wave-794/batch-0 |
| `AVOptionRanges` | type | wave-4/batch-2 | wave-794/batch-0 |
| `av_opt_get_key_value` | symbol | wave-20/batch-3 | wave-854/batch-0 |
| `av_opt_child_class_iterate` | symbol | wave-22/batch-0 | wave-856/batch-0 |
| `av_opt_child_next` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_copy` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_flag_is_set` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_free` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_get` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_get_array` | symbol | wave-22/batch-0 | wave-856/batch-0 |
| `av_opt_get_array_size` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_get_chlayout` | symbol | wave-22/batch-0 | wave-856/batch-0 |
| `av_opt_get_dict_val` | symbol | wave-22/batch-0 | wave-856/batch-0 |
| `av_opt_get_double` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_get_image_size` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_get_int` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_get_pixel_fmt` | symbol | wave-22/batch-0 | wave-856/batch-0 |
| `av_opt_get_q` | symbol | wave-22/batch-0 | wave-856/batch-0 |
| `av_opt_get_sample_fmt` | symbol | wave-22/batch-0 | wave-856/batch-0 |
| `av_opt_get_video_rate` | symbol | wave-22/batch-0 | wave-856/batch-0 |
| `av_opt_is_set_to_default_by_name` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_serialize` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_set` | symbol | wave-20/batch-3 | wave-856/batch-0 |
| `av_opt_set_array` | symbol | wave-22/batch-1 | wave-856/batch-1 |
| `av_opt_set_bin` | symbol | wave-20/batch-3 | wave-856/batch-1 |
| `av_opt_set_chlayout` | symbol | wave-22/batch-1 | wave-856/batch-1 |
| `av_opt_set_defaults` | symbol | wave-20/batch-3 | wave-856/batch-1 |
| `av_opt_set_defaults2` | symbol | wave-20/batch-3 | wave-856/batch-1 |
| `av_opt_set_dict` | symbol | wave-22/batch-1 | wave-856/batch-1 |
| `av_opt_set_dict2` | symbol | wave-22/batch-1 | wave-856/batch-1 |
| `av_opt_set_dict_val` | symbol | wave-22/batch-1 | wave-856/batch-1 |
| `av_opt_set_double` | symbol | wave-20/batch-3 | wave-856/batch-1 |
| `av_opt_set_from_string` | symbol | wave-20/batch-3 | wave-856/batch-1 |
| `av_opt_set_image_size` | symbol | wave-20/batch-3 | wave-856/batch-1 |
| `av_opt_set_int` | symbol | wave-20/batch-3 | wave-856/batch-1 |
| `av_opt_set_pixel_fmt` | symbol | wave-22/batch-1 | wave-856/batch-1 |
| `av_opt_set_q` | symbol | wave-22/batch-1 | wave-856/batch-1 |
| `av_opt_set_sample_fmt` | symbol | wave-22/batch-1 | wave-856/batch-1 |
| `av_opt_set_video_rate` | symbol | wave-22/batch-1 | wave-856/batch-1 |
| `av_opt_show2` | symbol | wave-20/batch-3 | wave-856/batch-1 |
| `av_set_options_string` | symbol | wave-20/batch-4 | wave-856/batch-1 |
| `av_opt_eval_double` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_eval_flags` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_eval_float` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_eval_int` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_eval_int64` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_eval_q` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_eval_uint` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_find` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_find2` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_freep_ranges` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_is_set_to_default` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_next` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_query_ranges` | symbol | wave-24/batch-0 | wave-858/batch-0 |
| `av_opt_query_ranges_default` | symbol | wave-24/batch-0 | wave-858/batch-0 |

### libavutil/log.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_log_get_flags` | symbol | wave-4/batch-12 | wave-796/batch-0 |
| `av_log_get_level` | symbol | wave-4/batch-12 | wave-796/batch-0 |
| `av_log_set_callback` | symbol | wave-4/batch-12 | wave-796/batch-0 |
| `av_log_set_flags` | symbol | wave-4/batch-12 | wave-796/batch-0 |
| `av_log_set_level` | symbol | wave-4/batch-12 | wave-796/batch-0 |
| `AVClass` | type | wave-6/batch-0 | wave-798/batch-0 |
| `av_default_get_category` | symbol | wave-6/batch-2 | wave-800/batch-0 |
| `av_default_item_name` | symbol | wave-4/batch-10 | wave-800/batch-0 |
| `av_log` | symbol | wave-4/batch-12 | wave-800/batch-0 |
| `av_log_default_callback` | symbol | wave-4/batch-12 | wave-800/batch-0 |
| `av_log_format_line` | symbol | wave-4/batch-12 | wave-800/batch-0 |
| `av_log_format_line2` | symbol | wave-4/batch-12 | wave-800/batch-0 |
| `av_log_once` | symbol | wave-4/batch-12 | wave-800/batch-0 |
| `av_vlog` | symbol | wave-4/batch-14 | wave-800/batch-0 |

### libavutil/lzo.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_lzo1x_decode` | symbol | wave-26/batch-10 | wave-802/batch-0 |

### libavutil/md5.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVMD5` | type | wave-26/batch-5 | wave-804/batch-0 |
| `av_md5_size` | symbol | wave-26/batch-10 | wave-804/batch-1 |
| `av_md5_sum` | symbol | wave-26/batch-10 | wave-804/batch-1 |
| `av_md5_alloc` | symbol | wave-28/batch-1 | wave-806/batch-0 |
| `av_md5_final` | symbol | wave-28/batch-1 | wave-806/batch-0 |
| `av_md5_init` | symbol | wave-28/batch-1 | wave-806/batch-0 |
| `av_md5_update` | symbol | wave-28/batch-1 | wave-806/batch-0 |

### libavutil/murmur3.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVMurMur3` | type | wave-26/batch-5 | wave-808/batch-0 |
| `av_murmur3_alloc` | symbol | wave-28/batch-1 | wave-810/batch-0 |
| `av_murmur3_final` | symbol | wave-28/batch-1 | wave-810/batch-0 |
| `av_murmur3_init` | symbol | wave-28/batch-2 | wave-810/batch-0 |
| `av_murmur3_init_seeded` | symbol | wave-28/batch-2 | wave-810/batch-0 |
| `av_murmur3_update` | symbol | wave-28/batch-2 | wave-810/batch-0 |

### libavutil/pixdesc.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVComponentDescriptor` | type | wave-10/batch-4 | wave-812/batch-0 |
| `av_chroma_location_from_name` | symbol | wave-10/batch-7 | wave-812/batch-1 |
| `av_color_primaries_from_name` | symbol | wave-10/batch-7 | wave-812/batch-1 |
| `av_color_range_from_name` | symbol | wave-10/batch-7 | wave-812/batch-1 |
| `av_color_space_from_name` | symbol | wave-10/batch-7 | wave-812/batch-1 |
| `av_color_transfer_from_name` | symbol | wave-10/batch-7 | wave-812/batch-1 |
| `AVPixFmtDescriptor` | type | wave-12/batch-2 | wave-814/batch-0 |
| `av_alpha_mode_from_name` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_alpha_mode_name` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_chroma_location_enum_to_pos` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_chroma_location_name` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_chroma_location_pos_to_enum` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_color_primaries_name` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_color_range_name` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_color_space_name` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_color_transfer_name` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_find_best_pix_fmt_of_2` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_get_pix_fmt` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_get_pix_fmt_loss` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_get_pix_fmt_name` | symbol | wave-12/batch-3 | wave-814/batch-1 |
| `av_get_pix_fmt_string` | symbol | wave-12/batch-4 | wave-814/batch-1 |
| `av_pix_fmt_count_planes` | symbol | wave-12/batch-4 | wave-814/batch-1 |
| `av_pix_fmt_get_chroma_sub_sample` | symbol | wave-12/batch-4 | wave-814/batch-1 |
| `av_pix_fmt_swap_endianness` | symbol | wave-12/batch-5 | wave-814/batch-1 |
| `av_get_bits_per_pixel` | symbol | wave-14/batch-1 | wave-816/batch-0 |
| `av_get_padded_bits_per_pixel` | symbol | wave-14/batch-1 | wave-816/batch-0 |
| `av_pix_fmt_desc_get` | symbol | wave-14/batch-1 | wave-816/batch-0 |
| `av_pix_fmt_desc_get_id` | symbol | wave-14/batch-1 | wave-816/batch-0 |
| `av_pix_fmt_desc_next` | symbol | wave-14/batch-1 | wave-816/batch-0 |
| `av_read_image_line` | symbol | wave-14/batch-1 | wave-816/batch-0 |
| `av_read_image_line2` | symbol | wave-14/batch-1 | wave-816/batch-0 |
| `av_write_image_line` | symbol | wave-14/batch-1 | wave-816/batch-0 |
| `av_write_image_line2` | symbol | wave-14/batch-1 | wave-816/batch-0 |

### libavutil/pixelutils.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_pixelutils_get_sad_fn` | symbol | wave-12/batch-5 | wave-818/batch-0 |
| `av_pixelutils_sad_fn` | callback | wave-10/batch-7 | wave-818/batch-0 |

### libavutil/random_seed.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_get_random_seed` | symbol | wave-4/batch-11 | wave-820/batch-0 |
| `av_random_bytes` | symbol | wave-4/batch-13 | wave-820/batch-0 |

### libavutil/csp.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_csp_eotf_function` | callback | wave-10/batch-7 | wave-824/batch-0 |
| `av_csp_trc_function` | callback | wave-10/batch-7 | wave-824/batch-0 |
| `AVCIExy` | type | wave-12/batch-0 | wave-826/batch-0 |
| `AVLumaCoefficients` | type | wave-12/batch-1 | wave-826/batch-0 |
| `av_csp_approximate_eotf_gamma` | symbol | wave-12/batch-3 | wave-826/batch-1 |
| `av_csp_approximate_trc_gamma` | symbol | wave-12/batch-3 | wave-826/batch-1 |
| `av_csp_itu_eotf` | symbol | wave-12/batch-3 | wave-826/batch-1 |
| `av_csp_itu_eotf_inv` | symbol | wave-12/batch-3 | wave-826/batch-1 |
| `av_csp_trc_func_from_id` | symbol | wave-12/batch-3 | wave-826/batch-1 |
| `av_csp_trc_func_inv_from_id` | symbol | wave-12/batch-3 | wave-826/batch-1 |
| `AVPrimaryCoefficients` | type | wave-14/batch-0 | wave-828/batch-0 |
| `av_csp_luma_coeffs_from_avcsp` | symbol | wave-14/batch-1 | wave-828/batch-1 |
| `AVColorPrimariesDesc` | type | wave-16/batch-0 | wave-830/batch-0 |
| `av_csp_primaries_desc_from_id` | symbol | wave-18/batch-0 | wave-832/batch-0 |
| `av_csp_primaries_id_from_desc` | symbol | wave-18/batch-0 | wave-832/batch-0 |

### libavutil/imgutils.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_image_alloc` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_check_sar` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_check_size` | symbol | wave-10/batch-7 | wave-834/batch-0 |
| `av_image_check_size2` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_copy` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_copy2` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_copy_plane` | symbol | wave-10/batch-7 | wave-834/batch-0 |
| `av_image_copy_plane_uc_from` | symbol | wave-10/batch-7 | wave-834/batch-0 |
| `av_image_copy_to_buffer` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_copy_uc_from` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_fill_arrays` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_fill_black` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_fill_color` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_fill_linesizes` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_fill_max_pixsteps` | symbol | wave-14/batch-1 | wave-834/batch-0 |
| `av_image_fill_plane_sizes` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_fill_pointers` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_get_buffer_size` | symbol | wave-12/batch-4 | wave-834/batch-0 |
| `av_image_get_linesize` | symbol | wave-12/batch-4 | wave-834/batch-0 |

### libavutil/mathematics.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_add_stable` | symbol | wave-6/batch-2 | wave-836/batch-0 |
| `av_bessel_i0` | symbol | wave-4/batch-10 | wave-836/batch-0 |
| `av_compare_mod` | symbol | wave-4/batch-10 | wave-836/batch-0 |
| `av_compare_ts` | symbol | wave-6/batch-2 | wave-836/batch-0 |
| `av_gcd` | symbol | wave-4/batch-11 | wave-836/batch-0 |
| `av_rescale` | symbol | wave-4/batch-13 | wave-836/batch-0 |
| `av_rescale_delta` | symbol | wave-6/batch-2 | wave-836/batch-0 |
| `av_rescale_q` | symbol | wave-6/batch-2 | wave-836/batch-0 |
| `av_rescale_q_rnd` | symbol | wave-6/batch-2 | wave-836/batch-0 |
| `av_rescale_rnd` | symbol | wave-6/batch-2 | wave-836/batch-0 |

### libavutil/parseutils.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_find_info_tag` | symbol | wave-20/batch-3 | wave-838/batch-0 |
| `av_get_known_color_name` | symbol | wave-20/batch-3 | wave-838/batch-0 |
| `av_parse_color` | symbol | wave-20/batch-4 | wave-838/batch-0 |
| `av_parse_ratio` | symbol | wave-22/batch-1 | wave-838/batch-0 |
| `av_parse_time` | symbol | wave-20/batch-4 | wave-838/batch-0 |
| `av_parse_video_rate` | symbol | wave-22/batch-1 | wave-838/batch-0 |
| `av_parse_video_size` | symbol | wave-20/batch-4 | wave-838/batch-0 |
| `av_small_strptime` | symbol | wave-22/batch-1 | wave-838/batch-0 |
| `av_timegm` | symbol | wave-22/batch-1 | wave-838/batch-0 |

### libavutil/rc4.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVRC4` | type | wave-26/batch-6 | wave-840/batch-0 |
| `av_rc4_alloc` | symbol | wave-28/batch-2 | wave-842/batch-0 |
| `av_rc4_crypt` | symbol | wave-28/batch-2 | wave-842/batch-0 |
| `av_rc4_init` | symbol | wave-28/batch-2 | wave-842/batch-0 |

### libavutil/ripemd.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVRIPEMD` | type | wave-26/batch-6 | wave-844/batch-0 |
| `av_ripemd_size` | symbol | wave-26/batch-10 | wave-844/batch-1 |
| `av_ripemd_alloc` | symbol | wave-28/batch-2 | wave-846/batch-0 |
| `av_ripemd_final` | symbol | wave-28/batch-2 | wave-846/batch-0 |
| `av_ripemd_init` | symbol | wave-28/batch-2 | wave-846/batch-0 |
| `av_ripemd_update` | symbol | wave-28/batch-2 | wave-846/batch-0 |

### libavutil/samplefmt.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_get_alt_sample_fmt` | symbol | wave-12/batch-3 | wave-848/batch-0 |
| `av_get_bytes_per_sample` | symbol | wave-12/batch-3 | wave-848/batch-0 |
| `av_get_packed_sample_fmt` | symbol | wave-12/batch-3 | wave-848/batch-0 |
| `av_get_planar_sample_fmt` | symbol | wave-12/batch-4 | wave-848/batch-0 |
| `av_get_sample_fmt` | symbol | wave-12/batch-4 | wave-848/batch-0 |
| `av_get_sample_fmt_name` | symbol | wave-12/batch-4 | wave-848/batch-0 |
| `av_get_sample_fmt_string` | symbol | wave-12/batch-4 | wave-848/batch-0 |
| `av_sample_fmt_is_planar` | symbol | wave-12/batch-5 | wave-848/batch-0 |
| `av_samples_alloc` | symbol | wave-12/batch-5 | wave-848/batch-0 |
| `av_samples_alloc_array_and_samples` | symbol | wave-12/batch-5 | wave-848/batch-0 |
| `av_samples_copy` | symbol | wave-12/batch-5 | wave-848/batch-0 |
| `av_samples_fill_arrays` | symbol | wave-12/batch-5 | wave-848/batch-0 |
| `av_samples_get_buffer_size` | symbol | wave-12/batch-5 | wave-848/batch-0 |
| `av_samples_set_silence` | symbol | wave-12/batch-5 | wave-848/batch-0 |

### libavutil/sha.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVSHA` | type | wave-26/batch-7 | wave-860/batch-0 |
| `av_sha_size` | symbol | wave-26/batch-10 | wave-860/batch-1 |
| `av_sha_alloc` | symbol | wave-28/batch-2 | wave-862/batch-0 |
| `av_sha_final` | symbol | wave-28/batch-2 | wave-862/batch-0 |
| `av_sha_init` | symbol | wave-28/batch-2 | wave-862/batch-0 |
| `av_sha_update` | symbol | wave-28/batch-2 | wave-862/batch-0 |

### libavutil/sha512.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVSHA512` | type | wave-26/batch-7 | wave-864/batch-0 |
| `av_sha512_size` | symbol | wave-26/batch-10 | wave-864/batch-1 |
| `av_sha512_alloc` | symbol | wave-28/batch-2 | wave-866/batch-0 |
| `av_sha512_final` | symbol | wave-28/batch-2 | wave-866/batch-0 |
| `av_sha512_init` | symbol | wave-28/batch-2 | wave-866/batch-0 |
| `av_sha512_update` | symbol | wave-28/batch-2 | wave-866/batch-0 |

### libavutil/tea.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVTEA` | type | wave-26/batch-8 | wave-868/batch-0 |
| `av_tea_size` | symbol | wave-26/batch-10 | wave-868/batch-1 |
| `av_tea_alloc` | symbol | wave-28/batch-2 | wave-870/batch-0 |
| `av_tea_crypt` | symbol | wave-28/batch-2 | wave-870/batch-0 |
| `av_tea_init` | symbol | wave-28/batch-2 | wave-870/batch-0 |

### libavutil/time.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_gettime` | symbol | wave-4/batch-11 | wave-876/batch-0 |
| `av_gettime_relative` | symbol | wave-4/batch-11 | wave-876/batch-0 |
| `av_gettime_relative_is_monotonic` | symbol | wave-4/batch-11 | wave-876/batch-0 |
| `av_usleep` | symbol | wave-4/batch-14 | wave-876/batch-0 |

### libavutil/timestamp.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_ts_make_string` | symbol | wave-4/batch-14 | wave-884/batch-0 |
| `av_ts_make_time_string` | symbol | wave-6/batch-2 | wave-884/batch-0 |
| `av_ts_make_time_string2` | symbol | wave-6/batch-3 | wave-884/batch-0 |

### libavutil/twofish.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVTWOFISH` | type | wave-26/batch-8 | wave-890/batch-0 |
| `av_twofish_size` | symbol | wave-26/batch-10 | wave-890/batch-1 |
| `av_twofish_alloc` | symbol | wave-28/batch-2 | wave-892/batch-0 |
| `av_twofish_crypt` | symbol | wave-28/batch-2 | wave-892/batch-0 |
| `av_twofish_init` | symbol | wave-28/batch-2 | wave-892/batch-0 |

### libavutil/utils.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_fourcc_make_string` | symbol | wave-4/batch-11 | wave-898/batch-0 |
| `av_get_media_type_string` | symbol | wave-6/batch-2 | wave-898/batch-0 |
| `av_get_picture_type_char` | symbol | wave-6/batch-2 | wave-898/batch-0 |
| `av_get_time_base_q` | symbol | wave-6/batch-2 | wave-898/batch-0 |

### libavutil/frame.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVFrameSideData` | type | wave-34/batch-0 | wave-900/batch-0 |
| `AVSideDataDescriptor` | type | wave-30/batch-4 | wave-900/batch-0 |
| `AVFrame` | type | wave-36/batch-0 | wave-902/batch-0 |
| `AVRegionOfInterest` | type | wave-32/batch-0 | wave-902/batch-1 |
| `av_frame_alloc` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_apply_cropping` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_clone` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_copy` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_copy_props` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_free` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_get_buffer` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_get_plane_buffer` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_get_side_data` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_is_writable` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_make_writable` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_move_ref` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_new_side_data` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_new_side_data_from_buf` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_ref` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_remove_side_data` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_replace` | symbol | wave-38/batch-0 | wave-904/batch-0 |
| `av_frame_unref` | symbol | wave-38/batch-0 | wave-904/batch-0 |

### libavutil/hwcontext.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVHWDeviceContext` | type | wave-42/batch-0 | wave-952/batch-0 |
| `AVHWFramesConstraints` | type | wave-42/batch-0 | wave-952/batch-0 |
| `av_hwdevice_ctx_alloc` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwdevice_ctx_create` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwdevice_ctx_create_derived` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwdevice_ctx_create_derived_opts` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwdevice_ctx_init` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwdevice_find_type_by_name` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwdevice_get_type_name` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwdevice_hwconfig_alloc` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwdevice_iterate_types` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwframe_ctx_alloc` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwframe_ctx_create_derived` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwframe_ctx_init` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwframe_get_buffer` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwframe_map` | symbol | wave-42/batch-1 | wave-952/batch-1 |
| `av_hwframe_transfer_data` | symbol | wave-42/batch-2 | wave-952/batch-2 |
| `av_hwframe_transfer_get_formats` | symbol | wave-42/batch-2 | wave-952/batch-2 |
| `AVHWFramesContext` | type | wave-44/batch-0 | wave-954/batch-0 |
| `av_hwdevice_get_hwframe_constraints` | symbol | wave-44/batch-1 | wave-954/batch-1 |
| `av_hwframe_constraints_free` | symbol | wave-44/batch-1 | wave-954/batch-1 |

### libavutil/side_data.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_frame_side_data_add` | symbol | wave-36/batch-1 | wave-980/batch-0 |
| `av_frame_side_data_clone` | symbol | wave-36/batch-1 | wave-980/batch-0 |
| `av_frame_side_data_desc` | symbol | wave-32/batch-1 | wave-980/batch-0 |
| `av_frame_side_data_free` | symbol | wave-36/batch-1 | wave-980/batch-0 |
| `av_frame_side_data_get` | symbol | wave-36/batch-1 | wave-980/batch-0 |
| `av_frame_side_data_get_c` | symbol | wave-36/batch-1 | wave-980/batch-0 |
| `av_frame_side_data_name` | symbol | wave-32/batch-1 | wave-980/batch-0 |
| `av_frame_side_data_new` | symbol | wave-36/batch-1 | wave-980/batch-0 |
| `av_frame_side_data_remove` | symbol | wave-36/batch-1 | wave-980/batch-0 |
| `av_frame_side_data_remove_by_props` | symbol | wave-36/batch-1 | wave-980/batch-0 |

### libavutil/uuid.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_uuid_copy` | symbol | wave-26/batch-10 | wave-998/batch-0 |
| `av_uuid_equal` | symbol | wave-26/batch-10 | wave-998/batch-0 |
| `av_uuid_nil` | symbol | wave-26/batch-10 | wave-998/batch-0 |
| `av_uuid_parse` | symbol | wave-26/batch-10 | wave-998/batch-0 |
| `av_uuid_parse_range` | symbol | wave-26/batch-10 | wave-998/batch-0 |
| `av_uuid_unparse` | symbol | wave-26/batch-10 | wave-998/batch-0 |
| `av_uuid_urn_parse` | symbol | wave-26/batch-10 | wave-998/batch-0 |

### libavutil/version.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `av_version_info` | symbol | wave-4/batch-14 | wave-1000/batch-0 |
| `avutil_configuration` | symbol | wave-4/batch-14 | wave-1000/batch-0 |
| `avutil_license` | symbol | wave-4/batch-14 | wave-1000/batch-0 |
| `avutil_version` | symbol | wave-4/batch-14 | wave-1000/batch-0 |

### libavutil/xtea.c

| item | kind | ran in | new batch |
|---|---|---|---|
| `AVXTEA` | type | wave-26/batch-9 | wave-1010/batch-0 |
| `av_xtea_alloc` | symbol | wave-28/batch-2 | wave-1012/batch-0 |
| `av_xtea_crypt` | symbol | wave-28/batch-3 | wave-1012/batch-0 |
| `av_xtea_init` | symbol | wave-28/batch-3 | wave-1012/batch-0 |
| `av_xtea_le_crypt` | symbol | wave-28/batch-3 | wave-1012/batch-0 |
| `av_xtea_le_init` | symbol | wave-28/batch-3 | wave-1012/batch-0 |

## Notes on the re-plan

- The decomposition is unchanged. Every unit-level edge of the campaign-wide public-signature graph has its
  record in `modules.json`, and each of the 1337 units is owned by exactly one module's configuration.
- The re-extracted tables, with the wavefront fix for fields declared with a callback typedef, change no node
  or edge of the public-signature graph: its 1419 units, their `deps`, `fallback`, `back_fill`, layers and
  field anchors are identical to runs 3 and 4. The 11 fields are in internal headers, which the API graph
  does not walk. They would change a default (body-deep) graph only.
- The modules run in the order of `depends_on.scheduled`, and among ready modules the lower index runs first, as
  `specs/artifacts/modules.md` orders them. Run 4 broke that tie by name, so `libavformat/avformat.c` (5) now
  runs before `libavformat/allformats.c` (6), the only change to run 4's module order.
- The waves of the 101 modules that had `todo` waves before hold the same batches as run 4 planned, in the same
  order. Only their indices and `depends_on` are new.
- Pre-waves, each wrapped by its own module right before its first consumer: `AVRational` (rational.c) and
  `AVOptionArrayDef`, `AVOptionRange`, `AVOption`, `AVOptionRanges` (opt.c) before log.c; `AVPacketSideData`
  (libavcodec/packet.c) before libavutil/ambient_viewing_environment.c; `AVProfile` (libavcodec/utils.c) before
  codec_desc.c; `AVHWAccel` (decode.c) before avcodec.c; `AVBitStreamFilterGraph` (bsfgraph.c) before
  bitstreamfilter.c.

## Reproducing

From a clean checkout of this commit, with wavefront at `d39a8d6` or later and the CodeQL tables extracted
under `crustify/wavefront/codeql/`, build the campaign-wide public-signature graph:

```sh
mkdir -p crustify/tmp
F=$(python3 -c "import json; c = json.load(open('crustify/wavefront/configs/wavefront-config.json')); print(' '.join(c['impl_files']['included'] + c['api_headers']['included']))")
wavefront "$PWD/c" --config ../crustify/wavefront/configs/wavefront-config.json schedule \
  --file $F --transitive --api-headers-only --output "$PWD/crustify/tmp/all-api-full.schedule.json"
```

The following prints the landed and re-scheduled counts, the landed items with no new batch, and every late
pair: 12 pairs over 11 items. Waves 0-45 of `schedule.json` are the landed waves and waves 672 on the re-plan:

```sh
python3 - <<'PY'
import json, collections
S = json.load(open("crustify/artifacts/schedule.json"))
G = json.load(open("crustify/tmp/all-api-full.schedule.json"))
U = {(i["name"], i["defined_in"]): i for w in G["waves"] for b in w["batches"] for i in b["items"]}
at = lambda ws: {(i["name"], i["defined_in"]): (w["index"], b["index"]) for w in ws for b in w["batches"] for i in b.get("items", [])}
landed = at([w for w in S["waves"] if w["index"] <= 45])
new = at([w for w in S["waves"] if w["index"] >= 672])
print("landed", len(landed), "re-scheduled", len(set(landed) & set(new)), "no new batch", sorted(k[0] for k in set(landed) - set(new)))
late = 0
for k, (ow, ob) in sorted(landed.items(), key=lambda x: x[1]):
    if k not in new: continue
    for d in U[k]["deps"]["types"] + U[k]["deps"]["symbols"]:
        dk = (d["name"], d["defined_in"])
        if dk not in new or new[dk][0] >= new[k][0]: continue
        if dk in landed and (landed[dk][0] < ow or landed[dk] == (ow, ob)): continue
        late += 1
        print(f"{k[0]}: ran wave-{ow}/batch-{ob}, new wave-{new[k][0]}/batch-{new[k][1]}; {dk[0]}: ran",
              f"wave-{landed[dk][0]}/batch-{landed[dk][1]}" if dk in landed else "never", f"new wave-{new[dk][0]}/batch-{new[dk][1]}")
print("late pairs", late)
PY
```
