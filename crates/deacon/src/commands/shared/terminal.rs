//! Terminal sizing helpers shared across commands.

use anyhow::Result;
use deacon_core::errors::{ConfigError, DeaconError};

/// Terminal dimensions for output formatting and PTY sizing.
///
/// Both columns and rows must be specified together when present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalDimensions {
    pub columns: u32,
    pub rows: u32,
}

impl TerminalDimensions {
    /// Create terminal dimensions from optional CLI inputs.
    ///
    /// Returns `Ok(None)` when both values are omitted. Errors if only one
    /// dimension is provided or if either dimension is zero.
    pub fn new(columns: Option<u32>, rows: Option<u32>) -> Result<Option<Self>> {
        match (columns, rows) {
            (Some(cols), Some(rows)) => {
                if cols == 0 || rows == 0 {
                    return Err(DeaconError::Config(ConfigError::Validation {
                        message: "terminalColumns and terminalRows must be positive integers"
                            .to_string(),
                    })
                    .into());
                }

                Ok(Some(Self {
                    columns: cols,
                    rows,
                }))
            }
            (None, None) => Ok(None),
            (Some(_), None) => Err(DeaconError::Config(ConfigError::Validation {
                message: "--terminal-columns and --terminal-rows must both be provided".to_string(),
            })
            .into()),
            (None, Some(_)) => Err(DeaconError::Config(ConfigError::Validation {
                message: "--terminal-columns and --terminal-rows must both be provided".to_string(),
            })
            .into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_dimensions_both_specified() {
        let dims = TerminalDimensions::new(Some(80), Some(24)).unwrap();
        assert_eq!(
            dims,
            Some(TerminalDimensions {
                columns: 80,
                rows: 24
            })
        );
    }

    #[test]
    fn test_terminal_dimensions_neither_specified() {
        let dims = TerminalDimensions::new(None, None).unwrap();
        assert!(dims.is_none());
    }

    #[test]
    fn test_terminal_dimensions_only_columns_fails() {
        let result = TerminalDimensions::new(Some(80), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_terminal_dimensions_only_rows_fails() {
        let result = TerminalDimensions::new(None, Some(24));
        assert!(result.is_err());
    }

    #[test]
    fn test_terminal_dimensions_rejects_zero_values() {
        let result = TerminalDimensions::new(Some(0), Some(24));
        assert!(result.is_err());

        let result = TerminalDimensions::new(Some(80), Some(0));
        assert!(result.is_err());
    }
}
