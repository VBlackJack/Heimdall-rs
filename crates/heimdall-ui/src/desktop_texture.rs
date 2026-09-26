/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! A remote desktop on the GPU renderer: one texture per tab, rewritten in place.
//!
//! An iced image is immutable, so a desktop drawn as an image needs a new one at every
//! server update, and the GPU renderer then draws nothing for some frames: measured on
//! 2026-09-26 against the xrdp login screen, 13 to 23 frames out of 40 showed the bare
//! window while the server was drawing, and none on the software renderer. A texture that
//! stays and is only rewritten has no such gap.

use std::collections::HashMap;
use std::fmt;

use heimdall_app::DesktopFramebuffer;
use heimdall_app::TabId;
use iced::Rectangle;
use iced::wgpu;
use iced::widget::shader::{self, Viewport};

/// Bytes of one pixel of the framebuffer (RGBA).
const PIXEL_BYTES: u32 = 4;

/// Draws the whole texture over the area it is given: one triangle covering the viewport,
/// sampled pixel for pixel. The desktop is opaque, whatever alpha the decoder left.
const SHADER: &str = r"
struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var desktop: texture_2d<f32>;
@group(0) @binding(1) var nearest: sampler;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Varying {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: Varying;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: Varying) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(desktop, nearest, in.uv).rgb, 1.0);
}
";

/// The desktop of one tab, as the GPU renderer draws it.
pub struct Desktop {
    /// The tab, which keys its texture.
    pub tab: TabId,
    /// The pixels.
    pub framebuffer: DesktopFramebuffer,
    /// Bumped by every update of the pixels.
    pub generation: u64,
}

impl fmt::Debug for Desktop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Desktop")
            .field("tab", &self.tab)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl shader::Primitive for Desktop {
    type Pipeline = Pipeline;

    fn prepare(
        &self,
        pipeline: &mut Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        pipeline.upload(self, device, queue);
    }

    fn draw(&self, pipeline: &Pipeline, pass: &mut wgpu::RenderPass<'_>) -> bool {
        if let Some(texture) = pipeline.textures.get(&self.tab) {
            pass.set_pipeline(&pipeline.render);
            pass.set_bind_group(0, &texture.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        true
    }
}

/// A tab's texture.
struct Slot {
    width: u16,
    height: u16,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    /// The generation it holds; `None` until the first upload.
    generation: Option<u64>,
    /// Drawn since the last trim.
    used: bool,
}

/// The textures of the desktops shown, and how to draw one.
pub struct Pipeline {
    render: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: HashMap<TabId, Slot>,
}

impl Pipeline {
    /// Rewrites the texture of `desktop` if its pixels changed since the last upload.
    fn upload(&mut self, desktop: &Desktop, device: &wgpu::Device, queue: &wgpu::Queue) {
        let Self {
            layout,
            sampler,
            textures,
            ..
        } = self;
        desktop.framebuffer.read(|width, height, pixels| {
            let row = PIXEL_BYTES * u32::from(width);
            let expected = usize::try_from(row * u32::from(height)).unwrap_or(usize::MAX);
            if width == 0 || height == 0 || pixels.len() < expected {
                return;
            }
            let texture = textures
                .entry(desktop.tab)
                .and_modify(|texture| {
                    if (texture.width, texture.height) != (width, height) {
                        *texture = Slot::new(device, layout, sampler, width, height);
                    }
                })
                .or_insert_with(|| Slot::new(device, layout, sampler, width, height));
            texture.used = true;
            if texture.generation == Some(desktop.generation) {
                return;
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels[..expected],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(u32::from(height)),
                },
                extent(width, height),
            );
            texture.generation = Some(desktop.generation);
        });
    }
}

impl Slot {
    fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        width: u16,
        height: u16,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("heimdall remote desktop"),
            size: extent(width, height),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // The bytes pass through as they are: sampled as sRGB, they came out darker
            // (measured against the software renderer on the xrdp login screen).
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("heimdall remote desktop"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        Self {
            width,
            height,
            texture,
            bind_group,
            generation: None,
            used: true,
        }
    }
}

fn extent(width: u16, height: u16) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: u32::from(width),
        height: u32::from(height),
        depth_or_array_layers: 1,
    }
}

impl shader::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("heimdall remote desktop"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("heimdall remote desktop"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("heimdall remote desktop"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("heimdall remote desktop"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("heimdall remote desktop"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });
        Self {
            render: pipeline,
            layout,
            sampler,
            textures: HashMap::new(),
        }
    }

    /// Drops the textures of the desktops not drawn since the last trim: a closed tab, or
    /// one in the background, which uploads again when shown.
    fn trim(&mut self) {
        self.textures
            .retain(|_, texture| std::mem::take(&mut texture.used));
    }
}
