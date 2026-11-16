mod pb;

use pb::sol::transfers::v1::{Transfer, Transfers};
use std::collections::HashMap;
use substreams_solana::pb::sf::solana::r#type::v1::Block;

const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

fn b58(bytes: &[u8]) -> String {
    bs58::encode(bytes).into_string()
}

fn read_u64_le(data: &[u8], offset: usize) -> Option<u64> {
    let slice = data.get(offset..offset + 8)?;
    Some(u64::from_le_bytes(slice.try_into().ok()?))
}

/// Decode a single instruction (top-level or inner) into a Transfer, if it is one.
///   program_id : base58 program of the instruction
///   acct_idx   : account indexes used by the instruction
///   data       : raw instruction data
///   keys       : resolved account keys of the tx (static + address-lookup-table)
///   owners     : token-account index -> wallet owner (from token balances)
fn decode(
    program_id: &str,
    acct_idx: &[u8],
    data: &[u8],
    keys: &[Vec<u8>],
    owners: &HashMap<u32, String>,
) -> Option<Transfer> {
    let key = |i: usize| -> Option<&Vec<u8>> { keys.get(*acct_idx.get(i)? as usize) };

    // Wallet owner of a token account, falling back to the token account itself
    let token_owner = |i: usize| -> Option<String> {
        let idx = *acct_idx.get(i)? as u32;
        match owners.get(&idx) {
            Some(o) => Some(o.clone()),
            None => Some(b58(keys.get(idx as usize)?)),
        }
    };

    if program_id == SYSTEM_PROGRAM {
        let tag = u32::from_le_bytes(data.get(0..4)?.try_into().ok()?);
        match tag {
            // SystemInstruction::Transfer { lamports }  accounts: [from, to]
            2 => Some(Transfer {
                from: b58(key(0)?),
                to: b58(key(1)?),
                amount: read_u64_le(data, 4)?,
            }),
            // SystemInstruction::TransferWithSeed  accounts: [from, base, to]
            11 => Some(Transfer {
                from: b58(key(0)?),
                to: b58(key(2)?),
                amount: read_u64_le(data, 4)?,
            }),
            _ => None,
        }
    } else if program_id == TOKEN_PROGRAM || program_id == TOKEN_2022_PROGRAM {
        match *data.first()? {
            // Transfer  accounts: [source, destination, authority]
            3 => Some(Transfer {
                from: token_owner(0)?,
                to: token_owner(1)?,
                amount: read_u64_le(data, 1)?,
            }),
            // TransferChecked  accounts: [source, mint, destination, authority]
            12 => Some(Transfer {
                from: token_owner(0)?,
                to: token_owner(2)?,
                amount: read_u64_le(data, 1)?,
            }),
            _ => None,
        }
    } else {
        None
    }
}

#[substreams::handlers::map]
fn map_transfers(block: Block) -> Result<Transfers, substreams::errors::Error> {
    let mut transfers: Vec<Transfer> = vec![];

    // transactions() yields successful transactions only
    for tx in block.transactions() {
        let msg = match tx.transaction.as_ref().and_then(|t| t.message.as_ref()) {
            Some(m) => m,
            None => continue,
        };
        let keys: &Vec<Vec<u8>> = tx.resolved_accounts();

        // token account index -> wallet owner
        let mut owners: HashMap<u32, String> = HashMap::new();
        if let Some(meta) = tx.meta.as_ref() {
            for b in meta.pre_token_balances.iter().chain(meta.post_token_balances.iter()) {
                if !b.owner.is_empty() {
                    owners.insert(b.account_index, b.owner.clone());
                }
            }
        }

        // 1) top-level instructions
        for ix in &msg.instructions {
            if let Some(pk) = keys.get(ix.program_id_index as usize) {
                if let Some(t) = decode(&b58(pk), &ix.accounts, &ix.data, keys, &owners) {
                    transfers.push(t);
                }
            }
        }

        // 2) inner instructions (CPI calls, e.g. swaps / routers)
        if let Some(meta) = tx.meta.as_ref() {
            for inner in &meta.inner_instructions {
                for ix in &inner.instructions {
                    if let Some(pk) = keys.get(ix.program_id_index as usize) {
                        if let Some(t) = decode(&b58(pk), &ix.accounts, &ix.data, keys, &owners) {
                            transfers.push(t);
                        }
                    }
                }
            }
        }
    }

    Ok(Transfers { transfers })
}
