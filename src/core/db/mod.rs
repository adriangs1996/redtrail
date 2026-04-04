mod schema;
mod connection;
mod types;
mod command;
mod session;
mod streaming;

pub use connection::*;
pub use types::*;
pub use command::*;
pub use session::*;
pub use streaming::*;

// Shared compression helpers used across submodules
pub(crate) fn decompress_blob(blob: &[u8]) -> Option<String> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut decoder = ZlibDecoder::new(blob);
    let mut out = String::new();
    decoder.read_to_string(&mut out).ok()?;
    Some(out)
}

fn compress_zlib(data: &str) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data.as_bytes()).expect("zlib write");
    encoder.finish().expect("zlib finish")
}
