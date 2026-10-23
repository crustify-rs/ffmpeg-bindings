fn main() {
    use ffmpeg_bindings::libavutil::libavutil::crc::{Table, TableSize};
    let entries;
    {
        let table = Table::new(true, 32, 0xedb88320, TableSize::Fast).unwrap();
        entries = table.entries();
    }
    println!("{entries:?}");
}
