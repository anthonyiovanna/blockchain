use actix_web::dev::{ServiceRequest, Extensions};
use actix_web::dev::Service;
use actix_web::http::header::HeaderValue;
use actix_web::dev::ServiceResponse;
use actix_web::dev::Payload;
use actix_web::HttpMessage;  // Add this for extensions_mut
use crate::block::Block;
use crate::contract::standards::ContractResult;
use crate::contract::{
    ContractEnvironment, ContractABI, ResourceLimits, ContractRuntime,
    ContractMethod, ContractEvent, ContractParam, ContractMetadata
};
use crate::crypto::Hash;
use crate::transaction::Transaction;
use actix_cors::Cors;
use actix_governor::{Governor, GovernorConfigBuilder};
use actix_web::{
    error::{ErrorForbidden, ErrorInternalServerError, ErrorUnauthorized},
    get, middleware, post,
    web::{self, Data, Json},
    App, HttpResponse, HttpServer, Responder,
};
use actix_web_httpauth::{
    extractors::{
        bearer::{BearerAuth, Config},
        AuthenticationError,
    },
    middleware::HttpAuthentication,
};
use actix_web_prom::PrometheusMetricsBuilder;
use jsonrpc_core::{IoHandler, Result as RpcResult};
use jsonrpc_derive::rpc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use prometheus::{Histogram, HistogramOpts, Registry};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::SystemTime};
use tokio::sync::RwLock;
use tracing::{error, info, instrument, warn};
use tracing_actix_web::TracingLogger;

/// Wrapper type for wasmer::Value that implements Serialize/Deserialize
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WasmValue {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
}

impl From<wasmer::Value> for WasmValue {
    fn from(value: wasmer::Value) -> Self {
        match value {
            wasmer::Value::I32(v) => WasmValue::I32(v),
            wasmer::Value::I64(v) => WasmValue::I64(v),
            wasmer::Value::F32(v) => WasmValue::F32(v),
            wasmer::Value::F64(v) => WasmValue::F64(v),
            _ => WasmValue::String("Unsupported type".to_string()),
        }
    }
}

impl Into<wasmer::Value> for WasmValue {
    fn into(self) -> wasmer::Value {
        match self {
            WasmValue::I32(v) => wasmer::Value::I32(v),
            WasmValue::I64(v) => wasmer::Value::I64(v),
            WasmValue::F32(v) => wasmer::Value::F32(v),
            WasmValue::F64(v) => wasmer::Value::F64(v),
            _ => wasmer::Value::I32(0), // Default for unsupported types
        }
    }
}

/// API state
pub struct ApiState {
    pub contract_runtime: Arc<RwLock<ContractRuntime>>,
    jwt_secret: String,
}

impl ApiState {
    pub fn new(jwt_secret: String) -> Self {
        ApiState {
            contract_runtime: Arc::new(RwLock::new(ContractRuntime::new())),
            jwt_secret,
        }
    }

    pub fn create_token(&self, username: &str, role: &str) -> Result<String, ApiError> {
        let expiration = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize + 24 * 3600; // 24 hours

        let claims = Claims {
            sub: username.to_string(),
            role: role.to_string(),
            exp: expiration,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )
        .map_err(|e| ApiError::Internal(format!("Token creation failed: {}", e)))
    }

    pub fn validate_token(&self, token: &str) -> Result<Claims, ApiError> {
        let validation = Validation::default();
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &validation,
        )
        .map(|data| data.claims)
        .map_err(|e| ApiError::Unauthorized(format!("Token validation failed: {}", e)))
    }
}

/// JWT Claims structure
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    role: String,
    exp: usize,
}

/// API error types
#[derive(Debug, Serialize, thiserror::Error)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
}

/// API response types
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
    pub status: String,
    pub timestamp: u64,
}

/// Login request
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

/// Login response
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    token: String,
}

/// Contract deployment request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployContractRequest {
    pub bytecode: Vec<u8>,
    pub abi: ContractABI,
    pub metadata: ContractMetadata,
    pub resource_limits: ResourceLimits,
}

/// Contract deployment response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployContractResponse {
    pub address: [u8; 32],
    pub implemented_standards: Vec<String>,
    pub version: String,
}

/// Contract execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteContractRequest {
    pub method: String,
    pub args: Vec<WasmValue>,
    pub gas_limit: u64,
}

/// Contract state query request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractStateRequest {
    pub key: Vec<u8>,
}

/// JWT authentication validator
async fn validator(
    mut req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (actix_web::Error, ServiceRequest)> {
    let state = req.app_data::<Data<ApiState>>().unwrap();
    
    match state.validate_token(credentials.token()) {
        Ok(claims) => {
            req.extensions_mut().insert(claims);
            Ok(req)
        }
        Err(_) => Err((ErrorUnauthorized("Invalid token").into(), req)),
    }
}

#[post("/contracts")]
#[instrument(skip(state))]
async fn deploy_contract(
    state: Data<ApiState>,
    request: Json<DeployContractRequest>,
) -> impl Responder {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Generate a unique address for the contract
    let mut address = [0u8; 32]; // Replace with proper address generation
    
    // Deploy the contract
    let mut runtime = state.contract_runtime.write().await;
    let result = runtime
        .deploy_contract(
            &request.bytecode,
            &address,
            &request.abi,
            request.metadata.clone(),
            &request.resource_limits,
        )
        .await;

    match result {
        Ok(_) => HttpResponse::Ok().json(ApiResponse {
            data: DeployContractResponse { 
                address,
                implemented_standards: request.abi.standards.clone(),
                version: request.metadata.version.clone(),
            },
            status: "success".to_string(),
            timestamp,
        }),
        Err(e) => {
            error!("Contract deployment failed: {:?}", e);
            HttpResponse::InternalServerError().json(ApiResponse {
                data: (),
                status: format!("error: {:?}", e),
                timestamp,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_rt::test]
    async fn test_contract_deployment() {
        let state = Data::new(ApiState::new("test_secret".to_string()));
        let token = state.create_token("test", "user").unwrap();
        
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .service(
                    web::scope("")
                        .wrap(HttpAuthentication::bearer(validator))
                        .service(deploy_contract)
                )
        ).await;

        let request = DeployContractRequest {
            bytecode: vec![0, 1, 2, 3], // Test bytecode
            abi: ContractABI {
                methods: vec![],
                events: vec![],
                standards: vec!["ERC20".to_string()], // Test standard
            },
            metadata: ContractMetadata {
                version: "1.0.0".to_string(),
                created_at: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                updated_at: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                author: [0u8; 32],
                description: "Test contract".to_string(),
                is_upgradeable: true,
            },
            resource_limits: ResourceLimits {
                max_memory: 1024 * 1024,
                max_gas: 1_000_000,
                max_storage: 1024 * 1024,
                max_call_depth: 5,
            },
        };

        let req = test::TestRequest::post()
            .uri("/contracts")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&request)
            .to_request();
            
        let resp: ApiResponse<DeployContractResponse> = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.status, "success");
        assert_eq!(resp.data.address.len(), 32);
        assert_eq!(resp.data.implemented_standards, vec!["ERC20".to_string()]);
        assert_eq!(resp.data.version, "1.0.0");
    }
}
