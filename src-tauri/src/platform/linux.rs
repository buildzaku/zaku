#![cfg(target_os = "linux")]

use {
    std::env,
    wgpu::{
        BackendOptions, Backends, DeviceType, Dx12BackendOptions, Dx12Compiler, GlBackendOptions,
        Gles3MinorVersion, Instance, InstanceDescriptor, InstanceFlags,
    },
};

pub fn initialize() {
    if has_nvidia_gpu() {
        env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1"); // https://github.com/tauri-apps/tauri/issues/9304
    }
}

fn has_nvidia_gpu() -> bool {
    const NVIDIA_VENDOR_ID: u32 = 0x10DE;

    let instance_descriptor = InstanceDescriptor {
        flags: InstanceFlags::empty(),
        backends: Backends::VULKAN | Backends::GL,
        backend_options: BackendOptions {
            gl: GlBackendOptions {
                gles_minor_version: Gles3MinorVersion::Automatic,
            },
            dx12: Dx12BackendOptions {
                shader_compiler: Dx12Compiler::default(),
            },
        },
    };
    let instance = Instance::new(&instance_descriptor);

    for adapter in instance.enumerate_adapters(Backends::VULKAN | Backends::GL) {
        let info = adapter.get_info();

        match info.device_type {
            DeviceType::DiscreteGpu | DeviceType::IntegratedGpu | DeviceType::VirtualGpu => {
                if info.vendor == NVIDIA_VENDOR_ID {
                    return true;
                }
            }
            _ => {}
        }
    }

    return false;
}
