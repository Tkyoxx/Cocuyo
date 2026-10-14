// el candado entre el pc y el celular, con lo que trae windows (cng). nada de cripto hecha en casa: solo se arma

use windows::core::PCWSTR;
use windows::Win32::Foundation::NTSTATUS;
use windows::Win32::Security::Cryptography::*;

pub const PUBLIC_LEN: usize = 65;

fn ok(status: NTSTATUS, what: &str) -> Result<(), String> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(format!("{what} falló (0x{:08x})", status.0 as u32))
    }
}

pub fn random(len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    unsafe {
        let _ = BCryptGenRandom(None, &mut out, BCRYPT_USE_SYSTEM_PREFERRED_RNG);
    }
    out
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    unsafe {
        let _ = BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, data, &mut out);
    }
    out
}

pub fn hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    unsafe {
        let _ = BCryptHash(BCRYPT_HMAC_SHA256_ALG_HANDLE, Some(key), data, &mut out);
    }
    out
}

pub fn hkdf(ikm: &[u8], salt: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let prk = hmac(if salt.is_empty() { &[0u8; 32] } else { salt }, ikm);
    let mut out = Vec::with_capacity(len);
    let mut block: Vec<u8> = Vec::new();
    let mut counter = 1u8;
    while out.len() < len {
        let mut input = block.clone();
        input.extend_from_slice(info);
        input.push(counter);
        block = hmac(&prk, &input).to_vec();
        out.extend_from_slice(&block);
        counter += 1;
    }
    out.truncate(len);
    out
}

struct Alg(BCRYPT_ALG_HANDLE);

impl Alg {
    fn open(id: PCWSTR, flags: BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS) -> Result<Alg, String> {
        let mut handle = BCRYPT_ALG_HANDLE::default();
        ok(unsafe { BCryptOpenAlgorithmProvider(&mut handle, id, PCWSTR::null(), flags) }, "abrir el algoritmo")?;
        Ok(Alg(handle))
    }
}

impl Drop for Alg {
    fn drop(&mut self) {
        unsafe {
            let _ = BCryptCloseAlgorithmProvider(self.0, 0);
        }
    }
}

struct Key(BCRYPT_KEY_HANDLE);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = BCryptDestroyKey(self.0);
        }
    }
}

pub fn pbkdf2(password: &[u8], salt: &[u8], rounds: u64, len: usize) -> Result<Vec<u8>, String> {
    let alg = Alg::open(BCRYPT_SHA256_ALGORITHM, BCRYPT_ALG_HANDLE_HMAC_FLAG)?;
    let mut out = vec![0u8; len];
    ok(unsafe { BCryptDeriveKeyPBKDF2(alg.0, Some(password), Some(salt), rounds, &mut out, 0) }, "pbkdf2")?;
    Ok(out)
}

fn ecdh() -> Result<Alg, String> {
    Alg::open(BCRYPT_ECDH_P256_ALGORITHM, BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0))
}

fn blob(magic: u32, parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + 96);
    out.extend_from_slice(&magic.to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    for part in parts {
        out.extend_from_slice(part);
    }
    out
}

pub struct Pair {
    private: Vec<u8>,
}

impl Pair {
    pub fn generate() -> Result<Pair, String> {
        let alg = ecdh()?;
        let mut handle = BCRYPT_KEY_HANDLE::default();
        ok(unsafe { BCryptGenerateKeyPair(alg.0, &mut handle, 256, 0) }, "crear la llave")?;
        let key = Key(handle);
        ok(unsafe { BCryptFinalizeKeyPair(key.0, 0) }, "cerrar la llave")?;
        let mut size = 0u32;
        ok(unsafe { BCryptExportKey(key.0, None, BCRYPT_ECCPRIVATE_BLOB, None, &mut size, 0) }, "medir la llave")?;
        let mut private = vec![0u8; size as usize];
        ok(unsafe { BCryptExportKey(key.0, None, BCRYPT_ECCPRIVATE_BLOB, Some(&mut private), &mut size, 0) }, "sacar la llave")?;
        private.truncate(size as usize);
        Ok(Pair { private })
    }

    pub fn from_secret(secret: &[u8]) -> Result<Pair, String> {
        if secret.len() != 8 + 96 || secret[..4] != BCRYPT_ECDH_PRIVATE_P256_MAGIC.to_le_bytes() {
            return Err("llave guardada rota".into());
        }
        Ok(Pair { private: secret.to_vec() })
    }

    pub fn secret(&self) -> &[u8] {
        &self.private
    }

    pub fn public(&self) -> [u8; PUBLIC_LEN] {
        let mut out = [0u8; PUBLIC_LEN];
        out[0] = 4;
        out[1..].copy_from_slice(&self.private[8..8 + 64]);
        out
    }

    pub fn agree(&self, peer: &[u8]) -> Result<[u8; 32], String> {
        if peer.len() != PUBLIC_LEN || peer[0] != 4 {
            return Err("llave del otro lado mal formada".into());
        }
        let alg = ecdh()?;
        let mut mine = BCRYPT_KEY_HANDLE::default();
        ok(unsafe { BCryptImportKeyPair(alg.0, None, BCRYPT_ECCPRIVATE_BLOB, &mut mine, &self.private, 0) }, "cargar mi llave")?;
        let mine = Key(mine);
        let mut theirs = BCRYPT_KEY_HANDLE::default();
        let public = blob(BCRYPT_ECDH_PUBLIC_P256_MAGIC, &[&peer[1..33], &peer[33..65]]);
        ok(unsafe { BCryptImportKeyPair(alg.0, None, BCRYPT_ECCPUBLIC_BLOB, &mut theirs, &public, 0) }, "cargar la llave del otro lado")?;
        let theirs = Key(theirs);
        let mut secret = BCRYPT_SECRET_HANDLE::default();
        ok(unsafe { BCryptSecretAgreement(mine.0, theirs.0, &mut secret, 0) }, "acordar el secreto")?;
        let mut out = [0u8; 32];
        let mut size = 0u32;
        let derived = unsafe { BCryptDeriveKey(secret, BCRYPT_KDF_RAW_SECRET, None, Some(&mut out), &mut size, 0) };
        unsafe {
            let _ = BCryptDestroySecret(secret);
        }
        ok(derived, "sacar el secreto")?;
        out.reverse();
        Ok(out)
    }
}

fn gcm(key: &[u8]) -> Result<(Alg, Key), String> {
    let alg = Alg::open(BCRYPT_AES_ALGORITHM, BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0))?;
    let mode: Vec<u8> = "ChainingModeGCM\0".encode_utf16().flat_map(u16::to_le_bytes).collect();
    ok(unsafe { BCryptSetProperty(BCRYPT_HANDLE(alg.0 .0), BCRYPT_CHAINING_MODE, &mode, 0) }, "poner gcm")?;
    let mut handle = BCRYPT_KEY_HANDLE::default();
    ok(unsafe { BCryptGenerateSymmetricKey(alg.0, &mut handle, None, key, 0) }, "cargar la llave aes")?;
    Ok((alg, Key(handle)))
}

pub fn seal(key: &[u8], nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, String> {
    let (_alg, handle) = gcm(key)?;
    let mut tag = [0u8; 16];
    let mut info = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        cbSize: std::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
        dwInfoVersion: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION,
        pbNonce: nonce.as_ptr() as *mut u8,
        cbNonce: 12,
        pbAuthData: aad.as_ptr() as *mut u8,
        cbAuthData: aad.len() as u32,
        pbTag: tag.as_mut_ptr(),
        cbTag: 16,
        ..Default::default()
    };
    let mut out = vec![0u8; plain.len()];
    let mut size = 0u32;
    let input = (!plain.is_empty()).then_some(plain);
    let output = (!plain.is_empty()).then_some(&mut out[..]);
    ok(unsafe { BCryptEncrypt(handle.0, input, Some(&mut info as *mut _ as *const _), None, output, &mut size, BCRYPT_FLAGS(0)) }, "cifrar")?;
    out.extend_from_slice(&tag);
    Ok(out)
}

pub fn open(key: &[u8], nonce: &[u8; 12], aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < 16 {
        return None;
    }
    let (body, tag) = sealed.split_at(sealed.len() - 16);
    let (_alg, handle) = gcm(key).ok()?;
    let mut tag = tag.to_vec();
    let mut info = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        cbSize: std::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
        dwInfoVersion: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION,
        pbNonce: nonce.as_ptr() as *mut u8,
        cbNonce: 12,
        pbAuthData: aad.as_ptr() as *mut u8,
        cbAuthData: aad.len() as u32,
        pbTag: tag.as_mut_ptr(),
        cbTag: 16,
        ..Default::default()
    };
    let mut out = vec![0u8; body.len()];
    let mut size = 0u32;
    let input = (!body.is_empty()).then_some(body);
    let output = (!body.is_empty()).then_some(&mut out[..]);
    let status = unsafe { BCryptDecrypt(handle.0, input, Some(&mut info as *mut _ as *const _), None, output, &mut size, BCRYPT_FLAGS(0)) };
    status.is_ok().then_some(out)
}

pub fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn hkdf_matches_rfc5869() {
        let ikm = hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let out = hkdf(&ikm, &hex("000102030405060708090a0b0c"), &hex("f0f1f2f3f4f5f6f7f8f9"), 42);
        assert_eq!(out, hex("3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"));
    }

    #[test]
    fn pbkdf2_matches_the_known_vector() {
        let out = pbkdf2(b"password", b"salt", 1, 32).unwrap();
        assert_eq!(out, hex("120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"));
    }

    #[test]
    fn gcm_matches_nist_and_rejects_tampering() {
        let key = [0u8; 32];
        let nonce = [0u8; 12];
        let sealed = seal(&key, &nonce, &[], &[0u8; 16]).unwrap();
        assert_eq!(sealed, hex("cea7403d4d606b6e074ec5d3baf39d18d0d1c8a799996bf0265b98b5d48ab919"));
        assert_eq!(open(&key, &nonce, &[], &sealed).unwrap(), vec![0u8; 16]);
        let mut bad = sealed.clone();
        bad[3] ^= 1;
        assert!(open(&key, &nonce, &[], &bad).is_none());
        assert!(open(&key, &nonce, b"otra cosa", &sealed).is_none());
    }

    #[test]
    #[ignore]
    fn cruce_con_el_navegador_1() {
        let dir = std::path::PathBuf::from(std::env::var("COCUYO_CRUCE").unwrap());
        let pc = Pair::generate().unwrap();
        std::fs::write(dir.join("pc-secreta.bin"), pc.secret()).unwrap();
        std::fs::write(dir.join("pc-publica.bin"), pc.public()).unwrap();
    }

    #[test]
    #[ignore]
    fn cruce_con_el_navegador_2() {
        let dir = std::path::PathBuf::from(std::env::var("COCUYO_CRUCE").unwrap());
        let pc = Pair::from_secret(&std::fs::read(dir.join("pc-secreta.bin")).unwrap()).unwrap();
        let cel = std::fs::read(dir.join("cel-publica.bin")).unwrap();
        let dh = pc.agree(&cel).unwrap();
        assert_eq!(dh.to_vec(), std::fs::read(dir.join("cel-dh.bin")).unwrap(), "el acuerdo no coincide");
        let llave = hkdf(&dh, b"sal", b"info", 32);
        assert_eq!(llave, std::fs::read(dir.join("cel-hkdf.bin")).unwrap(), "hkdf no coincide");
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(&7u64.to_be_bytes());
        let abierto = open(&llave, &nonce, b"cocuyo-remoto-1", &std::fs::read(dir.join("cel-sellado.bin")).unwrap()).expect("no se pudo abrir lo del celular");
        assert_eq!(abierto, "hola desde el celular".as_bytes());
        assert_eq!(pbkdf2(b"K7Q2XM94P", b"cocuyo-emparejar-1", 150000, 64).unwrap(), std::fs::read(dir.join("cel-pbkdf2.bin")).unwrap(), "pbkdf2 no coincide");
        nonce[4..].copy_from_slice(&8u64.to_be_bytes());
        std::fs::write(dir.join("pc-sellado.bin"), seal(&llave, &nonce, b"cocuyo-remoto-1", "hola desde el pc".as_bytes()).unwrap()).unwrap();
    }

    #[test]
    fn both_sides_agree_and_keys_survive_saving() {
        let a = Pair::generate().unwrap();
        let b = Pair::generate().unwrap();
        assert_eq!(a.agree(&b.public()).unwrap(), b.agree(&a.public()).unwrap());
        let again = Pair::from_secret(a.secret()).unwrap();
        assert_eq!(again.public(), a.public());
        assert!(a.agree(&[4u8; 65]).is_err());
        assert!(a.agree(&[0u8; 10]).is_err());
    }
}
