fn main() {
    use ffmpeg::libavutil::crc::{Table, TableSize};
    let table = Table::new(true, 32, 0xedb88320, TableSize::Fast).unwrap();
    table.entries()[0] = 99;
}
