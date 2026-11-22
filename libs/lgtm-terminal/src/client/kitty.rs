//! Streaming Kitty Graphics Protocol (`\x1b_G...`) rewriter converting local file payloads (`t=f`/`t=t`) into inline PNG (`t=d,f=100`) so `@wterm/ghostty` WASM can render them natively.

use base64::Engine;
use image::{ImageBuffer, Rgb, Rgba};

/// Rewrites Kitty Graphics APC local-file (`t=f`) and temp-file (`t=t`) sequences into inline PNG (`t=d,f=100`) sequences across streaming chunk boundaries.
#[derive(Debug, Default)]
pub struct KittyInlineRewriter {
    pending: Vec<u8>,
    rewritten_count: usize,
}

impl KittyInlineRewriter {
    /// Creates an empty [`KittyInlineRewriter`].
    pub fn new() -> Self {
        Self::default()
    }

    // Public ------------------------------------------------------------------

    /// Returns the number of local-file Kitty sequences rewritten to inline PNGs so far.
    pub fn get_rewritten_count(&self) -> usize {
        self.rewritten_count
    }

    /// Processes an incoming PTY byte slice and returns bytes with local-file Kitty sequences rewritten to inline PNG payloads.
    pub fn process(&mut self, input: &[u8]) -> Vec<u8> {
        if self.pending.is_empty() && !Self::contains_apc_prefix(input) {
            return input.to_vec();
        }

        let mut combined = std::mem::take(&mut self.pending);
        combined.extend_from_slice(input);

        let mut output = Vec::with_capacity(combined.len());
        let mut cursor = 0;

        while cursor < combined.len() {
            let Some(rel_start) = Self::find_apc_start(&combined[cursor..]) else {
                // Keep trailing ESC or ESC _ if it might be the start of a split APC prefix.
                let tail_keep = Self::trailing_partial_prefix_len(&combined[cursor..]);
                let emit_end = combined.len() - tail_keep;
                output.extend_from_slice(&combined[cursor..emit_end]);
                if tail_keep > 0 {
                    self.pending.extend_from_slice(&combined[emit_end..]);
                }
                break;
            };

            let start = cursor + rel_start;
            output.extend_from_slice(&combined[cursor..start]);

            let body_start = start + 3;
            let Some((rel_end, term_len)) = Self::find_apc_end(&combined[body_start..]) else {
                self.pending.extend_from_slice(&combined[start..]);
                break;
            };

            let body_end = body_start + rel_end;
            let full_end = body_end + term_len;
            let body = &combined[body_start..body_end];

            if let Some(rewritten) = self.try_rewrite_file_apc(body) {
                self.rewritten_count += 1;
                output.extend_from_slice(&rewritten);
            } else {
                output.extend_from_slice(&combined[start..full_end]);
            }

            cursor = full_end;
        }

        output
    }

    // Private -----------------------------------------------------------------

    fn contains_apc_prefix(bytes: &[u8]) -> bool {
        bytes.windows(3).any(|w| w == b"\x1b_G")
            || bytes.ends_with(b"\x1b")
            || bytes.ends_with(b"\x1b_")
    }

    fn find_apc_start(bytes: &[u8]) -> Option<usize> {
        bytes.windows(3).position(|w| w == b"\x1b_G")
    }

    fn find_apc_end(bytes: &[u8]) -> Option<(usize, usize)> {
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == 0x07 {
                return Some((i, 1));
            }
            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                return Some((i, 2));
            }
            i += 1;
        }
        None
    }

    fn trailing_partial_prefix_len(bytes: &[u8]) -> usize {
        if bytes.ends_with(b"\x1b_") {
            2
        } else if bytes.ends_with(b"\x1b") {
            1
        } else {
            0
        }
    }

    fn try_rewrite_file_apc(&self, body: &[u8]) -> Option<Vec<u8>> {
        let body_str = std::str::from_utf8(body).ok()?;
        let (params_str, payload_b64) = body_str.split_once(';')?;

        let mut transmit_type: Option<char> = None;
        let mut format: u32 = 32;
        let mut width_px: u32 = 0;
        let mut height_px: u32 = 0;
        let mut kept_params: Vec<&str> = Vec::new();

        for kv in params_str.split(',') {
            let Some((k, v)) = kv.split_once('=') else {
                if !kv.is_empty() {
                    kept_params.push(kv);
                }
                continue;
            };
            match k {
                "t" => transmit_type = v.chars().next(),
                "f" => format = v.parse().unwrap_or(format),
                "s" => width_px = v.parse().unwrap_or(width_px),
                "v" => height_px = v.parse().unwrap_or(height_px),
                "S" | "O" => {}
                _ => kept_params.push(kv),
            }
        }

        if !matches!(transmit_type, Some('f') | Some('t')) {
            return None;
        }

        let path_bytes = base64::engine::general_purpose::STANDARD
            .decode(payload_b64.trim())
            .ok()?;
        let file_path = String::from_utf8(path_bytes).ok()?;
        let raw_bytes = std::fs::read(file_path.trim()).ok()?;
        if transmit_type == Some('t') {
            let _ = std::fs::remove_file(file_path.trim());
        }

        let png_bytes = Self::normalize_to_png(&raw_bytes, format, width_px, height_px)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

        let mut out = String::with_capacity(encoded.len() + 64);
        out.push_str("\x1b_G");
        for (idx, param) in kept_params.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            out.push_str(param);
        }
        if !kept_params.is_empty() {
            out.push(',');
        }
        out.push_str("t=d,f=100;");
        out.push_str(&encoded);
        out.push_str("\x1b\\");
        Some(out.into_bytes())
    }

    fn normalize_to_png(
        raw_bytes: &[u8],
        format: u32,
        width_px: u32,
        height_px: u32,
    ) -> Option<Vec<u8>> {
        if raw_bytes.is_empty() {
            return None;
        }
        if format == 100 || raw_bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Some(raw_bytes.to_vec());
        }

        let w = width_px.max(1);
        let h = height_px.max(1);
        let mut png_buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_buf);

        if format == 24 && raw_bytes.len() >= (w * h * 3) as usize {
            let img = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(
                w,
                h,
                raw_bytes[..(w * h * 3) as usize].to_vec(),
            )?;
            img.write_to(&mut cursor, image::ImageFormat::Png).ok()?;
            return Some(png_buf);
        }

        if format == 32 && raw_bytes.len() >= (w * h * 4) as usize {
            let img = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(
                w,
                h,
                raw_bytes[..(w * h * 4) as usize].to_vec(),
            )?;
            img.write_to(&mut cursor, image::ImageFormat::Png).ok()?;
            return Some(png_buf);
        }

        None
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // fixtures ----------------------------------------------------------------

    const PNG_1X1_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    // tests -------------------------------------------------------------------

    mod given_a_local_png_file {
        use super::*;

        mod when_processing_a_split_kitty_file_transmission {
            use super::*;

            #[test]
            fn should_rewrite_t_equals_f_into_inline_t_equals_d_png() {
                // Arrange
                let dir = tempfile::tempdir().unwrap();
                let img_path = dir.path().join("preview.png");
                let png_bytes = base64::engine::general_purpose::STANDARD
                    .decode(PNG_1X1_B64)
                    .unwrap();
                std::fs::write(&img_path, &png_bytes).unwrap();

                let path_b64 = base64::engine::general_purpose::STANDARD
                    .encode(img_path.to_string_lossy().as_bytes());
                let apc = format!("before\x1b_Ga=T,t=f,f=100,i=7,c=8,r=4;{path_b64}\x1b\\after");
                let split = apc.len() / 2;
                let mut rewriter = KittyInlineRewriter::new();

                // Act
                let mut out = rewriter.process(&apc.as_bytes()[..split]);
                out.extend_from_slice(&rewriter.process(&apc.as_bytes()[split..]));
                let rendered = String::from_utf8(out).unwrap();

                // Assert
                assert_eq!(rewriter.get_rewritten_count(), 1);
                assert!(rendered.starts_with("before\x1b_Ga=T,i=7,c=8,r=4,t=d,f=100;"));
                assert!(rendered.contains(PNG_1X1_B64));
                assert!(rendered.ends_with("\x1b\\after"));
            }
        }
    }
}
