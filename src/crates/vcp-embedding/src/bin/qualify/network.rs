// SPDX-License-Identifier: Apache-2.0
//! Controlled qualification traffic only; the embedding library has no network path.
use std::{
    io::Write,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    time::Duration,
};
use vcp_embedding::Error;

fn failure(reason: impl Into<String>) -> Error {
    Error::Runtime {
        reason: reason.into(),
    }
}

pub(super) struct Canary {
    address: SocketAddr,
    nonce: String,
}
impl Canary {
    pub fn new(address: &str, port: &str, nonce: &str) -> Result<Self, Error> {
        let address: Ipv4Addr = address
            .parse()
            .map_err(|_| failure("invalid canary IPv4 address"))?;
        let port: u16 = port.parse().map_err(|_| failure("invalid canary port"))?;
        if !address.is_private()
            || port == 0
            || nonce.len() != 32
            || !nonce.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(failure(
                "canary requires private IPv4, nonzero port and a 32-digit nonce",
            ));
        }
        Ok(Self {
            address: SocketAddr::V4(SocketAddrV4::new(address, port)),
            nonce: nonce.into(),
        })
    }
    pub fn observe(&self, denied: bool) -> Result<serde_json::Value, Error> {
        if !cfg!(windows) {
            return Err(failure(
                "network isolation qualification requires native Windows",
            ));
        }
        let connected = TcpStream::connect_timeout(&self.address, Duration::from_secs(2));
        let (diagnosis_status, missing_capability) = diagnose(&self.address.ip().to_string())?;
        match connected {
            Ok(mut stream) => {
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .map_err(|e| failure(e.to_string()))?;
                stream
                    .write_all(format!("VCP_LOCAL_CANARY {}\n", self.nonce).as_bytes())
                    .map_err(|e| failure(e.to_string()))?;
                if denied {
                    return Err(failure("network canary unexpectedly connected"));
                }
                // Missing-capability diagnostics are meaningful only in an
                // observed AppContainer. Normal desktop processes need no capability.
                Ok(serde_json::json!({"outcome":"connected","nonce":self.nonce,
                    "diagnosis_status":diagnosis_status,"missing_capability":missing_capability}))
            }
            Err(error) => {
                let timeout = error.kind() == std::io::ErrorKind::TimedOut;
                if !denied
                    || diagnosis_status != 0
                    || !(1..=3).contains(&missing_capability)
                    || !(timeout || error.raw_os_error() == Some(10013))
                {
                    return Err(failure(format!("unqualified network failure: kind={:?}, os_error={:?}, diagnosis={diagnosis_status}/{missing_capability}", error.kind(), error.raw_os_error())));
                }
                // This is not sufficient proof by itself. The parent must verify
                // the child token, both live controls and independently observed traffic.
                Ok(serde_json::json!({"outcome":"blocked","nonce":self.nonce,
                    "timed_out":timeout,"os_error":error.raw_os_error(),
                    "diagnosis_status":diagnosis_status,"missing_capability":missing_capability}))
            }
        }
    }
}

#[cfg(windows)]
fn diagnose(address: &str) -> Result<(u32, u32), Error> {
    #[link(name = "FirewallAPI", kind = "raw-dylib")]
    extern "system" {
        fn NetworkIsolationDiagnoseConnectFailureAndGetInfo(
            server: *const u16,
            kind: *mut u32,
        ) -> u32;
    }
    let server: Vec<u16> = address.encode_utf16().chain(Some(0)).collect();
    let mut kind = 0;
    // SAFETY: the API receives a terminated immutable UTF-16 literal address and
    // a live DWORD output. It retains neither pointer. No hostname lookup is used.
    let status =
        unsafe { NetworkIsolationDiagnoseConnectFailureAndGetInfo(server.as_ptr(), &mut kind) };
    Ok((status, kind))
}
#[cfg(not(windows))]
fn diagnose(_: &str) -> Result<(u32, u32), Error> {
    Err(failure(
        "network isolation qualification requires native Windows",
    ))
}
