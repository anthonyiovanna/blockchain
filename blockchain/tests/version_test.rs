use blockchain::contract::{
    ContractRuntime, ContractEnvironment, ResourceLimits, ContractABI,
    ContractMethod, ContractParam, ContractMetadata, DEPLOYER_ROLE, EXECUTOR_ROLE, 
    DEFAULT_ADMIN_ROLE, UPGRADER_ROLE,
};
use blockchain::msg;
use wasmer::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

// Test WASM modules for different versions
const TEST_WASM_V1: &[u8] = include_bytes!("fixtures/test_contract.wasm");
const TEST_WASM_V2: &[u8] = include_bytes!("fixtures/test_contract_v2/target/wasm32-unknown-unknown/release/test_contract_v2.wasm");

// Test accounts
const TEST_ACCOUNT: [u8; 32] = [0u8; 32];
const ADMIN_ACCOUNT: [u8; 32] = [0u8; 32];
const UPGRADER_ACCOUNT: [u8; 32] = [1u8; 32];

async fn setup_runtime() -> ContractRuntime {
    let mut runtime = ContractRuntime::new();
    
    // Set sender as admin account
    msg::test_utils::set_sender(ADMIN_ACCOUNT).unwrap();
    
    // Grant admin role
    runtime.grant_role(DEFAULT_ADMIN_ROLE, ADMIN_ACCOUNT).unwrap();
    
    // Grant roles to test account
    runtime.grant_role(DEPLOYER_ROLE, TEST_ACCOUNT).unwrap();
    runtime.grant_role(EXECUTOR_ROLE, TEST_ACCOUNT).unwrap();
    
    // Grant upgrader role to upgrader account
    runtime.grant_role(UPGRADER_ROLE, UPGRADER_ACCOUNT).unwrap();
    
    // Switch to test account
    msg::test_utils::set_sender(TEST_ACCOUNT).unwrap();
    
    runtime
}

#[tokio::test]
async fn test_version_upgrade_authorization() {
    let mut runtime = setup_runtime().await;
    let contract_addr = [1u8; 32];
    
    // Deploy initial version
    let abi_v1 = ContractABI {
        methods: vec![
            ContractMethod {
                name: "get_version".into(),
                inputs: vec![],
                outputs: vec![
                    ContractParam {
                        name: "version".into(),
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

    let metadata_v1 = ContractMetadata {
        version: "1.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567890,
        author: TEST_ACCOUNT,
        description: "Test Contract V1".into(),
        is_upgradeable: true,
    };

    // Deploy v1
    runtime.deploy_contract(TEST_WASM_V1, &contract_addr, &abi_v1, metadata_v1, &limits).await.unwrap();

    let metadata_v2 = ContractMetadata {
        version: "2.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567891,
        author: UPGRADER_ACCOUNT,
        description: "Test Contract V2".into(),
        is_upgradeable: true,
    };

    // Try to upgrade without proper role (using test account)
    let result = runtime.deploy_contract(TEST_WASM_V2, &contract_addr, &abi_v1, metadata_v2.clone(), &limits).await;
    assert!(result.is_err());
    assert!(format!("{}", result.err().unwrap()).contains("Version conflict"));

    // Switch to upgrader account
    msg::test_utils::set_sender(UPGRADER_ACCOUNT).unwrap();

    // Upgrade should succeed with proper role
    let result = runtime.deploy_contract(TEST_WASM_V2, &contract_addr, &abi_v1, metadata_v2, &limits).await;
    assert!(result.is_ok(), "Failed to upgrade contract: {:?}", result.err());

    // Clean up
    msg::test_utils::clear_sender().unwrap();
}

#[tokio::test]
async fn test_version_compatibility() {
    let mut runtime = setup_runtime().await;
    let contract_addr = [1u8; 32];
    
    // Deploy initial version
    let abi_v1 = ContractABI {
        methods: vec![
            ContractMethod {
                name: "store_value".into(),
                inputs: vec![
                    ContractParam {
                        name: "value".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                outputs: vec![],
                payable: false,
            },
            ContractMethod {
                name: "get_value".into(),
                inputs: vec![],
                outputs: vec![
                    ContractParam {
                        name: "value".into(),
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

    let metadata_v1 = ContractMetadata {
        version: "1.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567890,
        author: TEST_ACCOUNT,
        description: "Test Contract V1".into(),
        is_upgradeable: true,
    };

    // Deploy v1
    runtime.deploy_contract(TEST_WASM_V1, &contract_addr, &abi_v1, metadata_v1, &limits).await.unwrap();

    // Store initial value
    let env = ContractEnvironment {
        gas_limit: 1_000_000,
        block_number: 1,
        timestamp: 1234567890,
        caller: TEST_ACCOUNT,
        resource_limits: limits.clone(),
        gas_used: Arc::new(RwLock::new(0)),
    };

    runtime.execute_contract(
        contract_addr,
        "store_value",
        vec![Value::I32(42)],
        &env,
        None,
    ).await.unwrap();

    // Switch to upgrader account
    msg::test_utils::set_sender(UPGRADER_ACCOUNT).unwrap();

    // Try to upgrade with incompatible ABI (missing method)
    let incompatible_abi = ContractABI {
        methods: vec![
            ContractMethod {
                name: "get_value".into(),
                inputs: vec![],
                outputs: vec![
                    ContractParam {
                        name: "value".into(),
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

    let metadata_v2 = ContractMetadata {
        version: "2.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567891,
        author: UPGRADER_ACCOUNT,
        description: "Test Contract V2".into(),
        is_upgradeable: true,
    };

    let result = runtime.deploy_contract(TEST_WASM_V2, &contract_addr, &incompatible_abi, metadata_v2, &limits).await;
    assert!(result.is_err());
    assert!(format!("{}", result.err().unwrap()).contains("Version conflict"));

    // Clean up
    msg::test_utils::clear_sender().unwrap();
}

#[tokio::test]
async fn test_state_persistence() {
    let mut runtime = setup_runtime().await;
    let contract_addr = [1u8; 32];
    
    // Deploy initial version
    let abi = ContractABI {
        methods: vec![
            ContractMethod {
                name: "store_value".into(),
                inputs: vec![
                    ContractParam {
                        name: "value".into(),
                        param_type: "i32".into(),
                        indexed: false,
                    },
                ],
                outputs: vec![],
                payable: false,
            },
            ContractMethod {
                name: "get_value".into(),
                inputs: vec![],
                outputs: vec![
                    ContractParam {
                        name: "value".into(),
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

    let metadata_v1 = ContractMetadata {
        version: "1.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567890,
        author: TEST_ACCOUNT,
        description: "Test Contract V1".into(),
        is_upgradeable: true,
    };

    // Deploy v1
    runtime.deploy_contract(TEST_WASM_V1, &contract_addr, &abi, metadata_v1, &limits).await.unwrap();

    let env = ContractEnvironment {
        gas_limit: 1_000_000,
        block_number: 1,
        timestamp: 1234567890,
        caller: TEST_ACCOUNT,
        resource_limits: limits.clone(),
        gas_used: Arc::new(RwLock::new(0)),
    };

    // Store initial value
    runtime.execute_contract(
        contract_addr,
        "store_value",
        vec![Value::I32(42)],
        &env,
        None,
    ).await.unwrap();

    // Switch to upgrader account
    msg::test_utils::set_sender(UPGRADER_ACCOUNT).unwrap();

    let metadata_v2 = ContractMetadata {
        version: "2.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567891,
        author: UPGRADER_ACCOUNT,
        description: "Test Contract V2".into(),
        is_upgradeable: true,
    };

    // Upgrade to v2
    runtime.deploy_contract(TEST_WASM_V2, &contract_addr, &abi, metadata_v2, &limits).await.unwrap();

    // Switch back to test account
    msg::test_utils::set_sender(TEST_ACCOUNT).unwrap();

    // Verify state persisted
    let result = runtime.execute_contract(
        contract_addr,
        "get_value",
        vec![],
        &env,
        None,
    ).await.unwrap();

    assert_eq!(result[0].unwrap_i32(), 42);

    // Clean up
    msg::test_utils::clear_sender().unwrap();
}

#[tokio::test]
async fn test_rollback_functionality() {
    let mut runtime = setup_runtime().await;
    let contract_addr = [1u8; 32];
    
    // Deploy initial version
    let abi = ContractABI {
        methods: vec![
            ContractMethod {
                name: "get_version".into(),
                inputs: vec![],
                outputs: vec![
                    ContractParam {
                        name: "version".into(),
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

    let metadata_v1 = ContractMetadata {
        version: "1.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567890,
        author: TEST_ACCOUNT,
        description: "Test Contract V1".into(),
        is_upgradeable: true,
    };

    // Deploy v1
    runtime.deploy_contract(TEST_WASM_V1, &contract_addr, &abi, metadata_v1, &limits).await.unwrap();

    // Switch to upgrader account
    msg::test_utils::set_sender(UPGRADER_ACCOUNT).unwrap();

    let metadata_v2 = ContractMetadata {
        version: "2.0.0".into(),
        created_at: 1234567890,
        updated_at: 1234567891,
        author: UPGRADER_ACCOUNT,
        description: "Test Contract V2".into(),
        is_upgradeable: true,
    };

    // Upgrade to v2
    runtime.deploy_contract(TEST_WASM_V2, &contract_addr, &abi, metadata_v2, &limits).await.unwrap();

    let env = ContractEnvironment {
        gas_limit: 1_000_000,
        block_number: 1,
        timestamp: 1234567890,
        caller: TEST_ACCOUNT,
        resource_limits: limits.clone(),
        gas_used: Arc::new(RwLock::new(0)),
    };

    // Verify current version
    let result = runtime.execute_contract(
        contract_addr,
        "get_version",
        vec![],
        &env,
        None,
    ).await.unwrap();
    assert_eq!(result[0].unwrap_i32(), 2);

    // Rollback to v1
    runtime.rollback_contract(&contract_addr).await.unwrap();

    // Verify rolled back version
    let result = runtime.execute_contract(
        contract_addr,
        "get_version",
        vec![],
        &env,
        None,
    ).await.unwrap();
    assert_eq!(result[0].unwrap_i32(), 1);

    // Clean up
    msg::test_utils::clear_sender().unwrap();
}
