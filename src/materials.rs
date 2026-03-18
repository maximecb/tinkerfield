use std::fs;
use std::io::BufReader;
use std::path::Path;
use wgpu::util::DeviceExt;

/// CPU-side registry that loads and tiles textures from disk once
pub struct MaterialRegistry
{
    pub texture_datas: Vec<Vec<u8>>,
    pub specular_factors: Vec<f32>,
    pub names: Vec<String>,
}

/// GPU-side resources for materials (texture array, sampler, specular buffer)
#[allow(dead_code)]
pub struct GPUMaterials
{
    pub texture_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub specular_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl MaterialRegistry
{
    /// Load all PNG textures from the textures directory
    pub fn load() -> Self
    {
        let mut names = Vec::new();

        // Read all files in the textures directory to collect names
        let entries = fs::read_dir("textures").expect("Could not read textures directory");
        for entry in entries {
            let entry = entry.expect("Could not read directory entry");
            let path = entry.path();

            // Only process PNG files
            if path.extension().and_then(|s| s.to_str()) == Some("png") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    if name.to_lowercase() != name {
                        println!("Texture file names should be lowercase: {}", name);
                    }

                    names.push(name.to_string());
                }
            }
        }

        if names.is_empty() {
            panic!("No valid PNG textures found in textures/ directory");
        }

        // Sort concrete textures first as they are the default textures
        names.sort_by(|a, b| {
            let a_is_concrete = a.starts_with("concrete");
            let b_is_concrete = b.starts_with("concrete");

            if a_is_concrete && !b_is_concrete {
                std::cmp::Ordering::Less
            } else if !a_is_concrete && b_is_concrete {
                std::cmp::Ordering::Greater
            } else {
                a.cmp(b)
            }
        });

        let mut texture_datas = Vec::new();
        let mut specular_factors = Vec::new();

        // Load textures based on the sorted names
        for name in &names {
            let path = Path::new("textures").join(format!("{}.png", name));
            println!("Loading texture: {}.png", name);

            // Load PNG pixels and dimensions
            let (data, width, height) = load_png(&path);

            // Ensure the texture can be tiled perfectly into a 1024x1024 square
            if 1024 % width != 0 || 1024 % height != 0 {
                panic!("Texture size {}x{} in {:?} is not a divisor of 1024", width, height, path);
            }

            // Tile the texture to fill a 1024x1024 RGBA buffer
            let tiled_data = tile_texture(&data, width, height, 1024, 1024);
            texture_datas.push(tiled_data);

            // Set specular highlights based on keywords in the filename
            let spec = if name.contains("metal") {
                0.5f32 // Medium specular for metal
            } else if name.contains("glass") || name.contains("window") {
                0.8f32 // High specular for glass/windows
            } else if name.contains("concrete") {
                0.1f32
            } else {
                0.0f32
            };
            specular_factors.push(spec);
        }

        Self {
            texture_datas,
            specular_factors,
            names,
        }
    }

    /// Get the total number of loaded materials
    pub fn get_num_materials(&self) -> u32
    {
        self.texture_datas.len() as u32
    }
}

impl GPUMaterials
{
    /// Create the GPU-side texture array and buffers from the registry data
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        registry: &MaterialRegistry,
        layout: &wgpu::BindGroupLayout
    ) -> Self
    {
        let num_layers = registry.get_num_materials();

        let texture_extent = wgpu::Extent3d {
            width: 1024,
            height: 1024,
            depth_or_array_layers: num_layers,
        };

        // Create the 2D texture array on the GPU
        let texture_array = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Material Texture Array"),
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Upload each layer of the texture array
        for (i, data) in registry.texture_datas.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture_array,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: i as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * 1024),
                    rows_per_image: Some(1024),
                },
                wgpu::Extent3d { width: 1024, height: 1024, depth_or_array_layers: 1 },
            );
        }

        let texture_view = texture_array.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Material Texture View"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        // Create a standard linear sampler for materials
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Material Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Upload specular factors to a storage buffer
        let specular_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Specular Factor Buffer"),
            contents: bytemuck::cast_slice(&registry.specular_factors),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        // Create the bind group for all material-related resources
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: specular_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            texture_view,
            sampler,
            specular_buffer,
            bind_group,
        }
    }
}

/// Helper function to load a PNG and convert it to RGBA8 format
fn load_png(path: &Path) -> (Vec<u8>, u32, u32)
{
    let file = fs::File::open(path).expect("Could not open PNG file");
    let reader = BufReader::new(file);
    let decoder = png::Decoder::new(reader);
    let mut reader = decoder.read_info().expect("Could not read PNG metadata");

    let mut buf = vec![0; reader.output_buffer_size().expect("PNG output buffer size is too large")];
    let info = reader.next_frame(&mut buf).expect("Could not read PNG frame");

    let width = info.width;
    let height = info.height;

    let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);

    // Normalize color format to RGBA8
    match info.color_type {
        png::ColorType::Rgb => {
            for i in 0..(width * height) as usize {
                rgba_data.push(buf[i * 3]);
                rgba_data.push(buf[i * 3 + 1]);
                rgba_data.push(buf[i * 3 + 2]);
                rgba_data.push(255);
            }
        }
        png::ColorType::Rgba => {
            rgba_data = buf;
        }
        _ => panic!("Unsupported color type {:?} in {:?}", info.color_type, path),
    }

    (rgba_data, width, height)
}

/// Tiles a smaller texture pattern to fill a larger destination buffer
fn tile_texture(data: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8>
{
    // If the size already matches, just return a copy of the data
    if src_w == dst_w && src_h == dst_h {
        return data.to_vec();
    }

    let mut tiled = vec![0; (dst_w * dst_h * 4) as usize];
    let src_row_stride = (src_w * 4) as usize;
    let dst_row_stride = (dst_w * 4) as usize;

    // First, tile the unique source rows horizontally into the destination
    for y in 0..src_h {
        let src_row = &data[(y as usize * src_row_stride)..((y as usize + 1) * src_row_stride)];
        let dst_y_offset = y as usize * dst_row_stride;

        for x_offset in (0..dst_row_stride).step_by(src_row_stride) {
            tiled[dst_y_offset + x_offset .. dst_y_offset + x_offset + src_row_stride]
                .copy_from_slice(src_row);
        }
    }

    // Then, copy the horizontally-tiled rows vertically to fill the rest of the buffer
    for y in src_h..dst_h {
        let src_y = y % src_h;
        let src_y_offset = src_y as usize * dst_row_stride;
        let dst_y_offset = y as usize * dst_row_stride;

        let (src_part, dst_part) = tiled.split_at_mut(dst_y_offset);
        dst_part[0..dst_row_stride].copy_from_slice(&src_part[src_y_offset..src_y_offset + dst_row_stride]);
    }

    tiled
}
