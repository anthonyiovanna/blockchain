# Standards Module Changelog

## [2024-01-09] Added Concurrency Error Types

Added new error variants to ContractError for handling concurrent operations:

1. Concurrency Errors:
   - ConcurrencyLimitExceeded: For when operation limits are exceeded
   - OperationTimeout: For when operations take too long
   - OperationConflict: For when operations conflict with each other

2. Upgrade Limit Errors:
   - UpgradeLimitExceeded: For when upgrade frequency or count limits are exceeded

3. Enhanced Error Checking:
   - Added is_concurrency_error() method
   - Updated is_upgrade_error() to include UpgradeLimitExceeded

These changes support the concurrent operation limits testing by providing proper error handling for concurrency-related issues.
