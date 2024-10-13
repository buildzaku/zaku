#[cfg(target_os = "linux")]
use {
    std::env,
    wgpu::{
        Backends, DeviceType, Dx12Compiler, Gles3MinorVersion, Instance, InstanceDescriptor,
        InstanceFlags,
    },
};

#[cfg(target_os = "linux")]
pub fn initialize() {
    if has_nvidia_gpu() {
        env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1"); // https://github.com/tauri-apps/tauri/issues/9304
    }
}

#[cfg(target_os = "linux")]
fn has_nvidia_gpu() -> bool {
    const NVIDIA_VENDOR_ID: u32 = 0x10DE;

    let instance = Instance::new(InstanceDescriptor {
        flags: InstanceFlags::empty(),
        backends: Backends::VULKAN | Backends::GL,
        gles_minor_version: Gles3MinorVersion::Automatic,
        dx12_shader_compiler: Dx12Compiler::default(),
    });

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
