//! Fast screen grabbing on Windows through DXGI desktop duplication.
//!
//! The compositor hands over each new frame as a GPU texture. The GPU shrinks
//! it (mip-map generation is a proper box filter over every pixel) and only
//! the small result is read back, so a 4K display costs about as much as a
//! thumbnail. When nothing on screen changes, no work happens at all.

use windows::core::Interface;
use windows::Win32::Foundation::{E_FAIL, HMODULE};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET,
    D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
    D3D11_RESOURCE_MISC_GENERATE_MIPS, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION, DXGI_MODE_ROTATION_ROTATE180, DXGI_MODE_ROTATION_ROTATE270,
    DXGI_MODE_ROTATION_ROTATE90, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_WAIT_TIMEOUT,
    DXGI_OUTDUPL_FRAME_INFO,
};

type Result<T> = windows::core::Result<T>;

/// The reduced picture should stay at least this big so that the final
/// averaging down to the ambient grid has real detail to work with.
const MIN_WIDTH: u32 = 192;
const MIN_HEIGHT: u32 = 108;

/// A shrunken, upright copy of the screen as RGBA bytes.
pub struct Shot {
    pub rgba: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

struct Targets {
    src: (u32, u32),
    /// Full-size copy of the frame with a mip chain to shrink it on the GPU.
    mips: ID3D11Texture2D,
    view: ID3D11ShaderResourceView,
    /// CPU-readable copy of one small mip level.
    staging: ID3D11Texture2D,
    level: u32,
    size: (u32, u32),
}

pub struct Duplicator {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    rotation: DXGI_MODE_ROTATION,
    targets: Option<Targets>,
}

impl Duplicator {
    /// `device_name` is the display's GDI name, e.g. `\\.\DISPLAY1`.
    pub fn open(device_name: &str) -> Result<Self> {
        // SAFETY: plain COM calls; every out-pointer is a live local.
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
            let mut a = 0;
            while let Ok(adapter) = factory.EnumAdapters1(a) {
                a += 1;
                let mut o = 0;
                while let Ok(output) = adapter.EnumOutputs(o) {
                    o += 1;
                    let desc = output.GetDesc()?;
                    let len = desc.DeviceName.iter().position(|c| *c == 0).unwrap_or(desc.DeviceName.len());
                    if String::from_utf16_lossy(&desc.DeviceName[..len]) != device_name {
                        continue;
                    }
                    // The device has to live on the adapter that drives this
                    // display (laptops often have two).
                    let mut device = None;
                    let mut context = None;
                    D3D11CreateDevice(
                        &adapter,
                        D3D_DRIVER_TYPE_UNKNOWN,
                        HMODULE::default(),
                        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                        None,
                        D3D11_SDK_VERSION,
                        Some(&mut device),
                        None,
                        Some(&mut context),
                    )?;
                    let (Some(device), Some(context)) = (device, context) else {
                        return Err(E_FAIL.into());
                    };
                    let duplication = output.cast::<IDXGIOutput1>()?.DuplicateOutput(&device)?;
                    let rotation = duplication.GetDesc().Rotation;
                    return Ok(Duplicator { device, context, duplication, rotation, targets: None });
                }
            }
        }
        Err(E_FAIL.into())
    }

    /// The newest frame, or `None` if the picture has not changed since the
    /// last call (or the first frame has not arrived yet). Any error means the duplication is gone (display mode
    /// change, lock screen, ...) and the caller should open a new one.
    pub fn grab(&mut self) -> Result<Option<Shot>> {
        // SAFETY: `info` and `resource` outlive the call that fills them, and
        // the acquired frame is always released before returning.
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;
            match self.duplication.AcquireNextFrame(0, &mut info, &mut resource) {
                Ok(()) => {}
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(None),
                Err(e) => return Err(e),
            }
            // A frame without a present time only reports a mouse move and
            // carries no picture. Several of those can arrive before the first
            // real frame, so they must never be read as an image.
            let shot = match resource {
                Some(resource) if info.LastPresentTime != 0 => self.read(&resource).map(Some),
                _ => Ok(None),
            };
            let _ = self.duplication.ReleaseFrame();
            shot
        }
    }

    unsafe fn read(&mut self, resource: &IDXGIResource) -> Result<Shot> {
        let frame: ID3D11Texture2D = resource.cast()?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        frame.GetDesc(&mut desc);
        if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err(E_FAIL.into());
        }
        if self.targets.as_ref().is_none_or(|t| t.src != (desc.Width, desc.Height)) {
            self.targets = Some(self.create_targets(desc.Width, desc.Height, desc.Format)?);
        }
        let t = self.targets.as_ref().expect("targets were just created");

        self.context.CopySubresourceRegion(&t.mips, 0, 0, 0, 0, &frame, 0, None);
        self.context.GenerateMips(&t.view);
        self.context.CopySubresourceRegion(&t.staging, 0, 0, 0, 0, &t.mips, t.level, None);

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        self.context.Map(&t.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let (sw, sh) = (t.size.0 as usize, t.size.1 as usize);
        let pitch = mapped.RowPitch as usize;
        // SAFETY: a mapped staging texture exposes `RowPitch * height` readable
        // bytes until it is unmapped below.
        let surface = std::slice::from_raw_parts(mapped.pData as *const u8, pitch * (sh - 1) + sw * 4);
        let shot = upright(surface, sw, sh, pitch, self.rotation);
        self.context.Unmap(&t.staging, 0);
        Ok(shot)
    }

    unsafe fn create_targets(&self, w: u32, h: u32, format: DXGI_FORMAT) -> Result<Targets> {
        // Halve as often as possible while staying above the minimum size.
        let mut level = 0;
        while (w >> (level + 1)) >= MIN_WIDTH && (h >> (level + 1)) >= MIN_HEIGHT {
            level += 1;
        }
        let size = ((w >> level).max(1), (h >> level).max(1));
        let sample = DXGI_SAMPLE_DESC { Count: 1, Quality: 0 };

        let mut mips = None;
        self.device.CreateTexture2D(
            &D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: 0, // the full chain
                ArraySize: 1,
                Format: format,
                SampleDesc: sample,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
            },
            None,
            Some(&mut mips),
        )?;
        let mips = mips.ok_or(E_FAIL)?;

        let mut view = None;
        self.device.CreateShaderResourceView(&mips, None, Some(&mut view))?;
        let view = view.ok_or(E_FAIL)?;

        let mut staging = None;
        self.device.CreateTexture2D(
            &D3D11_TEXTURE2D_DESC {
                Width: size.0,
                Height: size.1,
                MipLevels: 1,
                ArraySize: 1,
                Format: format,
                SampleDesc: sample,
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            },
            None,
            Some(&mut staging),
        )?;
        let staging = staging.ok_or(E_FAIL)?;

        Ok(Targets { src: (w, h), mips, view, staging, level, size })
    }
}

/// Converts the BGRA surface to RGBA and undoes the display's rotation: the
/// duplicated surface is in the panel's native orientation, not the desktop's.
fn upright(surface: &[u8], sw: usize, sh: usize, pitch: usize, rotation: DXGI_MODE_ROTATION) -> Shot {
    let sideways = rotation == DXGI_MODE_ROTATION_ROTATE90 || rotation == DXGI_MODE_ROTATION_ROTATE270;
    let (width, height) = if sideways { (sh, sw) } else { (sw, sh) };
    let mut rgba = vec![255u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = match rotation {
                DXGI_MODE_ROTATION_ROTATE90 => (y, sh - 1 - x),
                DXGI_MODE_ROTATION_ROTATE180 => (sw - 1 - x, sh - 1 - y),
                DXGI_MODE_ROTATION_ROTATE270 => (sw - 1 - y, x),
                _ => (x, y),
            };
            let s = sy * pitch + sx * 4;
            let d = (y * width + x) * 4;
            rgba[d] = surface[s + 2];
            rgba[d + 1] = surface[s + 1];
            rgba[d + 2] = surface[s];
        }
    }
    Shot { rgba, width, height }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Dxgi::Common::DXGI_MODE_ROTATION_IDENTITY;

    /// A 3x2 surface whose pixels are numbered in the blue channel.
    fn numbered() -> Vec<u8> {
        (0u8..6).flat_map(|i| [i, 0, 0, 255]).collect()
    }

    fn ids(shot: &Shot) -> Vec<u8> {
        // Blue ends up third after the BGRA -> RGBA swap.
        shot.rgba.chunks(4).map(|p| p[2]).collect()
    }

    #[test]
    fn identity_only_swaps_channels() {
        let shot = upright(&numbered(), 3, 2, 12, DXGI_MODE_ROTATION_IDENTITY);
        assert_eq!((shot.width, shot.height), (3, 2));
        assert_eq!(ids(&shot), vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(&shot.rgba[..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn rotations_turn_the_surface_upright() {
        // Surface (panel orientation):   0 1 2
        //                                3 4 5
        let r180 = upright(&numbered(), 3, 2, 12, DXGI_MODE_ROTATION_ROTATE180);
        assert_eq!(ids(&r180), vec![5, 4, 3, 2, 1, 0]);

        let r90 = upright(&numbered(), 3, 2, 12, DXGI_MODE_ROTATION_ROTATE90);
        assert_eq!((r90.width, r90.height), (2, 3));
        assert_eq!(ids(&r90), vec![3, 0, 4, 1, 5, 2]);

        let r270 = upright(&numbered(), 3, 2, 12, DXGI_MODE_ROTATION_ROTATE270);
        assert_eq!(ids(&r270), vec![2, 5, 1, 4, 0, 3]);
    }

    #[test]
    fn row_padding_is_skipped() {
        // Same surface, but each row carries 4 bytes of padding.
        let mut padded = Vec::new();
        for row in numbered().chunks(12) {
            padded.extend_from_slice(row);
            padded.extend_from_slice(&[99; 4]);
        }
        let shot = upright(&padded, 3, 2, 16, DXGI_MODE_ROTATION_IDENTITY);
        assert_eq!(ids(&shot), vec![0, 1, 2, 3, 4, 5]);
    }
}
