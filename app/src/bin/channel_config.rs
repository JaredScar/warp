//! Tools for loading a [`ChannelConfig`] from the external config generator binary.
//!
//! For non-bundled builds, the generator is invoked at runtime. For bundled builds, the config
//! is embedded at compile time via the build script.
use warp_core::channel::ChannelConfig;

/// The name of the config generator binary, expected to be on PATH.
const CONFIG_BIN_NAME: &str = "warp-channel-config";

#[macro_export]
#[cfg(windows)]
macro_rules! path_concat {
    ($path:expr, $file:expr) => {
        concat!($path, "\\", $file)
    };
}
#[macro_export]
#[cfg(not(windows))]
macro_rules! path_concat {
    ($path:expr, $file:expr) => {
        concat!($path, "/", $file)
    };
}

#[macro_export]
macro_rules! load_config {
    ($channel:expr) => {{
        #[cfg(feature = "release_bundle")]
        {
            channel_config::load_config_from_embedded(include_str!($crate::path_concat!(
                env!("OUT_DIR"),
                concat!($channel, "_config.json")
            )))
        }

        #[cfg(not(feature = "release_bundle"))]
        {
            channel_config::load_config_from_generator($channel)
        }
    }};
}
pub use load_config;

/// Returns a default [`ChannelConfig`] suitable for the OSS build when the
/// external config generator binary is unavailable.
fn default_oss_config() -> ChannelConfig {
    use warp_core::channel::{OzConfig, WarpServerConfig};
    ChannelConfig {
        app_id: warp_core::AppId::new("dev", "warp", "WarpOss"),
        logfile_name: "warp.log".into(),
        server_config: WarpServerConfig::production(),
        oz_config: OzConfig::production(),
        telemetry_config: None,
        autoupdate_config: None,
        crash_reporting_config: None,
        mcp_static_config: None,
    }
}

/// Invokes the config generator binary at runtime and deserializes its JSON output into a
/// [`ChannelConfig`].  Falls back to a hardcoded OSS default if the binary is not found.
#[cfg_attr(feature = "release_bundle", expect(dead_code))]
pub fn load_config_from_generator(channel: &str) -> ChannelConfig {
    let target_family = if cfg!(target_family = "wasm") {
        "wasm"
    } else {
        "native"
    };

    let target_os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };

    let result = command::blocking::Command::new(CONFIG_BIN_NAME)
        .arg("--channel")
        .arg(channel)
        .arg("--target-family")
        .arg(target_family)
        .arg("--target-os")
        .arg(target_os)
        .output();

    match result {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            log::warn!(
                "'{CONFIG_BIN_NAME}' not found on PATH — using built-in OSS defaults. \
                 Run ./script/install_channel_config for full channel configuration."
            );
            default_oss_config()
        }
        Err(err) => {
            log::error!("Failed to execute '{CONFIG_BIN_NAME}': {err} — falling back to OSS defaults.");
            default_oss_config()
        }
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("Config generator failed for channel '{channel}':\n{stderr} — falling back to OSS defaults.");
            default_oss_config()
        }
        Ok(output) => {
            serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
                let stdout = String::from_utf8_lossy(&output.stdout);
                log::error!(
                    "Failed to parse config generator output for channel '{channel}': {err}\nOutput:\n{stdout} — falling back to OSS defaults."
                );
                default_oss_config()
            })
        }
    }
}

/// Deserializes a [`ChannelConfig`] from a JSON string embedded at compile time.
///
/// This is used to load the channel configuration in release bundles, where configuration
/// is embedded at compile time instead of being generated at runtime.
#[cfg_attr(not(feature = "release_bundle"), expect(dead_code))]
pub fn load_config_from_embedded(json: &str) -> ChannelConfig {
    serde_json::from_str(json)
        .unwrap_or_else(|err| panic!("Failed to parse embedded channel config: {err}"))
}
