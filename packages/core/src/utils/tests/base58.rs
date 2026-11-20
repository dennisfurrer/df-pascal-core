use super::*;

#[test]
fn round_trips_keys_and_leading_zeros() {
    for bytes in [vec![], vec![0], vec![0, 0, 1], vec![255; 32], vec![7; 32], (0..64).collect::<Vec<u8>>()] {
        assert_eq!(decode(&encode(&bytes)).unwrap_or_default(), bytes);
    }
    assert_eq!(encode(&[0, 0, 1]), "112");
    assert_eq!(encode(&[0u8; 32]), "11111111111111111111111111111111");
}

#[test]
fn refuses_characters_outside_the_alphabet() {
    for text in ["0", "O", "I", "l", "abc!"] {
        assert!(decode(text).is_err(), "{text}");
    }
    assert_eq!(decode(""), Err(BadBase58::Empty));
    assert!(matches!(decode_array::<32>("2"), Err(BadLength::Length { expected: 32, found: 1 })));
}
