// SPDX-License-Identifier: Apache-2.0
//! Private native P0 coding trace. All model responses and files are synthetic.
#[path = "support/coding_trace.rs"]
mod coding_trace;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("supply a new absolute disposable directory")?,
    );
    if std::env::args_os().count() != 2 || !directory.is_absolute() {
        return Err("one absolute disposable directory is required".into());
    }
    std::fs::create_dir(&directory)?;
    let verifier = std::env::current_exe()?
        .parent()
        .ok_or("missing example parent")?
        .parent()
        .ok_or("missing debug root")?
        .join("vcp-process-fixture.exe");
    let result = coding_trace::run(&directory, &verifier).await;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
