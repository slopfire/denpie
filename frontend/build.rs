use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=../proto/denpie.proto");
    let mut config = prost_build::Config::new();
    config.type_attribute(".denpie.ApiRequest", "#[allow(clippy::large_enum_variant)]");
    config.type_attribute(
        ".denpie.ApiResponse",
        "#[allow(clippy::large_enum_variant)]",
    );
    config.boxed(".denpie.ApiV1Response.outcome.success");
    config.compile_protos(&["../proto/denpie.proto"], &["../proto/"])?;
    Ok(())
}
