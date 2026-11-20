use super::*;
use crate::utils::sign::Keypair;

#[test]
fn compact_u16_matches_solana() {
    for (value, bytes) in [
        (0usize, vec![0u8]),
        (0x7f, vec![0x7f]),
        (0x80, vec![0x80, 0x01]),
        (0x3fff, vec![0xff, 0x7f]),
        (0x4000, vec![0x80, 0x80, 0x01]),
    ] {
        let mut out = Vec::new();
        compact_u16(&mut out, value);
        assert_eq!(out, bytes, "{value}");
    }
}

#[test]
fn accounts_are_ordered_payer_first_then_by_role() {
    let payer = Keypair::from_seed(&[7; 32]).pubkey();
    let mint = Pubkey::new([1; 32]);
    let source = Pubkey::new([2; 32]);
    let destination = Pubkey::new([3; 32]);
    let message =
        Message::compile(&payer, &[transfer_checked(source, mint, destination, payer, 5, 6)], [9; 32]).unwrap();
    assert_eq!((message.required_signatures, message.readonly_signed, message.readonly_unsigned), (1, 0, 2));
    assert_eq!(message.keys, vec![payer, source, destination, mint, Pubkey::from_literal(TOKEN_PROGRAM)]);
    let instruction = &message.instructions[0];
    assert_eq!((instruction.program, instruction.accounts.clone()), (4, vec![1, 3, 2, 0]));
    assert_eq!(instruction.data, [vec![12], 5u64.to_le_bytes().to_vec(), vec![6]].concat());
}

#[test]
fn a_signed_transaction_carries_the_payer_s_signature_first() {
    let payer = Keypair::from_seed(&[7; 32]);
    let message = Message::compile(&payer.pubkey(), &[set_compute_unit_price(1)], [9; 32]).unwrap();
    let mut transaction = Transaction::new(message);
    assert!(!transaction.is_signed());
    transaction.sign(&payer).unwrap();
    assert!(transaction.is_signed());
    let bytes = transaction.serialize().unwrap();
    assert_eq!(bytes[0], 1);
    assert!(crate::utils::sign::verify(&payer.pubkey(), &transaction.message.serialize(), &transaction.id().unwrap()));
    assert!(transaction.add_signature(&Pubkey::new([5; 32]), &transaction.id().unwrap()).is_err());
}
