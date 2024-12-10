use blockchain::contract::{
    self,
    ContractRuntime,
    ContractABI,
    ContractMetadata,
    ContractMethod,
    ContractParam,
    ResourceLimits,
    DEPLOYER_ROLE,
    EXECUTOR_ROLE,
    UPGRADER_ROLE,
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::SystemTime;

// Helper function to create test contract metadata
fn create_test_metadata(version: &str) -> ContractMetadata {
    ContractMetadata {
        version: version.to_string(),
        created_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        updated_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        author: [0u8; 32],
        description: "Test Contract".to_string(),
        is_upgradeable: true,
    }
}

// Helper function to create test contract ABI
fn create_test_abi() -> ContractABI {
    ContractABI {
        methods: vec![ContractMethod {
            name: "test".to_string(),
            inputs: vec![ContractParam {
                name: "input".to_string(),
                param_type: "string".to_string(),
                indexed: false,
            }],
            outputs: vec![ContractParam {
                name: "output".to_string(),
                param_type: "string".to_string(),
                indexed: false,
            }],
            payable: false,
        }],
        events: vec![],
        standards: vec!["test".to_string()],
    }
}

// Helper function to create test contract bytes
fn create_test_contract() -> Vec<u8> {
    vec![
        0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00,
        // ... simplified for testing
    ]
}

// Helper function to create test resource limits
fn create_test_limits() -> ResourceLimits {
    ResourceLimits {
        max_memory: 1024 * 1024, // 1MB
        max_gas: 1_000_000,
        max_storage: 1024 * 1024, // 1MB
        max_call_depth: 10,
    }
}

fn benchmark_contract_deployment(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let test_account = [1u8; 32];
    
    c.bench_function("contract_deployment", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut runtime = ContractRuntime::new();
                runtime.grant_role(DEPLOYER_ROLE, test_account).unwrap();
                
                let contract_addr = [0u8; 32];
                let contract_bytes = create_test_contract();
                let abi = create_test_abi();
                let metadata = create_test_metadata("1.0.0");
                let limits = create_test_limits();
                
                black_box(
                    runtime.deploy_contract(
                        &contract_bytes,
                        &contract_addr,
                        &abi,
                        metadata,
                        &limits,
                    ).await.unwrap()
                );
            });
        })
    });
}

fn benchmark_state_operations(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let test_account = [1u8; 32];
    let contract_addr = [0u8; 32];
    
    // Setup
    rt.block_on(async {
        let mut runtime = ContractRuntime::new();
        runtime.grant_role(DEPLOYER_ROLE, test_account).unwrap();
        
        runtime.deploy_contract(
            &create_test_contract(),
            &contract_addr,
            &create_test_abi(),
            create_test_metadata("1.0.0"),
            &create_test_limits(),
        ).await.unwrap();
    });
    
    let mut group = c.benchmark_group("state_operations");
    
    group.bench_function("state_read", |b| {
        b.iter(|| {
            rt.block_on(async {
                let runtime = ContractRuntime::new();
                black_box(runtime.get_contract_state(&contract_addr));
            });
        })
    });
    
    group.bench_function("state_snapshot", |b| {
        b.iter(|| {
            rt.block_on(async {
                let runtime = ContractRuntime::new();
                black_box(runtime.get_state_snapshots(&contract_addr));
            });
        })
    });
    
    group.finish();
}

fn benchmark_concurrent_operations(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let test_account = [1u8; 32];
    
    c.bench_function("concurrent_deployments", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut handles = vec![];
                
                for i in 0..10 {
                    let contract_addr = [i as u8; 32];
                    let handle = tokio::spawn(async move {
                        let mut runtime = ContractRuntime::new();
                        runtime.grant_role(DEPLOYER_ROLE, test_account).unwrap();
                        
                        runtime.deploy_contract(
                            &create_test_contract(),
                            &contract_addr,
                            &create_test_abi(),
                            create_test_metadata("1.0.0"),
                            &create_test_limits(),
                        ).await.unwrap();
                    });
                    handles.push(handle);
                }
                
                for handle in handles {
                    handle.await.unwrap();
                }
            });
        })
    });
}

criterion_group!(
    benches,
    benchmark_contract_deployment,
    benchmark_state_operations,
    benchmark_concurrent_operations
);
criterion_main!(benches);
