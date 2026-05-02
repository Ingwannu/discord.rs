use flate2::{Decompress, FlushDecompress};
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::ws::GatewayCompression;

pub(super) const ZLIB_SUFFIX: &[u8] = &[0x00, 0x00, 0xff, 0xff];

pub(super) fn decode_gateway_message(
    message: WsMessage,
    compression_decoder: &mut GatewayCompressionDecoder,
) -> Result<Option<String>, String> {
    match message {
        WsMessage::Text(text) => Ok(Some(text.to_string())),
        WsMessage::Binary(bytes) => compression_decoder.decode(&bytes),
        _ => Ok(None),
    }
}

pub(super) struct GatewayZlibStream {
    decoder: Decompress,
    pending: Vec<u8>,
}

pub(super) enum GatewayCompressionDecoder {
    None,
    Zlib(GatewayZlibStream),
    #[cfg(feature = "zstd-stream")]
    Zstd(GatewayZstdStream),
}

impl GatewayCompressionDecoder {
    pub(super) fn new(compression: Option<GatewayCompression>) -> Result<Self, String> {
        let decoder = match compression {
            Some(GatewayCompression::ZlibStream) => Self::Zlib(GatewayZlibStream::new()),
            #[cfg(feature = "zstd-stream")]
            Some(GatewayCompression::ZstdStream) => Self::Zstd(GatewayZstdStream::new()?),
            None => Self::None,
        };
        Ok(decoder)
    }

    pub(super) fn decode(&mut self, bytes: &[u8]) -> Result<Option<String>, String> {
        match self {
            GatewayCompressionDecoder::None => String::from_utf8(bytes.to_vec())
                .map(Some)
                .map_err(|e| format!("binary gateway payload was not valid UTF-8: {e}")),
            GatewayCompressionDecoder::Zlib(stream) => stream.decode(bytes),
            #[cfg(feature = "zstd-stream")]
            GatewayCompressionDecoder::Zstd(stream) => stream.decode(bytes),
        }
    }
}

#[cfg(feature = "zstd-stream")]
pub(super) struct GatewayZstdStream {
    decoder: zstd::stream::raw::Decoder<'static>,
}

#[cfg(feature = "zstd-stream")]
impl GatewayZstdStream {
    fn new() -> Result<Self, String> {
        let decoder = zstd::stream::raw::Decoder::new()
            .map_err(|error| format!("zstd-stream decoder initialization failed: {error}"))?;
        Ok(Self { decoder })
    }

    fn decode(&mut self, bytes: &[u8]) -> Result<Option<String>, String> {
        use zstd::stream::raw::{Operation, OutBuffer};

        let mut input = bytes;
        let mut decompressed = Vec::new();
        while !input.is_empty() {
            let mut output = [0_u8; 8192];
            let status = self
                .decoder
                .run_on_buffers(input, &mut output)
                .map_err(|e| format!("zstd-stream decompression failed: {e}"))?;
            decompressed.extend_from_slice(&output[..status.bytes_written]);
            input = &input[status.bytes_read..];

            if status.bytes_read == 0 && status.bytes_written == 0 {
                break;
            }
        }

        loop {
            let mut output = [0_u8; 8192];
            let mut output_buffer = OutBuffer::around(&mut output);
            let remaining = self
                .decoder
                .flush(&mut output_buffer)
                .map_err(|e| format!("zstd-stream flush failed: {e}"))?;
            let written = output_buffer.pos();
            decompressed.extend_from_slice(&output[..written]);
            if remaining == 0 || written == 0 {
                break;
            }
        }

        if decompressed.is_empty() {
            return Ok(None);
        }

        String::from_utf8(decompressed)
            .map(Some)
            .map_err(|e| format!("zstd-stream payload was not valid UTF-8: {e}"))
    }
}

impl GatewayZlibStream {
    pub(super) fn new() -> Self {
        Self {
            decoder: Decompress::new(true),
            pending: Vec::new(),
        }
    }

    pub(super) fn decode(&mut self, bytes: &[u8]) -> Result<Option<String>, String> {
        let mut input = bytes;
        loop {
            let input_before = self.decoder.total_in();
            let output_before = self.decoder.total_out();
            let mut output = [0_u8; 8192];
            self.decoder
                .decompress(input, &mut output, FlushDecompress::Sync)
                .map_err(|e| format!("zlib-stream decompression failed: {e}"))?;

            let consumed = (self.decoder.total_in() - input_before) as usize;
            let produced = (self.decoder.total_out() - output_before) as usize;
            self.pending.extend_from_slice(&output[..produced]);

            if consumed == 0 && produced == 0 {
                break;
            }
            input = &input[consumed..];
            if input.is_empty() {
                break;
            }
        }

        if !bytes.ends_with(ZLIB_SUFFIX) {
            return Ok(None);
        }

        let decompressed = std::mem::take(&mut self.pending);
        String::from_utf8(decompressed)
            .map(Some)
            .map_err(|e| format!("zlib-stream payload was not valid UTF-8: {e}"))
    }
}
