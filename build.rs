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

/// Gives the Windows main thread the same 8 MB stack Linux and macOS give it,
/// instead of the 1 MB MSVC default. Startup runs clap's derived builders on
/// that thread, and unoptimized builds keep every builder step in its own
/// stack slot: `Command::augment_subcommands` alone reserves over 400 KB, and
/// it grows with each CLI argument. Debug binaries were one subcommand away
/// from overflowing before `--version` could print.
fn reserve_windows_main_stack() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }
}

fn main() {
    embed_windows_manifest();
    reserve_windows_main_stack();
}
