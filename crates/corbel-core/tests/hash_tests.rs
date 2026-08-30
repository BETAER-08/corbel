use corbel_core::error::Error;
use corbel_core::hash::{ContentHash, hash_bytes, hash_file};
use std::fs;
use tempfile::tempdir;

#[test]
fn hash_bytes_and_hash_file_agree_on_same_content() {
    let data = b"hello, corbel";
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("a.txt");
    fs::write(&file_path, data).unwrap();

    let from_bytes = hash_bytes(data);
    let from_file = hash_file(&file_path).unwrap();

    assert_eq!(from_bytes, from_file);
}

#[test]
fn different_content_produces_different_hash() {
    let a = hash_bytes(b"content-a");
    let b = hash_bytes(b"content-b");

    assert_ne!(a, b);
}

#[test]
fn empty_file_returns_valid_hash() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("empty.txt");
    fs::write(&file_path, b"").unwrap();

    let hash = hash_file(&file_path).unwrap();
    let expected = hash_bytes(b"");

    assert_eq!(hash, expected);
}

#[test]
fn display_and_from_hex_roundtrip() {
    let hash = hash_bytes(b"roundtrip content");
    let text = hash.to_string();

    assert_eq!(text.len(), 16);

    let restored = ContentHash::from_hex(&text).unwrap();

    assert_eq!(hash, restored);
}

#[test]
fn from_hex_rejects_wrong_length() {
    let result = ContentHash::from_hex("abc");

    assert!(matches!(result, Err(Error::InvalidContentHash { .. })));
}

#[test]
fn from_hex_rejects_non_hex_characters() {
    let result = ContentHash::from_hex("zzzzzzzzzzzzzzzz");

    assert!(matches!(result, Err(Error::InvalidContentHash { .. })));
}

#[test]
fn hash_file_on_missing_file_returns_io_error() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("does-not-exist.txt");

    let result = hash_file(&missing);

    assert!(matches!(result, Err(Error::Io { .. })));
}

#[test]
fn large_file_streaming_matches_in_memory_hash() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("large.bin");

    let mut data = Vec::with_capacity(200 * 1024);
    for i in 0..(200 * 1024) {
        data.push((i % 251) as u8);
    }
    fs::write(&file_path, &data).unwrap();

    let from_file = hash_file(&file_path).unwrap();
    let from_bytes = hash_bytes(&data);

    assert_eq!(from_file, from_bytes);
}
