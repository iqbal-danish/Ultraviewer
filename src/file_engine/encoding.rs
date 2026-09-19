use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Unknown,
}

impl Encoding {
    pub fn name(&self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8 (BOM)",
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::Unknown => "Unknown / Binary",
        }
    }

    /// Returns the BOM size in bytes, if present.
    pub fn bom_length(&self) -> usize {
        match self {
            Encoding::Utf8Bom => 3,
            Encoding::Utf16Le => 2,
            Encoding::Utf16Be => 2,
            _ => 0,
        }
    }

    /// Detect encoding from the initial header bytes of a file.
    pub fn detect(header: &[u8]) -> Self {
        if header.is_empty() {
            return Encoding::Utf8;
        }

        // 1. Check for standard Byte Order Marks (BOM)
        if header.len() >= 3 && header[0..3] == [0xEF, 0xBB, 0xBF] {
            return Encoding::Utf8Bom;
        }
        if header.len() >= 2 && header[0..2] == [0xFF, 0xFE] {
            return Encoding::Utf16Le;
        }
        if header.len() >= 2 && header[0..2] == [0xFE, 0xFF] {
            return Encoding::Utf16Be;
        }

        // 2. Sample-based heuristics for files without BOM
        let sample_len = header.len().min(8192);
        let sample = &header[..sample_len];

        // Check for UTF-16 patterns (alternating nulls in ASCII range)
        if sample_len >= 4 {
            let even_nulls = sample.iter().step_by(2).filter(|&&b| b == 0).count();
            let odd_nulls = sample.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
            let half = sample_len / 2;

            if odd_nulls > half * 7 / 10 && even_nulls < half / 10 {
                return Encoding::Utf16Le;
            }
            if even_nulls > half * 7 / 10 && odd_nulls < half / 10 {
                return Encoding::Utf16Be;
            }
        }

        // Check for valid UTF-8
        if std::str::from_utf8(sample).is_ok() {
            return Encoding::Utf8;
        }

        // Try encoding_rs guess or fallback to UTF-8 lossy
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut dst = vec![0u8; sample.len() * 3];
        let (res, _, _) = decoder.decode_to_utf8_without_replacement(sample, &mut dst, false);
        if res == encoding_rs::DecoderResult::InputEmpty {
            return Encoding::Utf8;
        }

        Encoding::Unknown
    }

    /// Decodes a byte slice according to this encoding into a Cow<str>.
    /// Zero-copy when valid UTF-8 without conversion.
    pub fn decode_chunk<'a>(&self, bytes: &'a [u8]) -> Cow<'a, str> {
        match self {
            Encoding::Utf8 | Encoding::Utf8Bom => String::from_utf8_lossy(bytes),
            Encoding::Utf16Le => {
                let (cow, _, _) = encoding_rs::UTF_16LE.decode(bytes);
                Cow::Owned(cow.into_owned())
            }
            Encoding::Utf16Be => {
                let (cow, _, _) = encoding_rs::UTF_16BE.decode(bytes);
                Cow::Owned(cow.into_owned())
            }
            Encoding::Unknown => String::from_utf8_lossy(bytes),
        }
    }
}
