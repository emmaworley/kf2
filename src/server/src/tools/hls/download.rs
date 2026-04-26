use std::collections::HashMap;
use std::path::Path;

use aes::Aes128;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use reqwest::Client;

use super::error::HlsError;
use super::progress::{HlsEvent, ProgressReporter};
use super::types::*;

type Aes128CbcDec = cbc::Decryptor<Aes128>;

pub async fn download_playlist(
    client: &Client,
    playlist: &Playlist,
    output_dir: &Path,
    progress: &dyn ProgressReporter,
) -> Result<DownloadResult, HlsError> {
    let mut key_cache: HashMap<String, [u8; 16]> = HashMap::new();
    let mut init_cache: HashMap<InitSegment, String> = HashMap::new();
    let mut init_counter: usize = 0;

    let mut playlist_lines = Vec::new();
    playlist_lines.push("#EXTM3U".to_string());
    if let Some(td) = playlist.target_duration {
        playlist_lines.push(format!("#EXT-X-TARGETDURATION:{td}"));
    }
    playlist_lines.push("#EXT-X-MEDIA-SEQUENCE:0".to_string());

    let total_segments = playlist.segments.len();
    progress.report(HlsEvent::PlaylistParsed {
        segment_count: total_segments,
    });

    let mut prev_init: Option<&InitSegment> = None;

    for (i, segment) in playlist.segments.iter().enumerate() {
        let filename = format!("seg_{i:05}.ts");
        let file_path = output_dir.join(&filename);

        // Download init segment if new/changed
        if let Some(init) = &segment.init_segment
            && !init_cache.contains_key(init)
        {
            let init_filename = format!("init_{init_counter}.ts");
            let init_path = output_dir.join(&init_filename);
            let data = fetch_data(client, &init.url, init.byte_range.as_ref()).await?;
            write_file(&init_path, &data).await?;
            init_cache.insert(init.clone(), init_filename);
            init_counter += 1;
            progress.report(HlsEvent::InitSegmentComplete { url: &init.url });
        }

        // Emit EXT-X-MAP in rewritten playlist when init segment changes
        let init_changed = match (&segment.init_segment, prev_init) {
            (Some(cur), Some(prev)) => cur != prev,
            (Some(_), None) => true,
            (None, Some(_)) => true,
            (None, None) => false,
        };
        if init_changed && let Some(init) = &segment.init_segment {
            let init_filename = &init_cache[init];
            playlist_lines.push(format!("#EXT-X-MAP:URI=\"{init_filename}\""));
        }
        prev_init = segment.init_segment.as_ref();

        if segment.discontinuity {
            playlist_lines.push("#EXT-X-DISCONTINUITY".to_string());
        }

        // Download segment data
        let byte_range = segment.byte_range.as_ref().map(|br| (br.length, br.offset));
        let mut data = fetch_data(
            client,
            &segment.url,
            byte_range
                .as_ref()
                .map(|(l, o)| InitByteRange {
                    length: *l,
                    offset: *o,
                })
                .as_ref(),
        )
        .await?;

        // Decrypt if needed
        match &segment.encryption {
            EncryptionState::None => {}
            EncryptionState::Aes128 { key_url, iv } => {
                let key = fetch_key(client, key_url, &mut key_cache).await?;
                let iv_bytes = match iv {
                    Some(explicit_iv) => *explicit_iv,
                    None => {
                        let seq = playlist.media_sequence + i as u64;
                        let mut iv_buf = [0u8; 16];
                        iv_buf[8..16].copy_from_slice(&seq.to_be_bytes());
                        iv_buf
                    }
                };
                data = decrypt_aes128_cbc(&data, &key, &iv_bytes, &filename)?;
            }
        }

        write_file(&file_path, &data).await?;

        playlist_lines.push(format!("#EXTINF:{},", segment.duration));
        playlist_lines.push(filename);

        progress.report(HlsEvent::SegmentComplete {
            index: i,
            total: total_segments,
        });
    }

    if playlist.end_list {
        playlist_lines.push("#EXT-X-ENDLIST".to_string());
    }

    let playlist_path = output_dir.join("playlist.m3u8");
    let playlist_content = playlist_lines.join("\n") + "\n";
    write_file(&playlist_path, playlist_content.as_bytes()).await?;

    Ok(DownloadResult {
        playlist_path,
        segment_count: playlist.segments.len(),
    })
}

async fn fetch_data(
    client: &Client,
    url: &str,
    byte_range: Option<&InitByteRange>,
) -> Result<Vec<u8>, HlsError> {
    let mut request = client.get(url);
    if let Some(br) = byte_range {
        let end = br.offset + br.length - 1;
        request = request.header("Range", format!("bytes={}-{end}", br.offset));
    }
    let bytes = request.send().await?.error_for_status()?.bytes().await?;
    Ok(bytes.to_vec())
}

async fn fetch_key(
    client: &Client,
    key_url: &str,
    cache: &mut HashMap<String, [u8; 16]>,
) -> Result<[u8; 16], HlsError> {
    if let Some(key) = cache.get(key_url) {
        return Ok(*key);
    }
    let bytes = client
        .get(key_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if bytes.len() != 16 {
        return Err(HlsError::Decryption {
            segment: key_url.to_string(),
            reason: format!("key must be 16 bytes, got {}", bytes.len()),
        });
    }
    let mut key = [0u8; 16];
    key.copy_from_slice(&bytes);
    cache.insert(key_url.to_string(), key);
    Ok(key)
}

fn decrypt_aes128_cbc(
    data: &[u8],
    key: &[u8; 16],
    iv: &[u8; 16],
    segment_name: &str,
) -> Result<Vec<u8>, HlsError> {
    let mut buf = data.to_vec();
    let decrypted = Aes128CbcDec::new(key.into(), iv.into())
        .decrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut buf)
        .map_err(|e| HlsError::Decryption {
            segment: segment_name.to_string(),
            reason: format!("AES-128-CBC decryption failed: {e}"),
        })?;
    Ok(decrypted.to_vec())
}

async fn write_file(path: &Path, data: &[u8]) -> Result<(), HlsError> {
    tokio::fs::write(path, data)
        .await
        .map_err(|e| HlsError::Io {
            path: path.to_path_buf(),
            source: e,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::{BlockEncryptMut, KeyIvInit};

    type Aes128CbcEnc = cbc::Encryptor<Aes128>;

    #[test]
    fn decrypt_known_vector() {
        let key = [0x01u8; 16];
        let iv = [0x02u8; 16];
        let plaintext = b"hello world!!!!!"; // 16 bytes, one block

        // Encrypt with PKCS7 padding
        let mut buf = [0u8; 32]; // one block plaintext + one block padding
        buf[..16].copy_from_slice(plaintext);
        let ciphertext = Aes128CbcEnc::new(&key.into(), &iv.into())
            .encrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut buf, 16)
            .unwrap();

        let decrypted = decrypt_aes128_cbc(ciphertext, &key, &iv, "test").unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn decrypt_wrong_key_fails() {
        let key = [0x01u8; 16];
        let iv = [0x02u8; 16];
        let plaintext = b"hello world!!!!!";

        let mut buf = [0u8; 32];
        buf[..16].copy_from_slice(plaintext);
        let ciphertext = Aes128CbcEnc::new(&key.into(), &iv.into())
            .encrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut buf, 16)
            .unwrap()
            .to_vec();

        let wrong_key = [0xFFu8; 16];
        let result = decrypt_aes128_cbc(&ciphertext, &wrong_key, &iv, "test");
        assert!(result.is_err());
    }
}
