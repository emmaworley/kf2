use url::Url;

use super::error::HlsError;
use super::types::*;

pub fn parse_playlist(base_url: &Url, body: &str) -> Result<Playlist, HlsError> {
    let mut lines = body.lines().peekable();

    match lines.peek() {
        Some(first) if first.trim() == "#EXTM3U" => {
            lines.next();
        }
        _ => return Err(HlsError::Parse("missing #EXTM3U header".into())),
    }

    let mut target_duration = None;
    let mut media_sequence: u64 = 0;
    let mut end_list = false;
    let mut segments = Vec::new();

    let mut current_encryption = EncryptionState::None;
    let mut current_init_segment: Option<InitSegment> = None;
    let mut current_duration: Option<f64> = None;
    let mut current_byte_range: Option<ByteRange> = None;
    let mut pending_discontinuity = false;
    let mut next_byte_range_offset: u64 = 0;

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            target_duration = Some(
                rest.trim()
                    .parse::<u64>()
                    .map_err(|e| HlsError::Parse(format!("bad TARGETDURATION: {e}")))?,
            );
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            media_sequence = rest
                .trim()
                .parse()
                .map_err(|e| HlsError::Parse(format!("bad MEDIA-SEQUENCE: {e}")))?;
        } else if line == "#EXT-X-ENDLIST" {
            end_list = true;
        } else if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            current_encryption = parse_key(rest, base_url)?;
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            current_init_segment = Some(parse_map(rest, base_url)?);
        } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let duration_str = rest.split(',').next().unwrap_or(rest);
            current_duration = Some(
                duration_str
                    .trim()
                    .parse::<f64>()
                    .map_err(|e| HlsError::Parse(format!("bad EXTINF duration: {e}")))?,
            );
        } else if let Some(rest) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            current_byte_range = Some(parse_byte_range(rest, next_byte_range_offset)?);
        } else if line == "#EXT-X-DISCONTINUITY" {
            pending_discontinuity = true;
            next_byte_range_offset = 0;
        } else if line.starts_with('#') {
            // Unknown tag — skip
        } else {
            // URI line — this is a segment
            let url = resolve_url(base_url, line)?;
            let byte_range = current_byte_range.take();
            if let Some(ref br) = byte_range {
                next_byte_range_offset = br.offset + br.length;
            }
            let duration = current_duration.take().unwrap_or(0.0);

            segments.push(Segment {
                duration,
                url,
                byte_range,
                encryption: current_encryption.clone(),
                init_segment: current_init_segment.clone(),
                discontinuity: pending_discontinuity,
            });
            pending_discontinuity = false;
        }
    }

    Ok(Playlist {
        target_duration,
        media_sequence,
        segments,
        end_list,
    })
}

fn parse_key(attrs_str: &str, base_url: &Url) -> Result<EncryptionState, HlsError> {
    let attrs = parse_attributes(attrs_str);
    let method = attrs
        .get("METHOD")
        .ok_or_else(|| HlsError::Parse("EXT-X-KEY missing METHOD".into()))?;

    match method.as_str() {
        "NONE" => Ok(EncryptionState::None),
        "AES-128" => {
            let uri_raw = attrs
                .get("URI")
                .ok_or_else(|| HlsError::Parse("AES-128 key missing URI".into()))?;
            let key_url = resolve_url(base_url, uri_raw)?;
            let iv = match attrs.get("IV") {
                Some(iv_str) => Some(parse_iv(iv_str)?),
                None => None,
            };
            Ok(EncryptionState::Aes128 { key_url, iv })
        }
        other => Err(HlsError::UnsupportedEncryption(other.to_string())),
    }
}

fn parse_map(attrs_str: &str, base_url: &Url) -> Result<InitSegment, HlsError> {
    let attrs = parse_attributes(attrs_str);
    let uri_raw = attrs
        .get("URI")
        .ok_or_else(|| HlsError::Parse("EXT-X-MAP missing URI".into()))?;
    let url = resolve_url(base_url, uri_raw)?;

    let byte_range = match attrs.get("BYTERANGE") {
        Some(br_str) => {
            let br = parse_byte_range(br_str, 0)?;
            Some(InitByteRange {
                length: br.length,
                offset: br.offset,
            })
        }
        None => None,
    };

    Ok(InitSegment { url, byte_range })
}

fn parse_byte_range(s: &str, default_offset: u64) -> Result<ByteRange, HlsError> {
    let s = s.trim();
    let (length_str, offset) = match s.split_once('@') {
        Some((l, o)) => (
            l,
            o.parse::<u64>()
                .map_err(|e| HlsError::Parse(format!("bad byte range offset: {e}")))?,
        ),
        None => (s, default_offset),
    };
    let length = length_str
        .parse::<u64>()
        .map_err(|e| HlsError::Parse(format!("bad byte range length: {e}")))?;
    Ok(ByteRange { length, offset })
}

fn parse_iv(s: &str) -> Result<[u8; 16], HlsError> {
    let hex_str = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    let bytes = hex::decode(hex_str).map_err(|e| HlsError::Parse(format!("bad IV hex: {e}")))?;
    if bytes.len() != 16 {
        return Err(HlsError::Parse(format!(
            "IV must be 16 bytes, got {}",
            bytes.len()
        )));
    }
    let mut iv = [0u8; 16];
    iv.copy_from_slice(&bytes);
    Ok(iv)
}

fn resolve_url(base_url: &Url, uri: &str) -> Result<String, HlsError> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        Ok(uri.to_string())
    } else {
        base_url
            .join(uri)
            .map(|u| u.to_string())
            .map_err(|e| HlsError::Parse(format!("cannot resolve URL '{uri}': {e}")))
    }
}

/// Parse m3u8 attribute list (e.g. `METHOD=AES-128,URI="https://...",IV=0x...`).
/// Handles quoted values that may contain commas.
fn parse_attributes(input: &str) -> std::collections::HashMap<String, String> {
    let mut attrs = std::collections::HashMap::new();
    let mut remaining = input.trim();

    while !remaining.is_empty() {
        let eq_pos = match remaining.find('=') {
            Some(p) => p,
            None => break,
        };
        let key = remaining[..eq_pos].trim().to_uppercase();
        remaining = &remaining[eq_pos + 1..];

        let (value, rest) = if let Some(after_quote) = remaining.strip_prefix('"') {
            match after_quote.find('"') {
                Some(end) => {
                    let val = &after_quote[..end];
                    let rest = &after_quote[end + 1..];
                    let rest = rest.strip_prefix(',').unwrap_or(rest);
                    (val.to_string(), rest)
                }
                None => {
                    // Unterminated quote — take the rest
                    (after_quote.to_string(), "")
                }
            }
        } else {
            // Unquoted value — ends at next comma
            match remaining.find(',') {
                Some(comma) => (remaining[..comma].to_string(), &remaining[comma + 1..]),
                None => (remaining.to_string(), ""),
            }
        };

        attrs.insert(key, value);
        remaining = rest.trim_start();
    }

    attrs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_url() -> Url {
        Url::parse("https://example.com/stream/playlist.m3u8").unwrap()
    }

    #[test]
    fn parse_basic_playlist() {
        let body = "\
#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXTINF:9.009,
segment0.ts
#EXTINF:9.009,
segment1.ts
#EXTINF:3.003,
segment2.ts
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        assert_eq!(playlist.target_duration, Some(10));
        assert_eq!(playlist.media_sequence, 0);
        assert!(playlist.end_list);
        assert_eq!(playlist.segments.len(), 3);
        assert_eq!(
            playlist.segments[0].url,
            "https://example.com/stream/segment0.ts"
        );
        assert!((playlist.segments[0].duration - 9.009).abs() < 0.001);
        assert!((playlist.segments[2].duration - 3.003).abs() < 0.001);
    }

    #[test]
    fn parse_absolute_urls() {
        let body = "\
#EXTM3U
#EXTINF:5.0,
https://cdn.example.com/seg0.ts
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        assert_eq!(playlist.segments[0].url, "https://cdn.example.com/seg0.ts");
    }

    #[test]
    fn parse_encryption_aes128_with_iv() {
        let body = "\
#EXTM3U
#EXT-X-KEY:METHOD=AES-128,URI=\"https://keys.example.com/key.bin\",IV=0x00000000000000000000000000000001
#EXTINF:10.0,
seg0.ts
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        match &playlist.segments[0].encryption {
            EncryptionState::Aes128 { key_url, iv } => {
                assert_eq!(key_url, "https://keys.example.com/key.bin");
                let expected_iv = {
                    let mut iv = [0u8; 16];
                    iv[15] = 1;
                    iv
                };
                assert_eq!(iv.unwrap(), expected_iv);
            }
            _ => panic!("expected AES-128 encryption"),
        }
    }

    #[test]
    fn parse_encryption_aes128_without_iv() {
        let body = "\
#EXTM3U
#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"
#EXTINF:10.0,
seg0.ts
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        match &playlist.segments[0].encryption {
            EncryptionState::Aes128 { key_url, iv } => {
                assert_eq!(key_url, "https://example.com/stream/key.bin");
                assert!(iv.is_none());
            }
            _ => panic!("expected AES-128 encryption"),
        }
    }

    #[test]
    fn parse_encryption_none_clears_state() {
        let body = "\
#EXTM3U
#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"
#EXTINF:10.0,
seg0.ts
#EXT-X-KEY:METHOD=NONE
#EXTINF:10.0,
seg1.ts
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        assert!(matches!(
            playlist.segments[0].encryption,
            EncryptionState::Aes128 { .. }
        ));
        assert!(matches!(
            playlist.segments[1].encryption,
            EncryptionState::None
        ));
    }

    #[test]
    fn parse_byte_range_with_offset() {
        let body = "\
#EXTM3U
#EXT-X-BYTERANGE:1000@0
#EXTINF:5.0,
bigfile.ts
#EXT-X-BYTERANGE:1000@1000
#EXTINF:5.0,
bigfile.ts
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        let br0 = playlist.segments[0].byte_range.as_ref().unwrap();
        assert_eq!(br0.length, 1000);
        assert_eq!(br0.offset, 0);
        let br1 = playlist.segments[1].byte_range.as_ref().unwrap();
        assert_eq!(br1.length, 1000);
        assert_eq!(br1.offset, 1000);
    }

    #[test]
    fn parse_byte_range_implicit_offset() {
        let body = "\
#EXTM3U
#EXT-X-BYTERANGE:500@0
#EXTINF:5.0,
bigfile.ts
#EXT-X-BYTERANGE:500
#EXTINF:5.0,
bigfile.ts
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        let br1 = playlist.segments[1].byte_range.as_ref().unwrap();
        assert_eq!(br1.length, 500);
        assert_eq!(br1.offset, 500);
    }

    #[test]
    fn parse_init_segment() {
        let body = "\
#EXTM3U
#EXT-X-MAP:URI=\"init.mp4\"
#EXTINF:5.0,
seg0.m4s
#EXTINF:5.0,
seg1.m4s
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        let init = playlist.segments[0].init_segment.as_ref().unwrap();
        assert_eq!(init.url, "https://example.com/stream/init.mp4");
        assert!(init.byte_range.is_none());
        // Both segments should share the same init segment
        assert_eq!(
            playlist.segments[1].init_segment.as_ref().unwrap().url,
            init.url
        );
    }

    #[test]
    fn parse_init_segment_with_byte_range() {
        let body = "\
#EXTM3U
#EXT-X-MAP:URI=\"combined.mp4\",BYTERANGE=\"652@0\"
#EXTINF:5.0,
seg0.m4s
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        let init = playlist.segments[0].init_segment.as_ref().unwrap();
        let br = init.byte_range.as_ref().unwrap();
        assert_eq!(br.length, 652);
        assert_eq!(br.offset, 0);
    }

    #[test]
    fn parse_discontinuity() {
        let body = "\
#EXTM3U
#EXTINF:5.0,
seg0.ts
#EXT-X-DISCONTINUITY
#EXTINF:5.0,
seg1.ts
#EXTINF:5.0,
seg2.ts
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        assert!(!playlist.segments[0].discontinuity);
        assert!(playlist.segments[1].discontinuity);
        assert!(!playlist.segments[2].discontinuity);
    }

    #[test]
    fn missing_extm3u_header() {
        let body = "#EXTINF:5.0,\nseg0.ts\n";
        let err = parse_playlist(&base_url(), body).unwrap_err();
        assert!(err.to_string().contains("EXTM3U"));
    }

    #[test]
    fn parse_attributes_handles_quoted_commas() {
        let input = r#"METHOD=AES-128,URI="https://example.com/key?a=1,b=2",IV=0x01"#;
        let attrs = parse_attributes(input);
        assert_eq!(attrs["METHOD"], "AES-128");
        assert_eq!(attrs["URI"], "https://example.com/key?a=1,b=2");
        assert_eq!(attrs["IV"], "0x01");
    }

    #[test]
    fn media_sequence_nonzero() {
        let body = "\
#EXTM3U
#EXT-X-MEDIA-SEQUENCE:42
#EXTINF:5.0,
seg42.ts
#EXT-X-ENDLIST";
        let playlist = parse_playlist(&base_url(), body).unwrap();
        assert_eq!(playlist.media_sequence, 42);
    }
}
