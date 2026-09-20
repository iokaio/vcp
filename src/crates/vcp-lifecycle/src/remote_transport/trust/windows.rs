// SPDX-License-Identifier: Apache-2.0
//! Narrow read-only CryptoAPI handle boundary. CertOpenStore must use explicit
//! READONLY | OPEN_EXISTING: the retained convenience wrapper does not.
use super::Budget;
use std::{mem::size_of, ptr};
use windows_sys::Win32::{
    Foundation::{
        GetLastError, SetLastError, CRYPT_E_NOT_FOUND, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND,
    },
    Security::Cryptography::*,
};

struct Store(HCERTSTORE);
impl Drop for Store {
    fn drop(&mut self) {
        // SAFETY: sole owner of a successfully opened store; all contexts below
        // are freed before this local store guard leaves scope.
        unsafe {
            CertCloseStore(self.0, 0);
        }
    }
}
struct Context(*const CERT_CONTEXT);
impl Drop for Context {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this is the last enumerated context not yet consumed by
            // another CertEnumCertificatesInStore call.
            unsafe {
                CertFreeCertificateContext(self.0);
            }
        }
    }
}

pub(super) struct Snapshot {
    pub roots: Vec<Vec<u8>>,
    pub denied: Vec<Vec<u8>>,
}
pub(super) fn snapshot() -> Result<Snapshot, String> {
    let mut budget = Budget::default();
    let mut roots = Vec::new();
    let mut denied = Vec::new();
    for location in [
        CERT_SYSTEM_STORE_CURRENT_USER,
        CERT_SYSTEM_STORE_LOCAL_MACHINE,
    ] {
        collect(location, "Disallowed", false, &mut budget, &mut denied)?;
        collect(location, "ROOT", true, &mut budget, &mut roots)?;
    }
    Ok(Snapshot { roots, denied })
}

fn collect(
    location: u32,
    name: &str,
    roots: bool,
    budget: &mut Budget,
    result: &mut Vec<Vec<u8>>,
) -> Result<(), String> {
    let wide: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: fixed system-store provider, local locations and terminated names;
    // no pointer outlives this call. These flags prevent creation and mutation.
    let handle = unsafe {
        CertOpenStore(
            CERT_STORE_PROV_SYSTEM_W,
            0,
            0,
            location | CERT_STORE_READONLY_FLAG | CERT_STORE_OPEN_EXISTING_FLAG,
            wide.as_ptr().cast(),
        )
    };
    if handle.is_null() {
        // SAFETY: immediately observe the preceding failing Win32 call.
        let error = unsafe { GetLastError() };
        if !roots && matches!(error, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
            return Ok(());
        }
        return Err("Windows TLS certificate store could not be opened read-only".into());
    }
    let store = Store(handle);
    let mut context = Context(ptr::null());
    loop {
        // Enum consumes/frees the previous context even on failure. Transfer it
        // out before calling so early errors cannot double-free the old pointer.
        let previous = std::mem::replace(&mut context.0, ptr::null());
        // SAFETY: store is live and previous belongs to exactly this enumeration.
        let next = unsafe { CertEnumCertificatesInStore(store.0, previous) };
        if next.is_null() {
            // SAFETY: no other OS call occurred after enumeration failure.
            if unsafe { GetLastError() } == CRYPT_E_NOT_FOUND as u32 {
                break;
            }
            return Err("Windows TLS certificate enumeration failed".into());
        }
        context.0 = next;
        // SAFETY: enumeration returned a live context held by the guard.
        let certificate = unsafe { &*next };
        let length = certificate.cbCertEncoded as usize;
        budget.observe_size(length)?;
        if certificate.pbCertEncoded.is_null() || certificate.pCertInfo.is_null() {
            return Err("Windows TLS certificate context rejected".into());
        }
        if roots {
            // SAFETY: pCertInfo is owned by the still-live enumerated context;
            // null time requests current local system time, without network IO.
            if unsafe { CertVerifyTimeValidity(ptr::null(), certificate.pCertInfo) } != 0 {
                continue;
            }
            if !server_auth(context.0)? {
                continue;
            }
        }
        // SAFETY: CryptoAPI owns cbCertEncoded bytes at this pointer until the
        // next enumeration; the allocation bound was checked before copying.
        result.push(
            unsafe { std::slice::from_raw_parts(certificate.pbCertEncoded, length) }.to_vec(),
        );
    }
    Ok(())
}

fn server_auth(context: *const CERT_CONTEXT) -> Result<bool, String> {
    let mut size = 0u32;
    // SAFETY: caller retains the enumerated context; null buffer queries size.
    if unsafe { CertGetEnhancedKeyUsage(context, 0, ptr::null_mut(), &mut size) } == 0 {
        return Err("Windows TLS certificate usage query failed".into());
    }
    if (size as usize) < size_of::<CTL_USAGE>() || size > 64 * 1024 {
        return Err("Windows TLS certificate usage exceeds bounds".into());
    }
    // usize supplies suitable alignment for CTL_USAGE and its pointer array.
    let mut storage = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    let allocated = storage.len() * size_of::<usize>();
    let usage = storage.as_mut_ptr().cast::<CTL_USAGE>();
    // SAFETY: allocated/aligned storage covers queried size. No context mutation.
    let (ok, last_error) = unsafe {
        SetLastError(0);
        let ok = CertGetEnhancedKeyUsage(context, 0, usage, &mut size);
        (ok, GetLastError())
    };
    if ok == 0 || size as usize > allocated || (size as usize) < size_of::<CTL_USAGE>() {
        return Err("Windows TLS certificate usage read failed".into());
    }
    // SAFETY: successful API call initialized this aligned structure.
    let usage = unsafe { &*usage };
    let count = usage.cUsageIdentifier as usize;
    if count == 0 {
        return match last_error {
            value if value == CRYPT_E_NOT_FOUND as u32 => Ok(true),
            0 => Ok(false),
            _ => Err("Windows TLS certificate usage unavailable".into()),
        };
    }
    if count > 1024 {
        return Err("Windows TLS certificate usage count exceeded".into());
    }
    let start = storage.as_ptr() as usize;
    let end = start + size as usize;
    let array = usage.rgpszUsageIdentifier as usize;
    let array_bytes = count * size_of::<*mut u8>();
    if array < start || array > end || array_bytes > end - array {
        return Err("Windows TLS certificate usage pointers rejected".into());
    }
    let mut accepted = false;
    for index in 0..count {
        // SAFETY: complete pointer array lies within initialized storage. An
        // unaligned read also avoids depending on nested allocation alignment.
        let pointer = unsafe { ptr::read_unaligned(usage.rgpszUsageIdentifier.add(index)) };
        let address = pointer as usize;
        if address < start || address >= end {
            return Err("Windows TLS certificate OID rejected".into());
        }
        let limit = (end - address).min(128);
        // SAFETY: this bounded slice stays within the returned property buffer.
        let bytes = unsafe { std::slice::from_raw_parts(pointer, limit) };
        let length = bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or("Windows TLS certificate OID exceeds bounds")?;
        accepted |= &bytes[..length] == b"1.3.6.1.5.5.7.3.1";
    }
    Ok(accepted)
}
