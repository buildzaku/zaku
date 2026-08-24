use anyhow::Context as _;
use futures::{AsyncReadExt, TryStreamExt};
use gpui::{App, AppContext};
use reqwest::{
    Method,
    multipart::{Form, Part},
};
use smol::stream::StreamExt;
use std::{ffi::OsStr, sync::Arc};

use client::{Client, telemetry::MINIDUMP_ENDPOINT};
use crash_diagnostics::CrashInfo;
use http_client::{AsyncBody, HttpClient, Request};
use system_specs::GpuInfo;

pub(crate) fn init(client: Arc<Client>, cx: &mut App) {
    cx.background_spawn(async move {
        if let Err(error) = upload_previous_minidumps(client).await {
            log::warn!("Failed to upload previous minidumps: {error:#}");
        }
    })
    .detach();
}

async fn upload_previous_minidumps(client: Arc<Client>) -> anyhow::Result<()> {
    let Some(minidump_endpoint) = MINIDUMP_ENDPOINT.as_ref() else {
        log::warn!("Minidump endpoint not set");
        return Ok(());
    };

    let mut children = smol::fs::read_dir(path::logs_dir())
        .await
        .context("failed to read logs directory")?;
    while let Some(child) = children.next().await {
        let child = match child {
            Ok(child) => child,
            Err(error) => {
                log::error!("Failed to read crash diagnostics directory entry: {error:#}");
                continue;
            }
        };
        let minidump_path = child.path();
        if minidump_path.extension() != Some(OsStr::new("dmp")) {
            continue;
        }

        let mut metadata_path = minidump_path.clone();
        metadata_path.set_extension("json");
        let metadata = match smol::fs::read(&metadata_path).await {
            Ok(metadata) => metadata,
            Err(error) => {
                log::error!(
                    "Failed to read crash metadata at {}: {error:#}",
                    metadata_path.display()
                );
                continue;
            }
        };
        let metadata: CrashInfo = match serde_json::from_slice(&metadata) {
            Ok(metadata) => metadata,
            Err(error) => {
                log::error!(
                    "Failed to parse crash metadata at {}: {error:#}",
                    metadata_path.display()
                );
                continue;
            }
        };
        let minidump = match smol::fs::read(&minidump_path).await {
            Ok(minidump) => minidump,
            Err(error) => {
                log::error!(
                    "Failed to read minidump at {}: {error:#}",
                    minidump_path.display()
                );
                continue;
            }
        };

        if let Err(error) =
            upload_minidump(client.clone(), minidump_endpoint, minidump, &metadata).await
        {
            log::error!(
                "Failed to upload minidump at {}: {error:#}",
                minidump_path.display()
            );
            continue;
        }

        telemetry::event!(
            "Minidump Uploaded",
            crashed_version = metadata.init.app_version.clone(),
            commit_sha = metadata.init.commit_sha.clone(),
        );

        if let Err(error) = smol::fs::remove_file(&minidump_path).await {
            log::error!(
                "Failed to remove uploaded minidump at {}: {error:#}",
                minidump_path.display()
            );
        }
        if let Err(error) = smol::fs::remove_file(&metadata_path).await {
            log::error!(
                "Failed to remove uploaded crash metadata at {}: {error:#}",
                metadata_path.display()
            );
        }
    }

    Ok(())
}

async fn upload_minidump(
    client: Arc<Client>,
    endpoint: &str,
    minidump: Vec<u8>,
    metadata: &CrashInfo,
) -> anyhow::Result<()> {
    let mut form = Form::new()
        .part(
            "upload_file_minidump",
            Part::bytes(minidump)
                .file_name("minidump.dmp")
                .mime_str("application/octet-stream")?,
        )
        .text(
            "sentry[tags][channel]",
            metadata.init.release_channel.clone(),
        )
        .text("sentry[tags][version]", metadata.init.app_version.clone())
        .text("sentry[tags][binary]", metadata.init.binary.clone())
        .text("sentry[release]", metadata.init.commit_sha.clone())
        .text("platform", "rust");

    if let Some(panic_info) = metadata.panic.as_ref() {
        form = form
            .text("sentry[logentry][formatted]", panic_info.message.clone())
            .text("span", panic_info.span.clone());
    }
    if let Some(minidump_error) = metadata.minidump_error.clone() {
        form = form.text("minidump_error", minidump_error);
    }
    if let Some(abort_message) = metadata.abort_message.as_ref() {
        // Sentry tag values are limited to 200 characters and must not contain newlines.
        let first_line = abort_message.lines().next().unwrap_or(abort_message);
        let tag: String = first_line.chars().take(200).collect();
        form = form
            .text("sentry[tags][abort_message]", tag)
            .text("sentry[contexts][abort][message]", abort_message.clone());
    }
    if let Some(installation_id) = client.telemetry().installation_id() {
        form = form.text(
            "sentry[user][id]",
            format!("installation-{installation_id}"),
        );
    }

    let gpu_count = metadata.gpus.len();
    for (index, gpu) in metadata.gpus.iter().cloned().enumerate() {
        let GpuInfo {
            device_name,
            device_pci_id,
            vendor_name,
            vendor_pci_id,
            driver_version,
            driver_name,
        } = gpu;
        let number = if gpu_count == 1 && metadata.active_gpu.is_none() {
            String::new()
        } else {
            index.to_string()
        };
        let name = format!("gpu{number}");
        let root = format!("sentry[contexts][{name}]");
        form = form
            .text(
                format!("{root}[Description]"),
                "A GPU found on the user's system. It may or may not be the GPU Zaku is using",
            )
            .text(format!("{root}[type]"), "gpu")
            .text(format!("{root}[name]"), device_name.unwrap_or(name))
            .text(format!("{root}[id]"), format!("{device_pci_id:#06x}"))
            .text(
                format!("{root}[vendor_id]"),
                format!("{vendor_pci_id:#06x}"),
            );
        if let Some(vendor_name) = vendor_name {
            form = form.text(format!("{root}[vendor_name]"), vendor_name);
        }
        if let Some(driver_version) = driver_version {
            form = form.text(format!("{root}[driver_version]"), driver_version);
        }
        if let Some(driver_name) = driver_name {
            form = form.text(format!("{root}[driver_name]"), driver_name);
        }
    }
    if let Some(active_gpu) = metadata.active_gpu.clone() {
        form = form
            .text(
                "sentry[contexts][Active_GPU][Description]",
                "The GPU Zaku is using",
            )
            .text("sentry[contexts][Active_GPU][type]", "gpu")
            .text("sentry[contexts][Active_GPU][name]", active_gpu.device_name)
            .text(
                "sentry[contexts][Active_GPU][driver_version]",
                active_gpu.driver_info,
            )
            .text(
                "sentry[contexts][Active_GPU][driver_name]",
                active_gpu.driver_name,
            )
            .text(
                "sentry[contexts][Active_GPU][is_software_emulated]",
                active_gpu.is_software_emulated.to_string(),
            );
    }

    let content_type = format!("multipart/form-data; boundary={}", form.boundary());
    let mut body_bytes = Vec::new();
    let mut stream = form
        .into_stream()
        .map_err(std::io::Error::other)
        .into_async_read();
    stream.read_to_end(&mut body_bytes).await?;
    let request = Request::builder()
        .method(Method::POST)
        .uri(endpoint)
        .header("Content-Type", content_type)
        .body(AsyncBody::from(body_bytes))?;
    let mut response = client.http_client().send(request).await?;
    let status = response.status();
    let mut response_text = String::new();
    response
        .body_mut()
        .read_to_string(&mut response_text)
        .await?;
    if !status.is_success() {
        anyhow::bail!("failed to upload minidump: HTTP {status}: {response_text}");
    }
    log::info!("Uploaded minidump with event ID: {response_text}");

    Ok(())
}
