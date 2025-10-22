use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tauri::command;
use x25519_dalek::x25519;

const X25519_BASEPOINT: [u8; 32] = [
    9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyPair {
    pub private_key: String,
    pub public_key: String,
}

#[command]
pub fn generate_keypair() -> Result<KeyPair, String> {
    log::info!("开始生成 WireGuard 密钥对");

    let mut private_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut private_bytes);

    clamp_private_key(&mut private_bytes);

    let private_key = BASE64.encode(&private_bytes);
    let public_bytes = x25519(private_bytes, X25519_BASEPOINT);
    let public_key = BASE64.encode(&public_bytes);

    log::info!("WireGuard 密钥对生成成功");

    Ok(KeyPair {
        private_key,
        public_key,
    })
}

#[command]
pub fn generate_preshared_key() -> Result<String, String> {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    Ok(BASE64.encode(key))
}

#[command]
pub fn private_key_to_public(private_key: String) -> Result<String, String> {
    let bytes = BASE64
        .decode(private_key.trim())
        .map_err(|e| format!("无效的私钥格式: {}", e))?;

    if bytes.len() != 32 {
        return Err("私钥长度必须为32字节".to_string());
    }

    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);

    clamp_private_key(&mut key_bytes);

    let public_bytes = x25519(key_bytes, X25519_BASEPOINT);

    Ok(BASE64.encode(&public_bytes))
}

pub fn compute_public_key(private_key: &str) -> Result<String, String> {
    let bytes = BASE64
        .decode(private_key.trim())
        .map_err(|e| format!("无效的私钥格式: {}", e))?;

    if bytes.len() != 32 {
        return Err("私钥长度必须为32字节".to_string());
    }

    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);

    clamp_private_key(&mut key_bytes);

    let public_bytes = x25519(key_bytes, X25519_BASEPOINT);

    Ok(BASE64.encode(&public_bytes))
}

fn clamp_private_key(key: &mut [u8; 32]) {
    key[0] &= 248;
    key[31] &= 127;
    key[31] |= 64;
}
