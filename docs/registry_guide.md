# Smart Contract Registry Usage Guide

## Table of Contents
1. [Contract Registration](#contract-registration)
2. [Version Management](#version-management)
3. [Search and Lookup](#search-and-lookup)
4. [Metadata Management](#metadata-management)
5. [State Management](#state-management)
6. [Best Practices](#best-practices)

## Contract Registration

### Prerequisites
- Contract bytecode must be compiled and validated
- Deployer role must be granted to the registering account
- Contract metadata must be properly formatted
- Resource limits must be defined

### Registration Process

1. **Prepare Contract Metadata**
   ```rust
   let metadata = ContractMetadata {
       version: "1.0.0".to_string(),
       created_at: SystemTime::now()
           .duration_since(UNIX_EPOCH)
           .unwrap()
           .as_secs(),
       updated_at: SystemTime::now()
           .duration_since(UNIX_EPOCH)
           .unwrap()
           .as_secs(),
       author: [/* author address */],
       description: "Contract description".to_string(),
       is_upgradeable: true,
   };
   ```

2. **Define Resource Limits**
   ```rust
   let limits = ResourceLimits {
       max_memory: 1024 * 1024,  // 1MB
       max_gas: 1_000_000,
       max_storage: 1024 * 1024, // 1MB
       max_call_depth: 10,
   };
   ```

3. **Register Contract**
   ```rust
   let contract_addr = [/* contract address */];
   runtime.deploy_contract(
       &bytecode,
       &contract_addr,
       &abi,
       metadata,
       &limits,
   ).await?;
   ```

### Post-Registration Verification
1. Verify contract existence
2. Check state initialization
3. Validate access controls
4. Test basic functionality

## Version Management

### Version Creation

1. **Semantic Versioning**
   - Major version (x.0.0): Breaking changes
   - Minor version (0.x.0): New features (backward compatible)
   - Patch version (0.0.x): Bug fixes

2. **Version Upgrade Process**
   ```rust
   let new_metadata = ContractMetadata {
       version: "1.1.0".to_string(),
       // ... other metadata fields
   };

   runtime.upgrade_contract(
       &contract_addr,
       &new_bytecode,
       &new_abi,
       new_metadata,
   ).await?;
   ```

### Version History Management

1. **List Versions**
   ```rust
   let versions = runtime.get_contract_versions(&contract_addr)?;
   ```

2. **Get Specific Version**
   ```rust
   let version = runtime.get_contract_version(&contract_addr, "1.0.0")?;
   ```

3. **Get Latest Version**
   ```rust
   let latest = runtime.get_latest_version(&contract_addr)?;
   ```

### Version Rollback

1. **Initiate Rollback**
   ```rust
   runtime.rollback_contract(&contract_addr).await?;
   ```

2. **Verify Rollback**
   - Check contract version
   - Verify state consistency
   - Test functionality

## Search and Lookup

### Contract Lookup

1. **By Address**
   ```rust
   if runtime.contract_exists(&contract_addr) {
       let contract = runtime.get_latest_version(&contract_addr)?;
       // Process contract
   }
   ```

2. **By Description**
   ```rust
   let matches = runtime.search_by_description("token contract");
   for (addr, contract) in matches {
       // Process matching contracts
   }
   ```

3. **List All Contracts**
   ```rust
   let all_contracts = runtime.list_all_contracts();
   for (addr, contract) in all_contracts {
       // Process each contract
   }
   ```

### Search Optimization

1. **Efficient Searching**
   - Use specific search terms
   - Filter results as needed
   - Cache frequently accessed contracts

2. **Performance Considerations**
   - Limit search results
   - Use pagination for large results
   - Index important fields

## Metadata Management

### Required Metadata Fields

1. **Version Information**
   - Semantic version number
   - Creation timestamp
   - Update timestamp

2. **Author Information**
   - Author address
   - Description
   - Upgrade permission flag

### Metadata Updates

1. **Update Process**
   ```rust
   let updated_metadata = ContractMetadata {
       version: current_version.clone(),
       updated_at: SystemTime::now()
           .duration_since(UNIX_EPOCH)
           .unwrap()
           .as_secs(),
       // ... other fields
   };
   ```

2. **Validation Rules**
   - Version format validation
   - Timestamp validation
   - Description length limits
   - Author address validation

## State Management

### State Operations

1. **Read State**
   ```rust
   if let Some(state) = runtime.get_contract_state(&contract_addr) {
       // Process state
   }
   ```

2. **Update State**
   ```rust
   runtime.update_contract_state(
       contract_addr,
       key.as_bytes().to_vec(),
       value.as_bytes().to_vec(),
   ).await?;
   ```

3. **State Snapshots**
   ```rust
   if let Some(snapshots) = runtime.get_state_snapshots(&contract_addr) {
       // Process snapshots
   }
   ```

### State Validation

1. **Integrity Checks**
   - Verify state consistency
   - Validate state size
   - Check state permissions

2. **State Recovery**
   - Create regular snapshots
   - Implement recovery procedures
   - Validate recovered state

## Best Practices

### Security

1. **Access Control**
   - Implement role-based access
   - Validate all operations
   - Monitor access patterns

2. **Version Control**
   - Use semantic versioning
   - Document changes
   - Test thoroughly before upgrade

3. **State Management**
   - Regular state backups
   - Validate state changes
   - Monitor state size

### Performance

1. **Operation Optimization**
   - Batch operations when possible
   - Cache frequent lookups
   - Clean up old data

2. **Resource Management**
   - Monitor gas usage
   - Control memory usage
   - Manage storage efficiently

### Monitoring

1. **Metrics Tracking**
   ```rust
   let active_ops = runtime.get_active_operations();
   let ops_per_sec = runtime.get_operations_per_second();
   ```

2. **Health Checks**
   - Regular state validation
   - Performance monitoring
   - Resource usage tracking

### Error Handling

1. **Common Errors**
   - Version conflicts
   - State validation failures
   - Permission denied
   - Resource limits exceeded

2. **Recovery Procedures**
   - Error logging
   - State recovery
   - Version rollback
   - Operation retry logic

## Appendix

### Common Patterns

1. **Contract Deployment**
   ```rust
   // 1. Prepare metadata
   let metadata = ContractMetadata { /* ... */ };
   
   // 2. Set resource limits
   let limits = ResourceLimits { /* ... */ };
   
   // 3. Deploy contract
   runtime.deploy_contract(
       &bytecode,
       &contract_addr,
       &abi,
       metadata,
       &limits,
   ).await?;
   
   // 4. Verify deployment
   assert!(runtime.contract_exists(&contract_addr));
   ```

2. **Version Management**
   ```rust
   // 1. Check current version
   let current = runtime.get_latest_version(&contract_addr)?;
   
   // 2. Prepare upgrade
   let new_metadata = ContractMetadata { /* ... */ };
   
   // 3. Perform upgrade
   runtime.upgrade_contract(
       &contract_addr,
       &new_bytecode,
       &new_abi,
       new_metadata,
   ).await?;
   
   // 4. Verify upgrade
   let upgraded = runtime.get_latest_version(&contract_addr)?;
   assert_ne!(current.metadata.version, upgraded.metadata.version);
   ```

### Troubleshooting Guide

1. **Version Conflicts**
   - Check version numbers
   - Verify upgrade permissions
   - Review change history

2. **State Issues**
   - Validate state integrity
   - Check state size limits
   - Review recent changes

3. **Performance Problems**
   - Monitor operation counts
   - Check resource usage
   - Review access patterns
