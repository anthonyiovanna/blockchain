use blockchain::contract::{
    ContractRuntime, ContractEnvironment, ResourceLimits, ContractABI,
    ContractMethod, ContractParam, ContractMetadata, DEPLOYER_ROLE, EXECUTOR_ROLE, DEFAULT_ADMIN_ROLE,
};
use blockchain::msg;
use wasmer::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

// Test WASM module that implements basic arithmetic operations
const TEST_WASM: &[u8] = include_bytes!("fixtures/test_contract.wasm");

// Test account for all operations
const TEST_ACCOUNT: [u8; 32] = [0u8; 32];  // Match DEFAULT_ADMIN_ROLE
const ADMIN_ACCOUNT: [u8; 32] = [0u8; 32];  // Must match DEFAULT_ADMIN_ROLE

async fn setup_runtime() -> ContractRuntime {
    let mut runtime = ContractRuntime::new();
    
    // Set sender as admin account (which is all zeros)
    msg::test_utils::set_sender(ADMIN_ACCOUNT).unwrap();
    
    // Grant admin role to admin account (this should work as it's the first admin role grant)
    runtime.grant_role(DEFAULT_ADMIN_ROLE, ADMIN_ACCOUNT).unwrap();
    
    // Now we can grant other roles since we're the admin
    runtime.grant_role(DEPLOYER_ROLE, TEST_ACCOUNT).unwrap();
    runtime.grant_role(EXECUTOR_ROLE, TEST_ACCOUNT).unwrap();
    
    // Switch to test account for the actual test
    msg::test_utils::set_sender(TEST_ACCOUNT).unwrap();
    
    runtime
}

#[tokio::test]
async fn test_contract_existence() {
    let mut runtime = setup_runtime().await;
    let contract_addr = [1u8; 32];
    let nonexistent_addr = [2u8; 32];
    
    // Verify contract doesn't exist before deployment
    assert!(!runtime.contract_exists(&contract_addr), "Contract should not exist before deployment");
    assert!(!runtime.contract_exists(&nonexistent_addr), "Non-existent contract should not exist");

    // Deploy a contract
    let abi = ContractABI {
        methods: vec![
            ContractMethod {
                name: "add".into(),
                inputs: vec![
                    ContractParam {
                        name: "a".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                    ContractParam {
                        name: "b".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                outputs: vec![
                    ContractParam {
                        name: "result".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                payable: false,
            },
        ],
        events: vec![],
        standards: vec![],
    };

    let limits = ResourceLimits {
        max_memory: 1024 * 1024,
        max_gas: 1_000_000,
        max_storage: 1024 * 1024,
        max_call_depth: 5,
    };

    let metadata = ContractMetadata {
        version: "1.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567890,
        author: TEST_ACCOUNT,
        description: "Test Contract".into(),
        is_upgradeable: true,
    };

    // Deploy the contract
    let result = runtime.deploy_contract(TEST_WASM, &contract_addr, &abi, metadata, &limits).await;
    assert!(result.is_ok(), "Failed to deploy contract: {:?}", result.err());

    // Verify contract exists after deployment
    assert!(runtime.contract_exists(&contract_addr), "Contract should exist after deployment");
    assert!(!runtime.contract_exists(&nonexistent_addr), "Non-existent contract should still not exist");

    // Try to execute non-existent contract
    let env = ContractEnvironment {
        gas_limit: 1_000_000,
        block_number: 1,
        timestamp: 1234567890,
        caller: TEST_ACCOUNT,
        resource_limits: limits,
        gas_used: Arc::new(RwLock::new(0)),
    };

    let result = runtime.execute_contract(
        nonexistent_addr,
        "add",
        vec![Value::I32(1), Value::I32(2)],
        &env,
        None,
    ).await;

    // Verify execution fails with "Contract not found"
    assert!(result.is_err());
    let err_msg = format!("{}", result.err().unwrap());
    assert!(err_msg.contains("Contract not found"), 
           "Unexpected error message: {}", err_msg);

    // Clean up
    msg::test_utils::clear_sender().unwrap();
}

#[tokio::test]
async fn test_contract_deployment() {
    let mut runtime = setup_runtime().await;
    let contract_addr = [1u8; 32];
    
    let abi = ContractABI {
        methods: vec![
            ContractMethod {
                name: "add".into(),
                inputs: vec![
                    ContractParam {
                        name: "a".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                    ContractParam {
                        name: "b".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                outputs: vec![
                    ContractParam {
                        name: "result".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                payable: false,
            },
        ],
        events: vec![],
        standards: vec![],
    };

    let limits = ResourceLimits {
        max_memory: 1024 * 1024,
        max_gas: 1_000_000,
        max_storage: 1024 * 1024,
        max_call_depth: 5,
    };

    let metadata = ContractMetadata {
        version: "1.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567890,
        author: TEST_ACCOUNT,
        description: "Test Contract".into(),
        is_upgradeable: true,
    };

    let result = runtime.deploy_contract(TEST_WASM, &contract_addr, &abi, metadata, &limits).await;
    assert!(result.is_ok(), "Failed to deploy contract: {:?}", result.err());

    // Clean up
    msg::test_utils::clear_sender().unwrap();
}

#[tokio::test]
async fn test_contract_execution() {
    let mut runtime = setup_runtime().await;
    let contract_addr = [1u8; 32];
    
    // Deploy test contract first
    let abi = ContractABI {
        methods: vec![
            ContractMethod {
                name: "add".into(),
                inputs: vec![
                    ContractParam {
                        name: "a".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                    ContractParam {
                        name: "b".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                outputs: vec![
                    ContractParam {
                        name: "result".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                payable: false,
            },
        ],
        events: vec![],
        standards: vec![],
    };

    let limits = ResourceLimits {
        max_memory: 1024 * 1024,
        max_gas: 1_000_000,
        max_storage: 1024 * 1024,
        max_call_depth: 5,
    };

    let metadata = ContractMetadata {
        version: "1.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567890,
        author: TEST_ACCOUNT,
        description: "Test Contract".into(),
        is_upgradeable: true,
    };

    runtime.deploy_contract(TEST_WASM, &contract_addr, &abi, metadata, &limits).await.unwrap();

    // Create execution environment
    let env = ContractEnvironment {
        gas_limit: 1_000_000,
        block_number: 1,
        timestamp: 1234567890,
        caller: TEST_ACCOUNT,
        resource_limits: limits,
        gas_used: Arc::new(RwLock::new(0)),
    };

    // Execute add method
    let args = vec![Value::I32(5), Value::I32(3)];
    let result = runtime.execute_contract(contract_addr, "add", args, &env, None).await;
    
    assert!(result.is_ok(), "Failed to execute contract: {:?}", result.err());
    let values = result.unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].unwrap_i32(), 8);

    // Clean up
    msg::test_utils::clear_sender().unwrap();
}

#[tokio::test]
async fn test_gas_metering() {
    let mut runtime = setup_runtime().await;
    let contract_addr = [1u8; 32];
    
    // Deploy test contract
    let abi = ContractABI {
        methods: vec![
            ContractMethod {
                name: "loop_test".into(),
                inputs: vec![
                    ContractParam {
                        name: "iterations".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                outputs: vec![],
                payable: false,
            },
        ],
        events: vec![],
        standards: vec![],
    };

    let limits = ResourceLimits {
        max_memory: 1024 * 1024,
        max_gas: 1_000,  // Very low gas limit
        max_storage: 1024 * 1024,
        max_call_depth: 5,
    };

    let metadata = ContractMetadata {
        version: "1.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567890,
        author: TEST_ACCOUNT,
        description: "Test Contract".into(),
        is_upgradeable: true,
    };

    runtime.deploy_contract(TEST_WASM, &contract_addr, &abi, metadata, &limits).await.unwrap();

    // Create execution environment with low gas limit
    let env = ContractEnvironment {
        gas_limit: 1_000,
        block_number: 1,
        timestamp: 1234567890,
        caller: TEST_ACCOUNT,
        resource_limits: limits,
        gas_used: Arc::new(RwLock::new(0)),
    };

    // Execute loop_test method with high iteration count
    let args = vec![Value::I32(1000000)];  // Large number of iterations
    let result = runtime.execute_contract(contract_addr, "loop_test", args, &env, None).await;
    
    // Should fail due to gas limit exceeded
    assert!(result.is_err());
    assert!(format!("{}", result.err().unwrap()).contains("Gas limit exceeded"));

    // Clean up
    msg::test_utils::clear_sender().unwrap();
}

#[tokio::test]
async fn test_error_handling() {
    let mut runtime = setup_runtime().await;
    let contract_addr = [1u8; 32];
    let different_addr = [2u8; 32];  // Different address than what we'll deploy to
    
    // First deploy a valid contract as the privileged user
    let abi = ContractABI {
        methods: vec![
            ContractMethod {
                name: "add".into(),
                inputs: vec![
                    ContractParam {
                        name: "a".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                    ContractParam {
                        name: "b".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                outputs: vec![
                    ContractParam {
                        name: "result".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                payable: false,
            },
        ],
        events: vec![],
        standards: vec![],
    };

    let limits = ResourceLimits {
        max_memory: 1024 * 1024,
        max_gas: 1_000_000,
        max_storage: 1024 * 1024,
        max_call_depth: 5,
    };

    let metadata = ContractMetadata {
        version: "1.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567890,
        author: TEST_ACCOUNT,
        description: "Test Contract".into(),
        is_upgradeable: true,
    };

    runtime.deploy_contract(TEST_WASM, &contract_addr, &abi, metadata, &limits).await.unwrap();
    
    // Create a new account that doesn't have any roles
    let unprivileged_account = [3u8; 32];
    
    // Switch to admin account to verify unprivileged account has no roles
    msg::test_utils::set_sender(ADMIN_ACCOUNT).unwrap();
    assert!(!runtime.has_role(EXECUTOR_ROLE, &unprivileged_account), "Unprivileged account should not have executor role");
    
    // Try to execute a different contract address with unprivileged account
    msg::test_utils::set_sender(unprivileged_account).unwrap();
    
    let env = ContractEnvironment {
        gas_limit: 1_000_000,
        block_number: 1,
        timestamp: 1234567890,
        caller: unprivileged_account,
        resource_limits: ResourceLimits {
            max_memory: 1024 * 1024,
            max_gas: 1_000_000,
            max_storage: 1024 * 1024,
            max_call_depth: 5,
        },
        gas_used: Arc::new(RwLock::new(0)),
    };

    let result = runtime.execute_contract(
        different_addr,  // Use different address than what we deployed
        "add",
        vec![Value::I32(1), Value::I32(2)],
        &env,
        None,
    ).await;
    
    assert!(result.is_err());
    let err_msg = format!("{}", result.err().unwrap());
    assert!(err_msg.contains("Access denied:"), 
           "Unexpected error message: {}", err_msg);

    // Deploy contract with invalid WASM
    // Switch back to privileged account for deployment
    msg::test_utils::set_sender(TEST_ACCOUNT).unwrap();
    
    let invalid_wasm = &[0, 1, 2, 3];  // Invalid WASM bytes
    let abi = ContractABI {
        methods: vec![],
        events: vec![],
        standards: vec![],
    };

    let metadata = ContractMetadata {
        version: "1.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567890,
        author: TEST_ACCOUNT,
        description: "Invalid Contract".into(),
        is_upgradeable: true,
    };

    let result = runtime.deploy_contract(invalid_wasm, &contract_addr, &abi, metadata, &limits).await;
    assert!(result.is_err());
    assert!(format!("{}", result.err().unwrap()).contains("Compilation"));

    // Clean up
    msg::test_utils::clear_sender().unwrap();
}
