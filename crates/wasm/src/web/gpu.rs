use std::cell::RefCell;

use bytemuck::{Pod, Zeroable};
use futures_channel::oneshot;
use wgpu::util::DeviceExt;

use super::protocol::SearchRequest;

const WORKGROUP_SIZE: u32 = 128;
const CANDIDATES_PER_INVOCATION: u32 = 8;
const CANDIDATES_PER_WORKGROUP: u64 = WORKGROUP_SIZE as u64 * CANDIDATES_PER_INVOCATION as u64;
pub(super) const GPU_BATCH_SIZE: u64 = 1 << 24;
const GPU_WARM_UP_SIZE: u32 = 1 << 20;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuParams {
    base_lo: u32,
    base_hi: u32,
    candidate_count: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuResult {
    min_offset: u32,
    padding: [u32; 3],
}

pub(super) struct GpuSession {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    result: wgpu::Buffer,
    readback: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    adapter_name: String,
}

impl GpuSession {
    #[allow(clippy::single_call_fn)]
    pub(super) async fn new(request: SearchRequest) -> Result<Self, String> {
        if request.debug {
            web_sys::console::debug_1(&"[wasm:webgpu] requesting browser adapter".into());
        }
        let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU;
        let instance = wgpu::Instance::new(instance_descriptor);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|error| format!("WebGPU adapter request failed: {error}"))?;
        let adapter_name = adapter.get_info().name;
        if request.debug {
            web_sys::console::debug_1(
                &format!("[wasm:webgpu] adapter selected: {adapter_name}").into(),
            );
        }
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("xray-qbrute browser WebGPU device"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("WebGPU device request failed: {error}"))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("xray-qbrute browser SHA-512 WGSL"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!(concat!(env!("OUT_DIR"), "/sha512_web.wgsl")).into(),
            ),
        });
        let constants = [
            ("COMMIT", f64::from(request.commit)),
            ("NODE_SUFFIX", f64::from(request.node_suffix)),
            ("LEADING_ZERO_BITS", f64::from(request.difficulty)),
            ("WORKGROUP_SIZE", f64::from(WORKGROUP_SIZE)),
            (
                "CANDIDATES_PER_INVOCATION",
                f64::from(CANDIDATES_PER_INVOCATION),
            ),
        ];
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("xray-qbrute browser search pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("search_main"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                zero_initialize_workgroup_memory: false,
            },
            cache: None,
        });
        if request.debug {
            web_sys::console::debug_1(&"[wasm:webgpu] pipeline created".into());
        }
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xray-qbrute browser search params"),
            size: size_of::<GpuParams>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let result = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("xray-qbrute browser search result"),
            contents: bytemuck::bytes_of(&GpuResult {
                min_offset: u32::MAX,
                padding: [0; 3],
            }),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xray-qbrute browser search readback"),
            size: size_of::<GpuResult>() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("xray-qbrute browser search bind group"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: result.as_entire_binding(),
                },
            ],
        });
        let session = Self {
            device,
            queue,
            pipeline,
            params,
            result,
            readback,
            bind_group,
            adapter_name,
        };
        session.dispatch(0, GPU_WARM_UP_SIZE).await?;
        if request.debug {
            web_sys::console::debug_1(
                &format!("[wasm:webgpu] warm-up complete candidates={GPU_WARM_UP_SIZE}").into(),
            );
        }
        Ok(session)
    }

    pub(super) fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    pub(super) async fn dispatch(&self, base: u64, count: u32) -> Result<u32, String> {
        self.queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&GpuParams {
                base_lo: base as u32,
                base_hi: (base >> 32) as u32,
                candidate_count: count,
                reserved: 0,
            }),
        );
        self.queue.write_buffer(
            &self.result,
            0,
            bytemuck::bytes_of(&GpuResult {
                min_offset: u32::MAX,
                padding: [0; 3],
            }),
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(
                u64::from(count).div_ceil(CANDIDATES_PER_WORKGROUP) as u32,
                1,
                1,
            );
        }
        encoder.copy_buffer_to_buffer(
            &self.result,
            0,
            &self.readback,
            0,
            size_of::<GpuResult>() as u64,
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = self.readback.slice(..);
        let (sender, receiver) = oneshot::channel();
        let sender = RefCell::new(Some(sender));
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if let Some(sender) = sender.take() {
                let _ = sender.send(result);
            }
        });
        receiver
            .await
            .map_err(|_| "WebGPU readback callback was dropped".to_owned())?
            .map_err(|error| format!("WebGPU readback failed: {error}"))?;
        let view = slice
            .get_mapped_range()
            .map_err(|error| format!("WebGPU mapped range failed: {error}"))?;
        let result = bytemuck::pod_read_unaligned::<GpuResult>(&view).min_offset;
        drop(view);
        self.readback.unmap();
        Ok(result)
    }
}
