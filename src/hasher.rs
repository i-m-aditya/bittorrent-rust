use sha1::{Digest, Sha1};

pub fn hash_bytes_and_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha1::default();
    hasher.update(bytes);
    bytes_to_hex(&hasher.finalize())
}

pub fn hash_bytes(bytes: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    hash.into()
}
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut result: String = String::new();
    for byte in bytes {
        result.push_str(format!("{:02x}", byte).as_str());
    }
    return result;
}

pub fn bytes_to_hex_url_encoded(bytes: &[u8]) -> String {
    let mut hasher = Sha1::default();
    hasher.update(bytes);
    let mut result: String = String::new();
    for byte in hasher.finalize() {
        result.push_str(format!("%{:02x}", byte).as_str());
    }
    return result;
}
