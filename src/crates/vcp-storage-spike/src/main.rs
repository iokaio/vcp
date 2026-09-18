// SPDX-License-Identifier: Apache-2.0
//! Native qualification executable. All inputs/keys are disposable synthetic data.
use age::{secrecy::ExposeSecret, x25519::Identity};
use ed25519_dalek::SigningKey;
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};
use vcp_storage_spike::{
    fixture, reject, search,
    store::Store,
    vault::{self, Fault, Trust},
    Result,
};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err(reject("expected mode and disposable root"));
    }
    let root = Path::new(&args[1]);
    if !root.is_absolute() {
        return Err(reject("absolute disposable root required"));
    }
    match args[0].as_str() {
        "crash" if args.len() == 4 => {
            let mut store = Store::open(root, &args[2]).await?;
            let (one, objects) = fixture(1, 100);
            for bytes in objects.values() {
                store.put_artifact(bytes)?;
            }
            store.commit(0, one).await?;
            println!("ACK 1");
            std::io::stdout().flush()?;
            let phase = &args[3];
            store
                .commit_observed(1, fixture(2, 101).0, |point| {
                    if phase == point {
                        barrier(point);
                    }
                })
                .await?;
            if phase == "after_ack" {
                println!("ACK 2");
                barrier("after_ack");
            }
            return Err(reject("unknown crash barrier"));
        }
        "inspect" if args.len() == 3 => {
            let store = Store::open(root, &args[2]).await?;
            println!(
                "{}",
                serde_json::json!({"status":"pass","view":store.view()})
            );
        }
        "benchmark" if args.len() == 2 => {
            fs::create_dir(root)?;
            println!("{}", benchmark(root).await?);
        }
        "handoff-produce" if args.len() == 2 => {
            fs::create_dir(root)?;
            handoff_produce(root)?;
        }
        "handoff-recovery" if args.len() == 2 => {
            fs::create_dir(root.join("recovery"))?;
            fs::write(
                root.join("recovery/identity.txt"),
                public_fixture_identity().to_string().expose_secret(),
            )?;
            println!("{{\"status\":\"pass\",\"identity\":\"public-age-test-vector-only\"}}");
        }
        "handoff-consume" if args.len() == 2 => {
            handoff_consume(root).await?;
        }
        _ => return Err(reject("unknown mode or arguments")),
    }
    Ok(())
}
fn barrier(point: &str) {
    println!("BARRIER {point}");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
fn trust() -> Trust {
    Trust {
        writers: BTreeSet::from([SigningKey::from_bytes(&[42; 32]).verifying_key().to_bytes()]),
        minimum_sequence: 0,
        minimum_deletion: 1,
        parent: None,
    }
}
async fn benchmark(root: &Path) -> Result<serde_json::Value> {
    let mut rows = Vec::new();
    let identity = Identity::generate();
    let writer = SigningKey::from_bytes(&[42; 32]);
    for count in [100, 1000] {
        for kind in ["sqlite", "files"] {
            for repetition in 0..3 {
                let dir = root.join(format!("{kind}-{count}-{repetition}"));
                fs::create_dir(&dir)?;
                let started = Instant::now();
                let mut store = Store::open(&dir.join("active"), kind).await?;
                let (one, objects) = fixture(1, count);
                for bytes in objects.values() {
                    store.put_artifact(bytes)?;
                }
                store.commit(0, one.clone()).await?;
                let commit_us = started.elapsed().as_micros();
                let pinned = store.view().unwrap();
                let (two, _) = fixture(2, count + 1);
                // The captured watermark is immutable while the authoritative writer
                // continues committing the next view on its own task.
                let writer_task = tokio::spawn(async move {
                    store.commit(1, two).await?;
                    store.close().await
                });
                for incremental in [false, true] {
                    let label = if incremental { "incremental" } else { "full" };
                    let vault_dir = dir.join(label);
                    let stage = dir.join(format!("stage-{label}"));
                    fs::create_dir(&vault_dir)?;
                    fs::create_dir(&stage)?;
                    let start = Instant::now();
                    let cpu = cpu_us()?;
                    let first = vault::publish(
                        &pinned,
                        &objects,
                        &identity.to_public(),
                        &writer,
                        &stage,
                        &vault_dir,
                        None,
                        incremental,
                        Fault::None,
                    )?;
                    let first_us = start.elapsed().as_micros();
                    let first_cpu_us = cpu_us()?.saturating_sub(cpu);
                    let start = Instant::now();
                    let cpu = cpu_us()?;
                    let restored = vault::restore(&vault_dir, &first.id, &identity, &trust())?;
                    let restore_us = start.elapsed().as_micros();
                    let restore_cpu_us = cpu_us()?.saturating_sub(cpu);
                    if restored.view != one || restored.artifacts != objects {
                        return Err(reject("snapshot consistency failed"));
                    }
                    // A stale binary index is an explicit incompatible cache, not input
                    // authority. Rebuild a new generation from the retained vectors.
                    fs::write(
                        dir.join(format!("incompatible-{label}.index")),
                        b"foreign-index-format-v999",
                    )?;
                    search::reject_incompatible_cache(&fs::read(
                        dir.join(format!("incompatible-{label}.index")),
                    )?)?;
                    let search =
                        search::rebuild(&restored.view, &dir.join(format!("search-{label}")))?;
                    let (two, _) = fixture(2, count + 1);
                    let start = Instant::now();
                    let cpu = cpu_us()?;
                    let update = vault::publish(
                        &two,
                        &objects,
                        &identity.to_public(),
                        &writer,
                        &stage,
                        &vault_dir,
                        Some(&first),
                        incremental,
                        Fault::None,
                    )?;
                    let update_us = start.elapsed().as_micros();
                    let update_cpu_us = cpu_us()?.saturating_sub(cpu);
                    let mut next = trust();
                    next.minimum_sequence = 1;
                    next.parent = Some(first.id.clone());
                    if vault::restore(&vault_dir, &update.id, &identity, &next)?.view != two {
                        return Err(reject("updated snapshot mismatch"));
                    }
                    rows.push(serde_json::json!({"backend":kind,"records":count+4,"repetition":repetition,"packaging":label,
                "commit_us":commit_us,"encrypt_first_us":first_us,"restore_us":restore_us,"update_us":update_us,
                "encrypt_first_cpu_us":first_cpu_us,"restore_cpu_us":restore_cpu_us,"update_cpu_us":update_cpu_us,
                "first_ciphertext_bytes":first.transferred,"changed_ciphertext_bytes":update.transferred,"search":search}));
                }
                writer_task.await??;
                let started = Instant::now();
                let reopened = Store::open(&dir.join("active"), kind).await?;
                if reopened.view() != Some(fixture(2, count + 1).0) {
                    return Err(reject("concurrent commit missing"));
                }
                let reopen_us = started.elapsed().as_micros();
                let started = Instant::now();
                let other = if kind == "sqlite" { "files" } else { "sqlite" };
                let converted_root = dir.join("converted");
                let mut converted = Store::open(&converted_root, other).await?;
                let exported = reopened.view().unwrap();
                for id in exported.artifacts.keys() {
                    converted.put_artifact(&reopened.artifact(id)?)?;
                }
                converted.commit(0, exported.clone()).await?;
                converted.close().await?;
                let mut converted = Store::open(&converted_root, other).await?;
                if converted.view() != Some(exported) {
                    return Err(reject("measured conversion differs"));
                }
                let conversion_us = started.elapsed().as_micros();
                let converted_configuration = converted.configuration().await?;
                for row in rows.iter_mut().rev().take(2) {
                    row["reopen_us"] = serde_json::json!(reopen_us);
                    row["conversion_us"] = serde_json::json!(conversion_us);
                    row["converted_backend"] = serde_json::json!(other);
                    row["converted_configuration"] = converted_configuration.clone();
                }
            }
        }
    }
    Ok(
        serde_json::json!({"status":"pass","rows":rows,"limits":"Synthetic 3-dimensional retained vectors; local-model feasibility remains P0-02. Logical disk and process CPU sampled by the native supervisor."}),
    )
}
#[cfg(windows)]
fn cpu_us() -> Result<u64> {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            process: *mut std::ffi::c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let (mut creation, mut exit, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    // Windows writes four FILETIME values; all pointers refer to live local
    // structures and the process pseudo-handle is valid for this call.
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok((((kernel.high as u64) << 32 | kernel.low as u64)
        + ((user.high as u64) << 32 | user.low as u64))
        / 10)
}
#[cfg(not(windows))]
fn cpu_us() -> Result<u64> {
    Err(reject("native Windows measurements required"))
}
fn handoff_produce(root: &Path) -> Result<()> {
    for dir in ["stage", "vault", "recovery"] {
        fs::create_dir(root.join(dir))?;
    }
    let identity = public_fixture_identity();
    fs::write(
        root.join("recovery/identity.txt"),
        identity.to_string().expose_secret(),
    )?;
    let writer = SigningKey::from_bytes(&[42; 32]);
    let (view, objects) = fixture(1, 100);
    let published = vault::publish(
        &view,
        &objects,
        &identity.to_public(),
        &writer,
        &root.join("stage"),
        &root.join("vault"),
        None,
        true,
        Fault::None,
    )?;
    fs::write(root.join("snapshot-id.txt"), &published.id)?;
    fs::write(root.join("recipient.txt"), identity.to_public().to_string())?;
    // Small independent interoperability object, encrypted by Rust and opened
    // by the pinned Go implementation; Go also encrypts the inverse fixture.
    fs::write(
        root.join("interop.age"),
        age::encrypt(
            &identity.to_public(),
            b"VCP synthetic interoperability v1\r\n",
        )?,
    )?;
    println!(
        "{}",
        serde_json::json!({"status":"pass","snapshot":published.id,"ciphertext_bytes":published.transferred})
    );
    Ok(())
}
// PUBLIC TEST VECTOR from age 0.11.2 src/x25519.rs tests (MIT/Apache-2.0).
// This published identity is deliberately compromised. Never use for user data.
fn public_fixture_identity() -> Identity {
    "AGE-SECRET-KEY-1GQ9778VQXMMJVE8SK7J6VT8UJ4HDQAJUVSFCWCM02D8GEWQ72PVQ2Y5J33"
        .parse()
        .expect("public test vector")
}
async fn handoff_consume(root: &Path) -> Result<()> {
    let identity: Identity = fs::read_to_string(root.join("recovery/identity.txt"))?
        .parse()
        .map_err(reject)?;
    let id = fs::read_to_string(root.join("snapshot-id.txt"))?;
    let restored = vault::restore(&root.join("vault"), &id, &identity, &trust())?;
    let (view, objects) = fixture(1, 100);
    if restored.view != view || restored.artifacts != objects {
        return Err(reject("handoff fixture differs"));
    }
    if root.join("go.age").exists()
        && vault::decrypt(&identity, &fs::read(root.join("go.age"))?)?
            != b"VCP synthetic interoperability v1\r\n"
    {
        return Err(reject("Go-to-Rust interoperability mismatch"));
    }
    for kind in ["sqlite", "files"] {
        let mut store = Store::open(&root.join(format!("restored-{kind}")), kind).await?;
        for bytes in restored.artifacts.values() {
            store.put_artifact(bytes)?;
        }
        store.commit(0, restored.view.clone()).await?;
        store.close().await?;
        let store = Store::open(&root.join(format!("restored-{kind}")), kind).await?;
        if store.view() != Some(view.clone()) {
            return Err(reject("handoff store mismatch"));
        }
    }
    let search = search::rebuild(&view, &root.join("restored-search"))?;
    println!(
        "{}",
        serde_json::json!({"status":"pass","restored_sequence":view.sequence,"backends":["sqlite","files"],"search":search})
    );
    Ok(())
}
