// Fixed Supply Token Program for Solana
// This program creates a token with a fixed total supply and permanently revokes minting authority
// to ensure no additional tokens can ever be created after the initial mint.

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint, entrypoint::ProgramResult,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
    program_option::COption,
};
use spl_token::{
    instruction::{mint_to, set_authority},
    state::{Mint, Account as TokenAccount},
    instruction::AuthorityType,
};

// Token configuration constants
// Total supply: 500 million tokens
const TOTAL_SUPPLY: u64 = 500_000_000;
// Token decimals: 9 (standard for most Solana tokens)
const DECIMALS: u8 = 9;

/// Custom error codes for specific validation failures
#[derive(Debug, Copy, Clone)]
pub enum CustomError {
    InvalidMintState = 6000,        // Mint account is not in the expected initial state
    TokenAccountNotEmpty,           // Token account already contains tokens or has delegates
    MintAuthorityNotRevoked,       // Mint authority was not successfully revoked
    TokenAccountOwnerMismatch,     // Token account owner doesn't match the payer
    MintAuthorityMismatch,         // Mint authority doesn't match the payer
}

impl From<CustomError> for ProgramError {
    fn from(e: CustomError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

/// Main program instruction processor
/// This function performs the following operations:
/// 1. Validates all input accounts and their states
/// 2. Mints the total supply to the specified token account
/// 3. Permanently revokes the mint authority to prevent future minting
/// 4. Verifies the mint authority was successfully revoked
pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> ProgramResult {
    // Calculate total supply with decimals (500M * 10^9)
    let total_supply_with_decimals = TOTAL_SUPPLY
        .checked_mul(10u64.pow(DECIMALS as u32))
        .ok_or(ProgramError::InvalidArgument)?;

    // Extract required accounts from the instruction
    let accounts_iter = &mut accounts.iter();
    let mint_account = next_account_info(accounts_iter)?;      // The token mint account
    let token_account = next_account_info(accounts_iter)?;     // The destination token account
    let mint_authority = next_account_info(accounts_iter)?;    // The mint authority (must sign)
    let payer = next_account_info(accounts_iter)?;             // The transaction payer (must sign)
    let token_program = next_account_info(accounts_iter)?;     // SPL Token program

    // Verify that required signers have signed the transaction
    if !mint_authority.is_signer || !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Ensure mint authority and payer are the same account for security
    if mint_authority.key != payer.key {
        return Err(CustomError::MintAuthorityMismatch.into());
    }

    // Verify that mint and token accounts are owned by the SPL Token program
    if *mint_account.owner != spl_token::id() || *token_account.owner != spl_token::id() {
        return Err(ProgramError::IllegalOwner);
    }

    // Ensure accounts are writable for the operations we need to perform
    if !mint_account.is_writable || !token_account.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }

    // Validate mint account state before minting
    // The mint must be initialized with correct decimals, zero supply, no freeze authority,
    // and the mint authority must match the provided authority
    let mint_data = Mint::unpack(&mint_account.data.borrow())?;
    if !(mint_data.is_initialized
        && mint_data.decimals == DECIMALS
        && mint_data.supply == 0
        && mint_data.freeze_authority.is_none()
        && mint_data.mint_authority == COption::Some(*mint_authority.key))
    {
        return Err(CustomError::InvalidMintState.into());
    }
    
    // Validate token account state before minting
    // The token account must be owned by the payer, associated with the correct mint,
    // have zero balance, and no delegates or close authority
    let token_data = TokenAccount::unpack(&token_account.data.borrow())?;
    if token_data.owner != *payer.key {
        return Err(CustomError::TokenAccountOwnerMismatch.into());
    }
    if !(token_data.mint == *mint_account.key
        && token_data.amount == 0
        && token_data.delegate.is_none()
        && token_data.close_authority.is_none())
    {
        return Err(CustomError::TokenAccountNotEmpty.into());
    }

    // Step 1: Mint the total supply to the token account
    // This creates all tokens that will ever exist for this mint
    solana_program::program::invoke(
        &mint_to(
            token_program.key,
            mint_account.key,
            token_account.key,
            mint_authority.key,
            &[],
            total_supply_with_decimals,
        )?,
        &[
            mint_account.clone(),
            token_account.clone(),
            mint_authority.clone(),
            token_program.clone(),
        ],
    )?;

    // Step 2: Permanently revoke the mint authority
    // This ensures no additional tokens can ever be minted, making the supply truly fixed
    solana_program::program::invoke(
        &set_authority(
            token_program.key,
            mint_account.key,
            None,  // Set authority to None (revoked)
            AuthorityType::MintTokens,
            mint_authority.key,
            &[mint_authority.key],
        )?,
        &[
            mint_account.clone(),
            mint_authority.clone(),
            token_program.clone(),
        ],
    )?;

    // Step 3: Final verification - ensure mint authority was successfully revoked
    // This is a critical security check to confirm the token supply is now permanently fixed
    let final_mint_data = Mint::unpack(&mint_account.data.borrow())?;
    if final_mint_data.mint_authority.is_some() {
        return Err(CustomError::MintAuthorityNotRevoked.into());
    }

    Ok(())
}

// Program entrypoint - required for all Solana programs
entrypoint!(process_instruction);
