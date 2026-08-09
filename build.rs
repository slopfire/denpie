use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Error, Result};
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

#[derive(Deserialize)]
struct ApiManifest {
    operations: Vec<ApiOperation>,
    result_fields: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct ApiOperation {
    operation: String,
}

fn rust_variant(name: &str) -> String {
    name.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(char::to_uppercase)
                .into_iter()
                .flatten()
                .chain(chars)
                .collect::<String>()
        })
        .collect()
}

fn generate_api_contract() -> Result<()> {
    let source = fs::read_to_string("api/operations-v1.json")?;
    let manifest: ApiManifest = serde_json::from_str(&source).map_err(Error::other)?;
    let operation_names: BTreeSet<_> = manifest
        .operations
        .iter()
        .map(|operation| operation.operation.as_str())
        .collect();
    let result_names: BTreeSet<_> = manifest.result_fields.keys().map(String::as_str).collect();
    if operation_names != result_names {
        return Err(Error::other(
            "api result_fields must cover every operation exactly",
        ));
    }

    let expected_arms = manifest
        .operations
        .iter()
        .map(|operation| {
            let variant = rust_variant(&operation.operation);
            let result = &manifest.result_fields[&operation.operation];
            format!("        super::pb::api_request::Op::{variant}(_) => \"{result}\",\n")
        })
        .collect::<String>();
    let actual_arms = manifest
        .result_fields
        .values()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|result| {
            let variant = rust_variant(result);
            format!("        super::pb::api_response::Result::{variant}(_) => \"{result}\",\n")
        })
        .collect::<String>();
    let generated = format!(
        "// Generated from api/operations-v1.json by build.rs.\n\
         pub(crate) fn expected_result_field(op: &super::pb::api_request::Op) -> &'static str {{\n\
             match op {{\n{expected_arms}    }}\n\
         }}\n\n\
         pub(crate) fn actual_result_field(response: &super::pb::ApiResponse) -> Option<&'static str> {{\n\
             Some(match response.result.as_ref()? {{\n{actual_arms}    }})\n\
         }}\n"
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    fs::write(output.join("api_contract.rs"), generated)
}

fn main() -> Result<()> {
    let sha = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=proto/denpie.proto");
    println!("cargo:rerun-if-changed=api/operations-v1.json");
    println!("cargo:rustc-env=DENPIE_BUILD_SHA={sha}");
    let mut config = prost_build::Config::new();
    config.type_attribute(".denpie.ApiRequest", "#[allow(clippy::large_enum_variant)]");
    config.type_attribute(
        ".denpie.ApiResponse",
        "#[allow(clippy::large_enum_variant)]",
    );
    config.boxed(".denpie.ApiV1Response.outcome.success");
    config.compile_protos(&["proto/denpie.proto"], &["proto/"])?;
    generate_api_contract()?;
    Ok(())
}
