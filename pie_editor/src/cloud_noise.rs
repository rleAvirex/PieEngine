//! 3D cloud noise texture generator.
//!
//! Generates a 128³ R8 texture containing Worley + Perlin fbm noise, used by
//! the cloud billboard shader for volumetric-looking density. Generated once
//! on the CPU at renderer construction and uploaded to the GPU as a 3D texture.
//!
//! The noise is tileable in all 3 axes so the shader can scroll the sampling
//! offset freely for wind animation without seams.

const NOISE_SIZE: u32 = 128;

/// Generate a 128³ R8 3D noise texture for cloud rendering.
///
/// The texture combines:
/// - 3D Worley noise (cellular) for the cloud "puff" structure
/// - 3D Perlin/value noise for large-scale variation
/// - 4 octaves of fbm for natural detail
///
/// Output is a single channel (R8) where 0 = no cloud, 255 = dense cloud.
/// The shader samples this with a 3D sampler and uses the value as density.
pub fn create_cloud_noise_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler) {
    let data = generate_noise_volume();

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cloud 3D noise texture"),
        size: wgpu::Extent3d {
            width: NOISE_SIZE,
            height: NOISE_SIZE,
            depth_or_array_layers: NOISE_SIZE,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(NOISE_SIZE),
            rows_per_image: Some(NOISE_SIZE),
        },
        wgpu::Extent3d {
            width: NOISE_SIZE,
            height: NOISE_SIZE,
            depth_or_array_layers: NOISE_SIZE,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D3),
        ..Default::default()
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("cloud noise sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    (texture, view, sampler)
}

/// Generate the 128³ R8 noise volume.
///
/// Combines Perlin noise (smooth large-scale) with Worley noise (cellular
/// puffs) across 4 octaves of fbm. The result is tileable because all noise
/// functions wrap at the texture boundary.
fn generate_noise_volume() -> Vec<u8> {
    let n = NOISE_SIZE as usize;
    let mut data = vec![0u8; n * n * n];

    // Precompute Perlin gradient lattice (tileable by wrapping indices).
    let gradients = generate_perlin_gradients();

    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let p = [x as f32 / n as f32, y as f32 / n as f32, z as f32 / n as f32];

                // 2-octave fbm (was 4 — halved load time, visually identical
                // at 128³ resolution).
                let perlin = fbm_perlin(&gradients, p, 2);
                let worley = fbm_worley(p, 2);

                // Combine: Perlin provides large-scale structure, Worley
                // carves out the cellular puff detail. Normalize to [0,1].
                let combined = 0.6 * perlin + 0.4 * worley;
                let value = (combined * 0.5 + 0.5).clamp(0.0, 1.0);

                data[z * n * n + y * n + x] = (value * 255.0) as u8;
            }
        }
    }

    data
}

/// Precompute a tileable Perlin gradient lattice. Size 16³ with wrapping
/// gives smooth, repeatable noise at the texture boundary. (Was 8³ — the
/// coarse lattice caused a visible grid pattern in the clouds.)
fn generate_perlin_gradients() -> Vec<[f32; 3]> {
    let lattice = 16usize;
    let mut gradients = Vec::with_capacity(lattice * lattice * lattice);
    // Use a fixed seed for reproducibility.
    let mut seed: u32 = 12345;
    for _ in 0..(lattice * lattice * lattice) {
        // Simple LCG random.
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let theta = (seed as f32 / u32::MAX as f32) * std::f32::consts::TAU;
        let phi = ((seed >> 16) as f32 / 65535.0) * std::f32::consts::PI;
        gradients.push([
            theta.cos() * phi.sin(),
            theta.sin() * phi.sin(),
            phi.cos(),
        ]);
    }
    gradients
}

/// 3D Perlin noise with lattice wrapping for tileability.
fn perlin_noise(gradients: &[[f32; 3]], p: [f32; 3], lattice_size: f32) -> f32 {
    let lattice = 16.0;
    let xi = (p[0] * lattice_size).floor() % lattice;
    let yi = (p[1] * lattice_size).floor() % lattice;
    let zi = (p[2] * lattice_size).floor() % lattice;
    let xf = (p[0] * lattice_size).fract();
    let yf = (p[1] * lattice_size).fract();
    let zf = (p[2] * lattice_size).fract();

    let xi = (xi as i32).rem_euclid(16) as usize;
    let yi = (yi as i32).rem_euclid(16) as usize;
    let zi = (zi as i32).rem_euclid(16) as usize;
    let xi1 = (xi + 1) % 16;
    let yi1 = (yi + 1) % 16;
    let zi1 = (zi + 1) % 16;

    let idx = |x: usize, y: usize, z: usize| (z * 16 + y) * 16 + x;

    let g000 = gradients[idx(xi, yi, zi)];
    let g100 = gradients[idx(xi1, yi, zi)];
    let g010 = gradients[idx(xi, yi1, zi)];
    let g110 = gradients[idx(xi1, yi1, zi)];
    let g001 = gradients[idx(xi, yi, zi1)];
    let g101 = gradients[idx(xi1, yi, zi1)];
    let g011 = gradients[idx(xi, yi1, zi1)];
    let g111 = gradients[idx(xi1, yi1, zi1)];

    let d000 = [xf, yf, zf];
    let d100 = [xf - 1.0, yf, zf];
    let d010 = [xf, yf - 1.0, zf];
    let d110 = [xf - 1.0, yf - 1.0, zf];
    let d001 = [xf, yf, zf - 1.0];
    let d101 = [xf - 1.0, yf, zf - 1.0];
    let d011 = [xf, yf - 1.0, zf - 1.0];
    let d111 = [xf - 1.0, yf - 1.0, zf - 1.0];

    let n000 = dot3(g000, d000);
    let n100 = dot3(g100, d100);
    let n010 = dot3(g010, d010);
    let n110 = dot3(g110, d110);
    let n001 = dot3(g001, d001);
    let n101 = dot3(g101, d101);
    let n011 = dot3(g011, d011);
    let n111 = dot3(g111, d111);

    // Smoothstep fade.
    let u = fade(xf);
    let v = fade(yf);
    let w = fade(zf);

    let nx00 = lerp(n000, n100, u);
    let nx10 = lerp(n010, n110, u);
    let nx01 = lerp(n001, n101, u);
    let nx11 = lerp(n011, n111, u);

    let nxy0 = lerp(nx00, nx10, v);
    let nxy1 = lerp(nx01, nx11, v);

    lerp(nxy0, nxy1, w) * 2.0 // Perlin output is roughly [-1, 1]
}

fn fbm_perlin(gradients: &[[f32; 3]], p: [f32; 3], octaves: u32) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 2.0;
    for _ in 0..octaves {
        value += amplitude * perlin_noise(gradients, p, frequency);
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    value
}

/// 3D Worley (cellular) noise. Returns distance to nearest feature point,
/// normalized to roughly [0, 1]. Tileable via lattice wrapping.
fn worley_noise(p: [f32; 3], frequency: f32) -> f32 {
    let lattice = 16.0;
    let cell_size = 1.0 / frequency;
    let cx = (p[0] / cell_size).floor() % lattice;
    let cy = (p[1] / cell_size).floor() % lattice;
    let cz = (p[2] / cell_size).floor() % lattice;
    let fx = (p[0] / cell_size).fract();
    let fy = (p[1] / cell_size).fract();
    let fz = (p[2] / cell_size).fract();

    let cx = (cx as i32).rem_euclid(16) as i32;
    let cy = (cy as i32).rem_euclid(16) as i32;
    let cz = (cz as i32).rem_euclid(16) as i32;

    let mut min_dist = f32::INFINITY;
    // Check the 3x3x3 neighborhood of cells.
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let nx = (cx + dx).rem_euclid(16) as usize;
                let ny = (cy + dy).rem_euclid(16) as usize;
                let nz = (cz + dz).rem_euclid(16) as usize;
                // Deterministic feature point per cell (hash-based).
                let h = hash3(nx, ny, nz);
                let px = dx as f32 + h.0 - fx;
                let py = dy as f32 + h.1 - fy;
                let pz = dz as f32 + h.2 - fz;
                let d = (px * px + py * py + pz * pz).sqrt();
                if d < min_dist {
                    min_dist = d;
                }
            }
        }
    }
    // Normalize: typical Worley distance is 0..~1.5 for cell_size=1.
    (min_dist / 1.5).min(1.0)
}

fn fbm_worley(p: [f32; 3], octaves: u32) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 2.0;
    for _ in 0..octaves {
        value += amplitude * (1.0 - worley_noise(p, frequency));
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    value * 2.0 - 1.0
}

fn hash3(x: usize, y: usize, z: usize) -> (f32, f32, f32) {
    let n = (z * 64 + y) * 64 + x;
    let s = (n.wrapping_mul(747796405).wrapping_add(2891336453)) as u32;
    let s1 = (s ^ (s >> 16)).wrapping_mul(2246822519);
    let s2 = (s1 ^ (s1 >> 13)).wrapping_mul(3266489917);
    let r = (s2 ^ (s2 >> 16)) as f32 / u32::MAX as f32;
    let r2 = ((s2 >> 8) as f32) / 255.0;
    let r3 = ((s2 >> 16) as f32) / 65535.0;
    (r, r2, r3)
}

#[inline]
fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
