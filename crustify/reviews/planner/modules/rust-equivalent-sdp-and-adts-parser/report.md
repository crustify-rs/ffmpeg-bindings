# Established crates missed for sdp.c and adts_parser.c

## What the earlier plan decided

Planner run 3 recorded no equivalent for `libavformat/sdp.c` (libavformat 16) or
`libavcodec/adts_parser.c` (libavcodec 1): `"rust_equivalent": {"std": null, "crates": []}`.

## Evidence against it

crates.io (queried 2026-10-23):

- `sdp` 0.17.2 (webrtc-rs, MIT OR Apache-2.0, 7.2M downloads, released 2026-07-20; pure
  Rust): an SDP session model with parse and marshal. `av_sdp_create` builds an SDP session
  from AVFormatContexts; the crate covers the session text, not the media, rtpmap and fmtp
  lines the C derives from each stream's codec parameters and extradata.
  `curl -s https://crates.io/api/v1/crates/sdp`.
- `adts-reader` 0.4.0 (MIT OR Apache-2.0, released 2026-03-06; pure Rust, no dependencies on
  C): `AdtsHeader::from_bytes` validates the sync word, the sampling-frequency index and the
  frame length, and gives `number_of_raw_data_blocks_in_frame()`, which is
  `av_adts_header_parse`'s `frames` (`samples` is that times 1024). Its source
  (`curl -sL https://crates.io/api/v1/crates/adts-reader/0.4.0/download | tar xz`,
  `src/lib.rs:110-131`) also requires the two CRC bytes, and a frame length of at least 9,
  when protection is present, where `ff_adts_header_parse` (`c/libavcodec/adts_header.c`)
  reads 7 bytes and accepts a frame length of 7.

## The edit and what it moves

Both modules list the crate with `partial` parity and those differences. Their objective
stays `wrap` (every module of a wrap campaign is); no wave moves.
