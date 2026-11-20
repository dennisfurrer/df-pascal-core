//! Legacy Solana transaction building and serialization, so a USDC deposit needs no JavaScript
//! library. Wire layout: a compact-u16 signature count and 64-byte signatures, then the message
//! (3-byte header, account keys ordered writable signers, read-only signers, writable non-signers,
//! read-only non-signers with the fee payer first, recent blockhash, instructions).

use crate::utils::keys::{Pubkey, Signature};
use crate::utils::solana::{ASSOCIATED_TOKEN_PROGRAM, COMPUTE_BUDGET_PROGRAM, SYSTEM_PROGRAM, TOKEN_PROGRAM};

/// The largest serialized transaction the network takes.
pub const MAX_TRANSACTION_BYTES: usize = 1232;

/// One account an instruction touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountMeta {
    pub pubkey: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
}

impl AccountMeta {
    #[must_use]
    pub const fn writable(pubkey: Pubkey, is_signer: bool) -> AccountMeta {
        AccountMeta { pubkey, is_signer, is_writable: true }
    }

    #[must_use]
    pub const fn readonly(pubkey: Pubkey, is_signer: bool) -> AccountMeta {
        AccountMeta { pubkey, is_signer, is_writable: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub program: Pubkey,
    pub accounts: Vec<AccountMeta>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadTransaction {
    #[error("a transaction is at most {MAX_TRANSACTION_BYTES} bytes; this one is {0}")]
    TooLarge(usize),
    #[error("at most 256 accounts")]
    TooManyAccounts,
    #[error("{0} must sign but is not among the signatures")]
    MissingSigner(Pubkey),
    #[error("{0} is not a signer of this transaction")]
    NotASigner(Pubkey),
}

/// A compiled transaction message, signed by every signer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub required_signatures: u8,
    pub readonly_signed: u8,
    pub readonly_unsigned: u8,
    pub keys: Vec<Pubkey>,
    pub blockhash: [u8; 32],
    pub instructions: Vec<CompiledInstruction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledInstruction {
    pub program: u8,
    pub accounts: Vec<u8>,
    pub data: Vec<u8>,
}

impl Message {
    /// Orders the accounts, `payer` first, and indexes each instruction against them.
    pub fn compile(
        payer: &Pubkey,
        instructions: &[Instruction],
        blockhash: [u8; 32],
    ) -> Result<Message, BadTransaction> {
        // Deduplicate keys, merging signer and writable flags.
        let mut metas: Vec<AccountMeta> = vec![AccountMeta::writable(*payer, true)];
        let mut merge = |meta: AccountMeta| match metas.iter_mut().find(|known| known.pubkey == meta.pubkey) {
            Some(known) => {
                known.is_signer |= meta.is_signer;
                known.is_writable |= meta.is_writable;
            }
            None => metas.push(meta),
        };
        for instruction in instructions {
            for meta in &instruction.accounts {
                merge(*meta);
            }
            merge(AccountMeta::readonly(instruction.program, false));
        }
        let class = |meta: &AccountMeta| match (meta.is_signer, meta.is_writable) {
            (true, true) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 3,
        };
        // Stable sort keeps the payer first among writable signers.
        metas.sort_by_key(class);
        if metas.len() > 256 {
            return Err(BadTransaction::TooManyAccounts);
        }
        let count = |wanted: u8| metas.iter().filter(|meta| class(meta) == wanted).count();
        let narrow = |value: usize| u8::try_from(value).map_err(|_| BadTransaction::TooManyAccounts);
        let required_signatures = narrow(count(0) + count(1))?;
        let readonly_signed = narrow(count(1))?;
        let readonly_unsigned = narrow(count(3))?;
        let keys: Vec<Pubkey> = metas.iter().map(|meta| meta.pubkey).collect();
        let index = |key: &Pubkey| {
            keys.iter().position(|known| known == key).and_then(|at| u8::try_from(at).ok()).unwrap_or(u8::MAX)
        };
        let instructions = instructions
            .iter()
            .map(|instruction| CompiledInstruction {
                program: index(&instruction.program),
                accounts: instruction.accounts.iter().map(|meta| index(&meta.pubkey)).collect(),
                data: instruction.data.clone(),
            })
            .collect();
        Ok(Message { required_signatures, readonly_signed, readonly_unsigned, keys, blockhash, instructions })
    }

    /// Serialized message bytes, which each signer signs.
    #[must_use]
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = vec![self.required_signatures, self.readonly_signed, self.readonly_unsigned];
        compact_u16(&mut out, self.keys.len());
        for key in &self.keys {
            out.extend_from_slice(key.as_bytes());
        }
        out.extend_from_slice(&self.blockhash);
        compact_u16(&mut out, self.instructions.len());
        for instruction in &self.instructions {
            out.push(instruction.program);
            compact_u16(&mut out, instruction.accounts.len());
            out.extend_from_slice(&instruction.accounts);
            compact_u16(&mut out, instruction.data.len());
            out.extend_from_slice(&instruction.data);
        }
        out
    }

    /// The keys that must sign, in signature order.
    #[must_use]
    pub fn signers(&self) -> &[Pubkey] {
        &self.keys[..usize::from(self.required_signatures).min(self.keys.len())]
    }
}

/// A message and one signature slot per required signer; unsigned slots are zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub signatures: Vec<[u8; 64]>,
    pub message: Message,
}

impl Transaction {
    #[must_use]
    pub fn new(message: Message) -> Transaction {
        Transaction { signatures: vec![[0; 64]; usize::from(message.required_signatures)], message }
    }

    /// Puts `signature` in `signer`'s slot.
    pub fn add_signature(&mut self, signer: &Pubkey, signature: &Signature) -> Result<(), BadTransaction> {
        let at =
            self.message.signers().iter().position(|key| key == signer).ok_or(BadTransaction::NotASigner(*signer))?;
        if let Some(slot) = self.signatures.get_mut(at) {
            *slot = signature.to_bytes();
        }
        Ok(())
    }

    /// Signs with `signer` in its slot.
    #[cfg(feature = "sign")]
    pub fn sign(&mut self, signer: &crate::utils::sign::Keypair) -> Result<(), BadTransaction> {
        let signature = signer.sign(&self.message.serialize());
        self.add_signature(&signer.pubkey(), &signature)
    }

    /// Whether every slot holds a signature.
    #[must_use]
    pub fn is_signed(&self) -> bool {
        self.signatures.iter().all(|signature| signature.iter().any(|byte| *byte != 0))
    }

    /// The first signature: the transaction's id once sent.
    #[must_use]
    pub fn id(&self) -> Option<Signature> {
        self.signatures
            .first()
            .filter(|signature| signature.iter().any(|byte| *byte != 0))
            .map(|bytes| Signature::new(*bytes))
    }

    /// The wire bytes; unsigned slots are zeros for a wallet to fill.
    pub fn serialize(&self) -> Result<Vec<u8>, BadTransaction> {
        let mut out = Vec::new();
        compact_u16(&mut out, self.signatures.len());
        for signature in &self.signatures {
            out.extend_from_slice(signature);
        }
        out.extend(self.message.serialize());
        match out.len() {
            length if length > MAX_TRANSACTION_BYTES => Err(BadTransaction::TooLarge(length)),
            _ => Ok(out),
        }
    }
}

/// Solana compact-u16: 7 bits per byte, least significant first, high bit set if more follow.
pub fn compact_u16(out: &mut Vec<u8>, value: usize) {
    let mut rest = value.min(usize::from(u16::MAX));
    loop {
        let low = (rest & 0x7f) as u8;
        rest >>= 7;
        if rest == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

/// SPL Token `TransferChecked`: `amount` base units from `source` to `destination`, both
/// accounts of `mint`, signed by `authority`. The mint's decimals are checked by the program.
#[must_use]
pub fn transfer_checked(
    source: Pubkey,
    mint: Pubkey,
    destination: Pubkey,
    authority: Pubkey,
    amount: u64,
    decimals: u8,
) -> Instruction {
    let mut data = vec![12];
    data.extend_from_slice(&amount.to_le_bytes());
    data.push(decimals);
    Instruction {
        program: Pubkey::from_literal(TOKEN_PROGRAM),
        accounts: vec![
            AccountMeta::writable(source, false),
            AccountMeta::readonly(mint, false),
            AccountMeta::writable(destination, false),
            AccountMeta::readonly(authority, true),
        ],
        data,
    }
}

/// Associated Token Account `CreateIdempotent`: creates `wallet`'s account for `mint` if it is
/// missing, and does nothing when it exists. `payer` pays the rent (about 0.00204 SOL).
#[must_use]
pub fn create_associated_token_account_idempotent(
    payer: Pubkey,
    account: Pubkey,
    wallet: Pubkey,
    mint: Pubkey,
) -> Instruction {
    Instruction {
        program: Pubkey::from_literal(ASSOCIATED_TOKEN_PROGRAM),
        accounts: vec![
            AccountMeta::writable(payer, true),
            AccountMeta::writable(account, false),
            AccountMeta::readonly(wallet, false),
            AccountMeta::readonly(mint, false),
            AccountMeta::readonly(Pubkey::from_literal(SYSTEM_PROGRAM), false),
            AccountMeta::readonly(Pubkey::from_literal(TOKEN_PROGRAM), false),
        ],
        data: vec![1],
    }
}

/// Compute Budget `SetComputeUnitLimit`.
#[must_use]
pub fn set_compute_unit_limit(units: u32) -> Instruction {
    let mut data = vec![2];
    data.extend_from_slice(&units.to_le_bytes());
    Instruction { program: Pubkey::from_literal(COMPUTE_BUDGET_PROGRAM), accounts: Vec::new(), data }
}

/// Compute Budget `SetComputeUnitPrice`, in micro-lamports per compute unit: the priority fee.
#[must_use]
pub fn set_compute_unit_price(micro_lamports: u64) -> Instruction {
    let mut data = vec![3];
    data.extend_from_slice(&micro_lamports.to_le_bytes());
    Instruction { program: Pubkey::from_literal(COMPUTE_BUDGET_PROGRAM), accounts: Vec::new(), data }
}

#[cfg(test)]
#[cfg(feature = "sign")]
#[path = "tests/tx.rs"]
mod tests;
