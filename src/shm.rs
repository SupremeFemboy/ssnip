use memmap2::MmapMut;
use std::fs::File;
use std::os::fd::AsFd;
use wayland_client::{
    protocol::{wl_buffer, wl_shm, wl_shm_pool},
    Dispatch, QueueHandle,
};

pub struct ShmBuffer {
    pub file: File,
    pub mmap: MmapMut,
    pub pool: wl_shm_pool::WlShmPool,
    pub buffer: wl_buffer::WlBuffer,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: wl_shm::Format,
}

impl ShmBuffer {
    pub fn new<State>(
        shm: &wl_shm::WlShm,
        width: u32,
        height: u32,
        stride: u32,
        format: wl_shm::Format,
        qh: &QueueHandle<State>,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        State: 'static + Dispatch<wl_shm_pool::WlShmPool, ()> + Dispatch<wl_buffer::WlBuffer, ()>,
    {
        let size = (stride * height) as usize;
        let fd = rustix::fs::memfd_create(
            "ssnip-shm",
            rustix::fs::MemfdFlags::CLOEXEC | rustix::fs::MemfdFlags::ALLOW_SEALING,
        )?;
        let file = File::from(fd);
        file.set_len(size as u64)?;

        // Safety: We just created this anonymous memfd and sized it, no other process has access yet.
        let mmap = unsafe { MmapMut::map_mut(&file)? };

        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(0, width as i32, height as i32, stride as i32, format, qh, ());

        Ok(Self {
            file,
            mmap,
            pool,
            buffer,
            width,
            height,
            stride,
            format,
        })
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.mmap[..]
    }

    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        &mut self.mmap[..]
    }
}
