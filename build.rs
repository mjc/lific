use std::path::Path;

/// The Windows application manifest. `consoleAllocationPolicy=detached` keeps
/// Windows 11 24H2+ from opening a console window when Explorer starts lific
/// at logon (the background service's Run entry), while a terminal still gets
/// an ordinary attached, waited-for CLI. Older Windows ignores the element.
/// Deliberately minimal: no supportedOS, code page or long-path settings, so
/// nothing else about the binary's runtime behavior changes.
const WINDOWS_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <consoleAllocationPolicy xmlns="http://schemas.microsoft.com/SMI/2024/WindowsSettings">detached</consoleAllocationPolicy>
    </windowsSettings>
  </application>
</assembly>
"#;

fn embed_windows_manifest() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out_dir = std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR for build scripts");
    let path = Path::new(&out_dir).join("lific.exe.manifest");
    std::fs::write(&path, WINDOWS_MANIFEST).expect("write the Windows manifest");
    embed_manifest::embed_manifest_file(&path).expect("embed the Windows manifest");
}

fn main() {
    embed_windows_manifest();
}
