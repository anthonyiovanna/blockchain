# Change Log for contract_test.rs

## [2024-01-09] Updated Contract Tests

### Added
- Added ContractMetadata parameter to all deploy_contract calls
- Added metadata fields:
  * version: "1.0.0"
  * created_at/updated_at timestamps
  * author: TEST_ACCOUNT
  * description: Test-specific descriptions
  * is_upgradeable: true

### Modified
- Updated all test cases to use the new ContractMetadata parameter
- Ensured consistent metadata across all test deployments
- Maintained existing test functionality while adapting to new interface

### Test Coverage
- Contract existence verification
- Basic contract deployment
- Contract execution with parameters
- Gas metering and limits
- Error handling scenarios:
  * Access control
  * Invalid WASM
  * Non-existent contracts
