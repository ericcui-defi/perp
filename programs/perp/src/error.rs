use anchor_lang::prelude::*;

#[error_code]
pub enum PerpError {

    #[msg("Oracle account does not match market")]
    WrongOracle,

    #[msg("Vault account does not match market")]
    WrongVault,

    #[msg("Position would be bankrupt at close")]
    Bankrupt,

    #[msg("Position size must be nonzero")]
    ZeroPositionSize,

    #[msg("Collateral amount must be nonzero")]
    ZeroCollateral,

    #[msg("Incorrect field value")]
    IncorrectFieldValue,
}
