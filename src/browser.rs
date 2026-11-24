//! In-memory browser entry point; input programs are never executed.
use crate::{common::pe::Pe, devirt::Analysis, unpack};
use anyhow::{Context, Result, bail, ensure};
use std::io::{Cursor, Write};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Debug)]
pub struct Output {
    name: String,
    data: Vec<u8>,
    text: String,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Output {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn name(&self) -> String {
        self.name.clone()
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn data(&self) -> Vec<u8> {
        self.data.clone()
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn text(&self) -> String {
        self.text.clone()
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn process(
    input: &[u8],
    name: &str,
    mode: &str,
    rva: &str,
    binary: bool,
    backend: &str,
) -> Result<Output, JsValue> {
    let result = async {
        validate_name(name)?;
        if mode == "unpack" {
            let mode = match backend {
                "auto" => None,
                "cpu" => Some(unpack::search::Mode::Cpu),
                "wgpu" => Some(unpack::search::Mode::Wgpu),
                _ => bail!("unknown search backend"),
            };
            archive(unpack::unpack_async(input, mode).await?, name)
        } else {
            run(input, name, mode, rva, binary)
        }
    }
    .await;
    result.map_err(|error| JsValue::from_str(&format!("{error:#}")))
}

fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..",
        "invalid input filename"
    );
    Ok(())
}

fn archive(result: unpack::Unpacked, name: &str) -> Result<Output> {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    archive.start_file(name, options)?;
    archive.write_all(&result.executable)?;
    for file in &result.files {
        let path = file
            .path
            .to_str()
            .context("non-UTF-8 EVB path")?
            .replace('\\', "/");
        ensure!(
            !path.eq_ignore_ascii_case(name),
            "EVB file conflicts with output executable"
        );
        archive.start_file(path, options)?;
        archive.write_all(&file.data)?;
    }
    Ok(Output {
        name: format!("{name}.unpacked.zip"),
        data: archive.finish()?.into_inner(),
        text: format!(
            "Recovered executable and {} EVB files. See log for EVB confirmation status.",
            result.files.len()
        ),
    })
}

fn run(input: &[u8], name: &str, mode: &str, rva: &str, binary: bool) -> Result<Output> {
    validate_name(name)?;
    match mode {
        "unpack" => {
            let result = unpack::unpack(input, Some(unpack::search::Mode::Cpu))?;
            archive(result, name)
        }
        "runtime" => Ok(Output {
            name: "runtime.dll".into(),
            data: unpack::extract_runtime(&Pe::parse(input)?)?,
            text: "Decoded RVA-mapped runtime DLL. No host unpacking or salt search performed."
                .into(),
        }),
        "scan" | "devirt" => {
            let analysis = Analysis::parse(input)?;
            let (data, text, suffix) = if mode == "scan" {
                let text = analysis
                    .entries()
                    .iter()
                    .map(|entry| format!("{entry:#010x}\n"))
                    .collect::<String>();
                (text.as_bytes().to_vec(), text, "entries.txt")
            } else {
                let value = rva
                    .strip_prefix("0x")
                    .or_else(|| rva.strip_prefix("0X"))
                    .unwrap_or(rva);
                let address =
                    u32::from_str_radix(value, 16).context("invalid 32-bit hexadecimal RVA")?;
                let code = analysis.devirtualize(address)?;
                let text = code.assembly()?;
                let data = if binary {
                    code.bytes().to_vec()
                } else {
                    text.as_bytes().to_vec()
                };
                (data, text, if binary { "bin" } else { "asm" })
            };
            Ok(Output {
                name: format!("{name}.{suffix}"),
                data,
                text,
            })
        }
        _ => bail!("unknown operation"),
    }
}

#[cfg(test)]
#[path = "../tests/browser.rs"]
mod tests;
