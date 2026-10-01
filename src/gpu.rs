//! Experimental native GPU compute. Scores use f32, unlike the CPU f64 accumulator.
use crate::Error;
#[path = "gpu_regex.rs"]
mod regex_compute;
use wgpu::util::DeviceExt;

/// Reusable hardware compute backend, shared with `Arc` across readers.
/// The opaque backend keeps GPU driver types out of async caller trait resolution.
pub struct GpuCompute {
    inner: Box<dyn ComputeBackend>,
    comparison_dispatches: std::sync::atomic::AtomicU64,
    text_dispatches: std::sync::atomic::AtomicU64,
    count_dispatches: std::sync::atomic::AtomicU64,
    sum_dispatches: std::sync::atomic::AtomicU64,
    graph_dispatches: std::sync::atomic::AtomicU64,
    weight_dispatches: std::sync::atomic::AtomicU64,
    rrf_dispatches: std::sync::atomic::AtomicU64,
    regex_dispatches: std::sync::atomic::AtomicU64,
    join_dispatches: std::sync::atomic::AtomicU64,
    sort_dispatches: std::sync::atomic::AtomicU64,
}
trait ComputeBackend: Send + Sync {
    fn device(&self) -> &wgpu::Device;
    fn queue(&self) -> &wgpu::Queue;
    fn validation_lock(&self) -> &std::sync::Mutex<()>;
    fn adapter_info(&self) -> &wgpu::AdapterInfo;
    fn cosine_batch(&self, query: &[f32], vectors: &[&[f32]]) -> Result<Vec<f32>, Error>;
}
impl ComputeBackend for WgpuCompute {
    fn device(&self) -> &wgpu::Device {
        &self.device
    }
    fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
    fn validation_lock(&self) -> &std::sync::Mutex<()> {
        &self.validation_lock
    }
    fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter
    }
    fn cosine_batch(&self, query: &[f32], vectors: &[&[f32]]) -> Result<Vec<f32>, Error> {
        WgpuCompute::cosine_batch(self, query, vectors)
    }
}
/// Backend selection and capabilities requested at device creation.
#[derive(Clone, Debug)]
pub struct GpuOptions {
    pub backends: wgpu::Backends,
    pub power_preference: wgpu::PowerPreference,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
}
impl Default for GpuOptions {
    fn default() -> Self {
        Self {
            backends: wgpu::Backends::all(),
            power_preference: wgpu::PowerPreference::HighPerformance,
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
        }
    }
}
impl GpuCompute {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            inner: Box::new(WgpuCompute::new(GpuOptions::default())?),
            comparison_dispatches: Default::default(),
            text_dispatches: Default::default(),
            count_dispatches: Default::default(),
            sum_dispatches: Default::default(),
            graph_dispatches: Default::default(),
            weight_dispatches: Default::default(),
            rrf_dispatches: Default::default(),
            regex_dispatches: Default::default(),
            join_dispatches: Default::default(),
            sort_dispatches: Default::default(),
        })
    }
    /// Request optional shader features and device limits explicitly.
    pub fn with_options(options: GpuOptions) -> Result<Self, Error> {
        Ok(Self {
            inner: Box::new(WgpuCompute::new(options)?),
            comparison_dispatches: Default::default(),
            text_dispatches: Default::default(),
            count_dispatches: Default::default(),
            sum_dispatches: Default::default(),
            graph_dispatches: Default::default(),
            weight_dispatches: Default::default(),
            rrf_dispatches: Default::default(),
            regex_dispatches: Default::default(),
            join_dispatches: Default::default(),
            sort_dispatches: Default::default(),
        })
    }
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        self.inner.adapter_info()
    }
    /// f32 cosine scores, in input order; variable lengths use the shared prefix.
    pub fn cosine_batch(&self, query: &[f32], vectors: &[&[f32]]) -> Result<Vec<f32>, Error> {
        self.inner.cosine_batch(query, vectors)
    }
}

/// Reusable device and cosine compute pipeline. Share with `Arc` across readers.
struct WgpuCompute {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    adapter: wgpu::AdapterInfo,
    validation_lock: std::sync::Mutex<()>,
}

impl WgpuCompute {
    /// Select a hardware adapter; software adapters are rejected.
    fn new(options: GpuOptions) -> Result<Self, Error> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: options.backends,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: options.power_preference,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok_or_else(|| Error::runtime("no GPU adapter available"))?;
        let info = adapter.get_info();
        if matches!(info.device_type, wgpu::DeviceType::Cpu) {
            return Err(Error::runtime("software GPU adapter rejected"));
        }
        if !adapter.features().contains(options.features)
            || !options.limits.check_limits(&adapter.limits())
        {
            return Err(Error::runtime("requested GPU capabilities are unsupported"));
        }
        if options.limits.max_storage_buffers_per_shader_stage < 4
            || options.limits.max_compute_invocations_per_workgroup < 64
            || options.limits.max_compute_workgroup_size_x < 64
        {
            return Err(Error::runtime(
                "GPU limits cannot run the built-in cosine shader",
            ));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_features: options.features,
                required_limits: options.limits,
                ..Default::default()
            },
            None,
        ))
        .map_err(|e| Error::runtime(format!("GPU device: {e}")))?;
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lin cosine"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/cosine.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("lin cosine"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(error) = pollster::block_on(device.pop_error_scope()) {
            return Err(Error::runtime(format!("GPU cosine pipeline: {error}")));
        }
        Ok(Self {
            device,
            queue,
            pipeline,
            adapter: info,
            validation_lock: std::sync::Mutex::new(()),
        })
    }

    /// Score variable-length vectors using the shared prefix, as on CPU.
    /// Batches are bounded by device buffer and dispatch limits.
    pub fn cosine_batch(&self, query: &[f32], vectors: &[&[f32]]) -> Result<Vec<f32>, Error> {
        if query.is_empty() || vectors.is_empty() {
            return Ok(vec![0.0; vectors.len()]);
        }
        if !query
            .iter()
            .chain(vectors.iter().flat_map(|v| v.iter()))
            .all(|x| x.is_finite())
        {
            return Err(Error::runtime("GPU cosine requires finite vectors"));
        }
        let limits = self.device.limits();
        let row_bytes = query
            .len()
            .checked_mul(4)
            .ok_or_else(|| Error::runtime("GPU dimension overflow"))?;
        let max_bytes = (limits.max_storage_buffer_binding_size as u64).min(limits.max_buffer_size);
        if row_bytes as u64 > max_bytes {
            return Err(Error::runtime("GPU vector exceeds buffer limit"));
        }
        let chunk_rows = (max_bytes as usize / row_bytes)
            .min(max_bytes as usize / 4)
            .min(limits.max_compute_workgroups_per_dimension as usize);
        let mut out = Vec::with_capacity(vectors.len());
        for chunk in vectors.chunks(chunk_rows) {
            out.extend(self.score_chunk(query, chunk)?);
        }
        Ok(out)
    }

    fn score_chunk(&self, query: &[f32], vectors: &[&[f32]]) -> Result<Vec<f32>, Error> {
        let mut packed = vec![0f32; query.len() * vectors.len()];
        let lengths: Vec<u32> = vectors
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let n = query.len().min(v.len());
                packed[i * query.len()..i * query.len() + n].copy_from_slice(&v[..n]);
                n as u32
            })
            .collect();
        let storage = |label, bytes: &[u8]| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytes,
                    usage: wgpu::BufferUsages::STORAGE,
                })
        };
        let q = storage("query", bytemuck::cast_slice(query));
        let v = storage("vectors", bytemuck::cast_slice(&packed));
        let n = storage("lengths", bytemuck::cast_slice(&lengths));
        let size = (vectors.len() * 4) as u64;
        let scores = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scores"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let buffers = [&q, &v, &n, &scores];
        let entries: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cosine"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(vectors.len() as u32, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&scores, 0, &readback, 0, size);
        let submission = self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device
            .poll(wgpu::Maintain::WaitForSubmissionIndex(submission));
        rx.recv()
            .map_err(|e| Error::runtime(format!("GPU readback: {e}")))?
            .map_err(|e| Error::runtime(format!("GPU mapping: {e}")))?;
        let mapped = readback.slice(..).get_mapped_range();
        let result = bytemuck::cast_slice::<u8, f32>(&mapped).to_vec();
        drop(mapped);
        readback.unmap();
        Ok(result)
    }
}

/// Native resource API for storage/uniform buffers, textures, samplers and layouts.
pub use wgpu as native;

/// A compiled compute entry point. Resources are retained by wgpu handles.
pub struct ComputeKernel {
    pipeline: wgpu::ComputePipeline,
}
impl ComputeKernel {
    /// Reflected bind group layout, usable with `GpuCompute::device()`.
    pub fn bind_group_layout(&self, index: u32) -> wgpu::BindGroupLayout {
        self.pipeline.get_bind_group_layout(index)
    }
}
impl GpuCompute {
    /// Obtain a reflected layout with validation errors returned as `Error`.
    pub fn bind_group_layout(
        &self,
        kernel: &ComputeKernel,
        index: u32,
    ) -> Result<wgpu::BindGroupLayout, Error> {
        if index >= self.device().limits().max_bind_groups {
            return Err(Error::runtime("GPU bind group index exceeds device limits"));
        }
        self.checked(|_, _| kernel.bind_group_layout(index))
    }

    /// Access native resource creation, limits and enabled features.
    /// Prefer `checked` when constructing user-provided resources.
    pub fn device(&self) -> &wgpu::Device {
        self.inner.device()
    }
    pub fn queue(&self) -> &wgpu::Queue {
        self.inner.queue()
    }

    /// Serialize validation scopes and convert validation/allocation errors to Lin errors.
    /// Calls inside the closure must not recursively call another checked operation.
    pub fn checked<T>(
        &self,
        operation: impl FnOnce(&wgpu::Device, &wgpu::Queue) -> T,
    ) -> Result<T, Error> {
        let _guard = self
            .inner
            .validation_lock()
            .lock()
            .map_err(|_| Error::runtime("GPU validation lock poisoned"))?;
        let device = self.device();
        device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let value = operation(device, self.queue());
        let validation = pollster::block_on(device.pop_error_scope());
        let allocation = pollster::block_on(device.pop_error_scope());
        if let Some(error) = validation.or(allocation) {
            return Err(Error::runtime(format!("GPU: {error}")));
        }
        Ok(value)
    }

    /// Compile arbitrary WGSL compute source and an entry point. Invalid source returns an error.
    pub fn compile_wgsl(&self, source: &str, entry: &str) -> Result<ComputeKernel, Error> {
        self.checked(|device, _| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("lin user compute"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            ComputeKernel {
                pipeline: device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: None,
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                }),
            }
        })
    }

    /// Submit a compute dispatch. Buffers and textures remain on GPU for subsequent dispatches.
    /// Workgroups are three-dimensional; binding/layout/device errors return `Error`.
    pub fn dispatch(
        &self,
        kernel: &ComputeKernel,
        groups: &[&wgpu::BindGroup],
        workgroups: [u32; 3],
    ) -> Result<wgpu::SubmissionIndex, Error> {
        let limit = self.device().limits().max_compute_workgroups_per_dimension;
        if workgroups.iter().any(|&n| n > limit) {
            return Err(Error::runtime("GPU dispatch exceeds device limits"));
        }
        self.checked(|device, queue| {
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&kernel.pipeline);
                for (i, group) in groups.iter().enumerate() {
                    pass.set_bind_group(i as u32, *group, &[]);
                }
                pass.dispatch_workgroups(workgroups[0], workgroups[1], workgroups[2]);
            }
            queue.submit([encoder.finish()])
        })
    }

    /// Upload reusable storage data. The buffer supports GPU writes, updates and readback.
    pub fn storage_buffer(&self, bytes: &[u8]) -> Result<wgpu::Buffer, Error> {
        if bytes.is_empty() || bytes.len() % 4 != 0 {
            return Err(Error::runtime(
                "GPU storage size must be a nonzero multiple of 4",
            ));
        }
        self.checked(|device, _| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("lin storage"),
                contents: bytes,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            })
        })
    }

    /// Read a COPY_SRC buffer after queued computations, without changing its mapping state.
    pub fn read_buffer(&self, source: &wgpu::Buffer) -> Result<Vec<u8>, Error> {
        let size = source.size();
        if size == 0 || size % 4 != 0 {
            return Err(Error::runtime(
                "GPU readback size must be a nonzero multiple of 4",
            ));
        }
        let (buffer, submission) = self.checked(|device, queue| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("lin readback"),
                size,
                mapped_at_creation: false,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_buffer_to_buffer(source, 0, &buffer, 0, size);
            let submission = queue.submit([encoder.finish()]);
            (buffer, submission)
        })?;
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device()
            .poll(wgpu::Maintain::WaitForSubmissionIndex(submission));
        rx.recv()
            .map_err(|e| Error::runtime(format!("GPU readback: {e}")))?
            .map_err(|e| Error::runtime(format!("GPU mapping: {e}")))?;
        let bytes = buffer.slice(..).get_mapped_range().to_vec();
        buffer.unmap();
        Ok(bytes)
    }
}

/// Vector corpus uploaded once. Chunk buffers stay resident between queries.
/// Use it with the `GpuCompute` device that created it.
pub struct GpuVectors {
    dimension: usize,
    rows: usize,
    kernel: ComputeKernel,
    chunks: Vec<ResidentChunk>,
}
struct ResidentChunk {
    vectors: wgpu::Buffer,
    lengths: wgpu::Buffer,
    scores: wgpu::Buffer,
    rows: usize,
}
impl GpuVectors {
    pub fn len(&self) -> usize {
        self.rows
    }
    pub fn is_empty(&self) -> bool {
        self.rows == 0
    }
    pub fn dimension(&self) -> usize {
        self.dimension
    }
}
impl GpuCompute {
    /// Upload a corpus once for repeated cosine queries, without repeated corpus transfers.
    pub fn upload_vectors(
        &self,
        dimension: usize,
        vectors: &[&[f32]],
    ) -> Result<GpuVectors, Error> {
        if dimension == 0 {
            return Err(Error::runtime("GPU corpus dimension must be nonzero"));
        }
        if !vectors.iter().flat_map(|v| v.iter()).all(|x| x.is_finite()) {
            return Err(Error::runtime("GPU cosine requires finite vectors"));
        }
        let limits = self.device().limits();
        let row_bytes = dimension
            .checked_mul(4)
            .ok_or_else(|| Error::runtime("GPU dimension overflow"))?;
        let max_bytes = (limits.max_storage_buffer_binding_size as u64).min(limits.max_buffer_size);
        if row_bytes as u64 > max_bytes {
            return Err(Error::runtime("GPU vector exceeds buffer limit"));
        }
        let chunk_rows = (max_bytes as usize / row_bytes)
            .min(limits.max_compute_workgroups_per_dimension as usize);
        let kernel = self.compile_wgsl(include_str!("shaders/cosine.wgsl"), "main")?;
        let mut chunks = Vec::new();
        for chunk in vectors.chunks(chunk_rows) {
            let mut packed = vec![0f32; dimension * chunk.len()];
            let lengths: Vec<u32> = chunk
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let n = dimension.min(v.len());
                    packed[i * dimension..i * dimension + n].copy_from_slice(&v[..n]);
                    n as u32
                })
                .collect();
            chunks.push(ResidentChunk {
                vectors: self.storage_buffer(bytemuck::cast_slice(&packed))?,
                lengths: self.storage_buffer(bytemuck::cast_slice(&lengths))?,
                scores: self.storage_buffer(&vec![0u8; chunk.len() * 4])?,
                rows: chunk.len(),
            });
        }
        Ok(GpuVectors {
            dimension,
            rows: vectors.len(),
            kernel,
            chunks,
        })
    }

    /// Score a resident corpus. Only the query and result cross the GPU boundary.
    /// Shared result buffers serialize concurrent calls on the same corpus.
    pub fn cosine_resident(
        &self,
        query: &[f32],
        corpus: &mut GpuVectors,
    ) -> Result<Vec<f32>, Error> {
        if query.len() != corpus.dimension {
            return Err(Error::runtime("GPU query/corpus dimension mismatch"));
        }
        if !query.iter().all(|x| x.is_finite()) {
            return Err(Error::runtime("GPU cosine requires finite vectors"));
        }
        if corpus.is_empty() {
            return Ok(Vec::new());
        }
        let query = self.storage_buffer(bytemuck::cast_slice(query))?;
        let mut result = Vec::with_capacity(corpus.rows);
        for chunk in &corpus.chunks {
            let buffers = [&query, &chunk.vectors, &chunk.lengths, &chunk.scores];
            let group = self.checked(|device, _| {
                let entries: Vec<_> = buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect();
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("resident cosine"),
                    layout: &corpus.kernel.bind_group_layout(0),
                    entries: &entries,
                })
            })?;
            self.dispatch(&corpus.kernel, &[&group], [chunk.rows as u32, 1, 1])?;
            let bytes = self.read_buffer(&chunk.scores)?;
            result.extend_from_slice(bytemuck::cast_slice::<u8, f32>(&bytes));
        }
        Ok(result)
    }
}

/// Counters for the executor's resident vector corpus cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuCacheStats {
    pub uploads: u64,
    pub hits: u64,
    pub resident_rows: usize,
}
#[derive(Default)]
pub(crate) struct SearchCache {
    entry: Option<SearchEntry>,
    stats: GpuCacheStats,
}
struct SearchEntry {
    // Retaining Arc owners prevents address reuse and in-place unique mutation.
    vectors: Vec<std::sync::Arc<[f32]>>,
    corpus: Box<dyn CachedCorpus>,
}
trait CachedCorpus: Send + Sync {
    fn dimension(&self) -> usize;
    fn score(&mut self, gpu: &GpuCompute, query: &[f32]) -> Result<Vec<f32>, Error>;
}
impl CachedCorpus for GpuVectors {
    fn dimension(&self) -> usize {
        self.dimension()
    }
    fn score(&mut self, gpu: &GpuCompute, query: &[f32]) -> Result<Vec<f32>, Error> {
        gpu.cosine_resident(query, self)
    }
}
impl SearchCache {
    pub(crate) fn stats(&self) -> GpuCacheStats {
        self.stats
    }
    pub(crate) fn score(
        &mut self,
        gpu: &GpuCompute,
        query: &[f32],
        vectors: &[std::sync::Arc<[f32]>],
    ) -> Result<Vec<f32>, Error> {
        if query.is_empty() || vectors.is_empty() {
            return Ok(vec![0.0; vectors.len()]);
        }
        let matches = self.entry.as_ref().is_some_and(|entry| {
            entry.corpus.dimension() == query.len()
                && entry.vectors.len() == vectors.len()
                && entry
                    .vectors
                    .iter()
                    .zip(vectors)
                    .all(|(a, b)| std::sync::Arc::ptr_eq(a, b))
        });
        if matches {
            self.stats.hits += 1;
        } else {
            let refs: Vec<&[f32]> = vectors.iter().map(|v| v.as_ref()).collect();
            let corpus = gpu.upload_vectors(query.len(), &refs)?;
            self.entry = Some(SearchEntry {
                vectors: vectors.to_vec(),
                corpus: Box::new(corpus),
            });
            self.stats.uploads += 1;
            self.stats.resident_rows = vectors.len();
        }
        self.entry
            .as_mut()
            .expect("uploaded corpus")
            .corpus
            .score(gpu, query)
    }
}

impl GpuCompute {
    /// Number of successfully submitted numeric comparison dispatches.
    pub fn comparison_dispatches(&self) -> u64 {
        self.comparison_dispatches
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Compare ordered 64-bit key pairs. Each record is [left_lo, left_hi,
    /// right_lo, right_hi, comparable]. Incomparable pairs are unequal.
    pub(crate) fn comparison_mask(
        &self,
        records: &[[u32; 5]],
        op: crate::CmpOp,
    ) -> Result<Vec<bool>, Error> {
        if records.is_empty() {
            return Ok(Vec::new());
        }
        let kernel = self.compile_wgsl(include_str!("shaders/compare.wgsl"), "main")?;
        let opcode: u32 = match op {
            crate::CmpOp::Eq => 0,
            crate::CmpOp::Ne => 1,
            crate::CmpOp::Gt => 2,
            crate::CmpOp::Lt => 3,
            crate::CmpOp::Ge => 4,
            crate::CmpOp::Le => 5,
        };
        let operation = self.storage_buffer(bytemuck::bytes_of(&opcode))?;
        let limits = self.device().limits();
        let count = (limits.max_storage_buffer_binding_size as usize / 20)
            .min(limits.max_buffer_size as usize / 20)
            .min(limits.max_compute_workgroups_per_dimension as usize * 64);
        if count == 0 {
            return Err(Error::runtime("GPU limits cannot hold a comparison record"));
        }
        let mut result = Vec::with_capacity(records.len());
        for chunk in records.chunks(count) {
            let input = self.storage_buffer(bytemuck::cast_slice(chunk))?;
            let output = self.storage_buffer(&vec![0u8; chunk.len() * 4])?;
            let group = self.checked(|device, _| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("numeric comparison"),
                    layout: &kernel.bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: input.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: operation.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: output.as_entire_binding(),
                        },
                    ],
                })
            })?;
            self.dispatch(
                &kernel,
                &[&group],
                [(chunk.len() as u32).div_ceil(64), 1, 1],
            )?;
            self.comparison_dispatches
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let bytes = self.read_buffer(&output)?;
            result.extend(
                bytes
                    .chunks_exact(4)
                    .map(|v| u32::from_ne_bytes(v.try_into().unwrap()) != 0),
            );
        }
        Ok(result)
    }
}
impl GpuCompute {
    pub(crate) fn combine_masks(
        &self,
        a: &[bool],
        b: &[bool],
        or: bool,
    ) -> Result<Vec<bool>, Error> {
        if a.len() != b.len() {
            return Err(Error::runtime("GPU mask lengths differ"));
        }
        if a.is_empty() {
            return Ok(Vec::new());
        }
        let kernel = self.compile_wgsl(include_str!("shaders/boolean.wgsl"), "main")?;
        let operation = self.storage_buffer(bytemuck::bytes_of(&(or as u32)))?;
        let limits = self.device().limits();
        let count = (limits.max_storage_buffer_binding_size as usize / 4)
            .min(limits.max_buffer_size as usize / 4)
            .min(limits.max_compute_workgroups_per_dimension as usize * 64);
        if count == 0 {
            return Err(Error::runtime("GPU limits cannot hold a boolean mask"));
        }
        let mut result = Vec::with_capacity(a.len());
        for (a, b) in a.chunks(count).zip(b.chunks(count)) {
            let a: Vec<u32> = a.iter().map(|v| *v as u32).collect();
            let b: Vec<u32> = b.iter().map(|v| *v as u32).collect();
            let a = self.storage_buffer(bytemuck::cast_slice(&a))?;
            let b = self.storage_buffer(bytemuck::cast_slice(&b))?;
            let output = self.storage_buffer(&vec![0u8; a.size() as usize])?;
            let buffers = [&a, &b, &operation, &output];
            let group = self.checked(|device, _| {
                let entries: Vec<_> = buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect();
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("boolean masks"),
                    layout: &kernel.bind_group_layout(0),
                    entries: &entries,
                })
            })?;
            self.dispatch(
                &kernel,
                &[&group],
                [((output.size() / 4) as u32).div_ceil(64), 1, 1],
            )?;
            let bytes = self.read_buffer(&output)?;
            result.extend(
                bytes
                    .chunks_exact(4)
                    .map(|v| u32::from_ne_bytes(v.try_into().unwrap()) != 0),
            );
        }
        Ok(result)
    }
}

fn encode_text(text: &str, fold: bool, words: bool) -> Vec<u32> {
    if !words {
        return text.bytes().map(u32::from).collect();
    }
    let chars: Vec<char> = text.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut encoded = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        let token = word(c);
        let start = token && (i == 0 || !word(chars[i - 1]));
        let end = token && (i + 1 == chars.len() || !word(chars[i + 1]));
        let string = if fold {
            c.to_lowercase().collect::<String>()
        } else {
            c.to_string()
        };
        let from = encoded.len();
        encoded.extend(
            string
                .bytes()
                .map(|b| b as u32 | if token { 1024 } else { 0 }),
        );
        if start {
            encoded[from] |= 256;
        }
        if end {
            *encoded.last_mut().expect("encoded char") |= 512;
        }
    }
    encoded
}
impl GpuCompute {
    /// Successfully submitted text matching dispatches.
    pub fn text_dispatches(&self) -> u64 {
        self.text_dispatches
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Byte substring/equality or whole-token matching. Unicode token metadata and
    /// case folding use Rust's classification, matching Lin's CPU semantics.
    pub(crate) fn text_mask(
        &self,
        texts: &[Option<&str>],
        needle: &str,
        op: u32,
        ci: bool,
    ) -> Result<Vec<bool>, Error> {
        self.text_mask_inner(texts, needle, op, ci, None)
    }
    pub(crate) fn text_accumulate(
        &self,
        texts: &[Option<&str>],
        needle: &str,
        op: u32,
        accumulator: &mut GpuWeightAccumulator<'_>,
        weight: u32,
    ) -> Result<(), Error> {
        if texts.len() != accumulator.rows || !std::ptr::eq(self, accumulator.gpu) {
            return Err(Error::runtime("GPU text scoring accumulator mismatch"));
        }
        self.text_mask_inner(texts, needle, op, false, Some((accumulator, weight)))?;
        Ok(())
    }
    fn text_mask_inner(
        &self,
        texts: &[Option<&str>],
        needle: &str,
        op: u32,
        ci: bool,
        mut accumulation: Option<(&mut GpuWeightAccumulator<'_>, u32)>,
    ) -> Result<Vec<bool>, Error> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let needle = if op == 3 && ci {
            needle
                .chars()
                .flat_map(char::to_lowercase)
                .collect::<String>()
        } else {
            needle.to_owned()
        };
        let limits = self.device().limits();
        let max_words = (limits.max_storage_buffer_binding_size as usize)
            .min(limits.max_buffer_size as usize)
            / 4;
        if needle.len().checked_add(2).is_none_or(|n| n > max_words) {
            return Err(Error::runtime("GPU text needle exceeds buffer limits"));
        }
        let kernel = self.compile_wgsl(include_str!("shaders/text.wgsl"), "main")?;
        let mut params = vec![op, needle.len() as u32];
        params.extend(needle.bytes().map(u32::from));
        let params = self.storage_buffer(bytemuck::cast_slice(&params))?;
        let row_limit =
            (max_words / 3).min(limits.max_compute_workgroups_per_dimension as usize * 64);
        if row_limit == 0 {
            return Err(Error::runtime("GPU limits cannot hold a text row"));
        }
        let mut result = if accumulation.is_some() {
            Vec::new()
        } else {
            Vec::with_capacity(texts.len())
        };
        let mut position = 0;
        while position < texts.len() {
            let batch_start = position;
            let mut data = Vec::new();
            let mut records = Vec::new();
            while position < texts.len() && records.len() / 3 < row_limit {
                let text = texts[position];
                let encoded = text
                    .map(|t| encode_text(t, op == 3 && ci, op == 3))
                    .unwrap_or_default();
                if encoded.len() > max_words {
                    return Err(Error::runtime("GPU text row exceeds buffer limits"));
                }
                if !records.is_empty() && data.len() + encoded.len() > max_words {
                    break;
                }
                records.extend([
                    data.len() as u32,
                    encoded.len() as u32,
                    text.is_some() as u32,
                ]);
                data.extend(encoded);
                position += 1;
            }
            if data.is_empty() {
                data.push(0);
            }
            let data = self.storage_buffer(bytemuck::cast_slice(&data))?;
            let records = self.storage_buffer(bytemuck::cast_slice(&records))?;
            let count = records.size() / 12;
            let output = self.storage_buffer(&vec![0u8; count as usize * 4])?;
            let buffers = [&data, &records, &params, &output];
            let group = self.checked(|device, _| {
                let entries: Vec<_> = buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect();
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("text matching"),
                    layout: &kernel.bind_group_layout(0),
                    entries: &entries,
                })
            })?;
            self.dispatch(&kernel, &[&group], [(count as u32).div_ceil(64), 1, 1])?;
            self.text_dispatches
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if let Some((accumulator, weight)) = accumulation.as_mut() {
                accumulator.add_buffer(&output, batch_start, count as usize, *weight)?;
            } else {
                let bytes = self.read_buffer(&output)?;
                result.extend(
                    bytes
                        .chunks_exact(4)
                        .map(|v| u32::from_ne_bytes(v.try_into().unwrap()) != 0),
                );
            }
        }
        Ok(result)
    }
}
impl GpuCompute {
    /// Successfully submitted group count dispatches.
    pub fn count_dispatches(&self) -> u64 {
        self.count_dispatches
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    /// GPU group counts; chunk results accumulate into i64 without u32 wraparound.
    pub(crate) fn group_counts(&self, ids: &[u32], groups: usize) -> Result<Vec<i64>, Error> {
        if groups == 0 {
            return if ids.is_empty() {
                Ok(Vec::new())
            } else {
                Err(Error::runtime("GPU count requires groups"))
            };
        }
        if ids.iter().any(|&id| id as usize >= groups) {
            return Err(Error::runtime("GPU count group ID out of bounds"));
        }
        let limits = self.device().limits();
        let max_words = (limits.max_storage_buffer_binding_size as usize)
            .min(limits.max_buffer_size as usize)
            / 4;
        if groups > max_words {
            return Err(Error::runtime("GPU count groups exceed buffer limit"));
        }
        let chunk_rows = max_words.min(limits.max_compute_workgroups_per_dimension as usize * 64);
        if chunk_rows == 0 {
            return Err(Error::runtime("GPU limits cannot hold count input"));
        }
        let mut result = vec![0i64; groups];
        if ids.is_empty() {
            return Ok(result);
        }
        let kernel = self.compile_wgsl(include_str!("shaders/count.wgsl"), "main")?;
        for chunk in ids.chunks(chunk_rows) {
            let input = self.storage_buffer(bytemuck::cast_slice(chunk))?;
            let output = self.storage_buffer(&vec![0u8; groups * 4])?;
            let group = self.checked(|device, _| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("group count"),
                    layout: &kernel.bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: input.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: output.as_entire_binding(),
                        },
                    ],
                })
            })?;
            self.dispatch(
                &kernel,
                &[&group],
                [(chunk.len() as u32).div_ceil(64), 1, 1],
            )?;
            self.count_dispatches
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let bytes = self.read_buffer(&output)?;
            for (total, bytes) in result.iter_mut().zip(bytes.chunks_exact(4)) {
                *total += u32::from_ne_bytes(bytes.try_into().unwrap()) as i64;
            }
        }
        Ok(result)
    }
}
impl GpuCompute {
    /// Successfully submitted join build/probe/matching dispatches.
    pub fn join_dispatches(&self) -> u64 {
        self.join_dispatches
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Match dictionary-encoded join keys (0 = missing). Matrix tiles are bounded
    /// by buffer/dispatch limits. Hits preserve left order, then right order.
    pub(crate) fn join_matches(
        &self,
        left: &[u32],
        right: &[u32],
    ) -> Result<Vec<Vec<usize>>, Error> {
        let mut hits = vec![Vec::new(); left.len()];
        if left.is_empty() || right.is_empty() {
            return Ok(hits);
        }
        let limits = self.device().limits();
        let max_words = (limits.max_storage_buffer_binding_size as usize)
            .min(limits.max_buffer_size as usize)
            / 4;
        let max_dispatch = limits.max_compute_workgroups_per_dimension as usize * 64;
        let left_rows = 64usize.min(max_words).min(max_dispatch);
        if left_rows == 0 {
            return Err(Error::runtime("GPU limits cannot hold join input"));
        }
        let kernel = self.compile_wgsl(include_str!("shaders/join.wgsl"), "main")?;
        for (left_tile, left_keys) in left.chunks(left_rows).enumerate() {
            let left_buffer = self.storage_buffer(bytemuck::cast_slice(left_keys))?;
            let right_rows = 4096usize
                .min(max_words / left_keys.len())
                .min(max_dispatch / left_keys.len());
            for (right_tile, right_keys) in right.chunks(right_rows).enumerate() {
                let right_buffer = self.storage_buffer(bytemuck::cast_slice(right_keys))?;
                let output =
                    self.storage_buffer(&vec![0u8; left_keys.len() * right_keys.len() * 4])?;
                let buffers = [&left_buffer, &right_buffer, &output];
                let group = self.checked(|device, _| {
                    let entries: Vec<_> = buffers
                        .iter()
                        .enumerate()
                        .map(|(i, b)| wgpu::BindGroupEntry {
                            binding: i as u32,
                            resource: b.as_entire_binding(),
                        })
                        .collect();
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("join tile"),
                        layout: &kernel.bind_group_layout(0),
                        entries: &entries,
                    })
                })?;
                self.dispatch(
                    &kernel,
                    &[&group],
                    [((output.size() / 4) as u32).div_ceil(64), 1, 1],
                )?;
                self.join_dispatches
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let bytes = self.read_buffer(&output)?;
                for (pair, bytes) in bytes.chunks_exact(4).enumerate() {
                    if u32::from_ne_bytes(bytes.try_into().unwrap()) != 0 {
                        let l = left_tile * left_rows + pair / right_keys.len();
                        let r = right_tile * right_rows + pair % right_keys.len();
                        hits[l].push(r);
                    }
                }
            }
        }
        Ok(hits)
    }
}
impl GpuCompute {
    /// ID joins use an atomic dense lookup table. Last duplicate wins, just as
    /// on CPU. Large dictionaries use the bounded matrix kernel instead.
    pub(crate) fn join_id_matches(
        &self,
        left: &[u32],
        right: &[u32],
    ) -> Result<Vec<Option<usize>>, Error> {
        if left.is_empty() || right.is_empty() {
            return Ok(vec![None; left.len()]);
        }
        if right.len() >= u32::MAX as usize {
            return Err(Error::runtime("GPU ID join right rows exceed index range"));
        }
        let limits = self.device().limits();
        let max_words = (limits.max_storage_buffer_binding_size as usize)
            .min(limits.max_buffer_size as usize)
            / 4;
        let groups = left.iter().chain(right).copied().max().unwrap_or(0) as usize + 1;
        if groups > max_words {
            return Ok(self
                .join_matches(left, right)?
                .into_iter()
                .map(|hits| hits.last().copied())
                .collect());
        }
        let chunk_rows = max_words.min(limits.max_compute_workgroups_per_dimension as usize * 64);
        if chunk_rows == 0 {
            return Err(Error::runtime("GPU limits cannot hold ID join input"));
        }
        let source = include_str!("shaders/join_id.wgsl");
        let build = self.compile_wgsl(source, "build")?;
        let probe = self.compile_wgsl(source, "probe")?;
        let lookup = self.storage_buffer(&vec![0u8; groups * 4])?;
        for (tile, keys) in right.chunks(chunk_rows).enumerate() {
            let input = self.storage_buffer(bytemuck::cast_slice(keys))?;
            let bias = (tile * chunk_rows) as u32;
            let params = self.storage_buffer(bytemuck::bytes_of(&bias))?;
            let group = self.checked(|device, _| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("ID join build"),
                    layout: &build.bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: input.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: lookup.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: params.as_entire_binding(),
                        },
                    ],
                })
            })?;
            self.dispatch(&build, &[&group], [(keys.len() as u32).div_ceil(64), 1, 1])?;
            self.join_dispatches
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let mut result = Vec::with_capacity(left.len());
        for keys in left.chunks(chunk_rows) {
            let input = self.storage_buffer(bytemuck::cast_slice(keys))?;
            let output = self.storage_buffer(&vec![0u8; keys.len() * 4])?;
            let group = self.checked(|device, _| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("ID join probe"),
                    layout: &probe.bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: input.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: lookup.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: output.as_entire_binding(),
                        },
                    ],
                })
            })?;
            self.dispatch(&probe, &[&group], [(keys.len() as u32).div_ceil(64), 1, 1])?;
            self.join_dispatches
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let bytes = self.read_buffer(&output)?;
            result.extend(bytes.chunks_exact(4).map(|bytes| {
                let value = u32::from_ne_bytes(bytes.try_into().unwrap());
                if value == 0 {
                    None
                } else {
                    Some(value as usize - 1)
                }
            }));
        }
        Ok(result)
    }
}

impl GpuCompute {
    /// Successfully submitted stable merge-sort passes.
    pub fn sort_dispatches(&self) -> u64 {
        self.sort_dispatches
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Stable GPU merge sort over scalar cell keys; returns original row indices.
    pub(crate) fn sort_indices(
        &self,
        cells: &[&crate::Cell],
        desc: bool,
    ) -> Result<Vec<usize>, Error> {
        use crate::Cell;
        if cells.len() < 2 {
            return Ok((0..cells.len()).collect());
        }
        let limits = self.device().limits();
        let max_words = (limits.max_storage_buffer_binding_size as usize)
            .min(limits.max_buffer_size as usize)
            / 4;
        if cells.len() > max_words / 5 || cells.len() > u32::MAX as usize / 2 {
            return Err(Error::runtime("GPU sort keys exceed buffer limits"));
        }
        if cells.len().div_ceil(64) > limits.max_compute_workgroups_per_dimension as usize {
            return Err(Error::runtime("GPU sort exceeds dispatch limits"));
        }
        let mut keys = Vec::with_capacity(cells.len() * 5);
        let mut text = Vec::new();
        for cell in cells {
            let (kind, value, string) = match cell {
                Cell::Null => (0u32, 0u64, None),
                Cell::Text(s) => (1, 0, Some(s.to_string())),
                Cell::Bool(b) => (2, *b as u64, None),
                Cell::Vec(_) => (3, 0, Some(cell.compact())),
                _ => {
                    let v = cell.as_f64().expect("numeric cell");
                    if v.is_nan() {
                        (5, 0, None)
                    } else {
                        let bits = if v == 0.0 { 0 } else { v.to_bits() };
                        let key = if bits >> 63 != 0 {
                            !bits
                        } else {
                            bits ^ (1u64 << 63)
                        };
                        (4, key, None)
                    }
                }
            };
            let offset = text.len();
            let length = string.as_ref().map_or(0, String::len);
            if length > max_words.saturating_sub(offset) {
                return Err(Error::runtime("GPU sort text exceeds buffer limits"));
            }
            if let Some(string) = string {
                text.extend(string.bytes().map(u32::from));
            }
            keys.extend([
                kind,
                (value >> 32) as u32,
                value as u32,
                offset as u32,
                length as u32,
            ]);
        }
        if text.is_empty() {
            text.push(0);
        }
        self.sort_encoded(&keys, &text, cells.len(), desc)
    }
    /// Exact integer ranking; unlike scalar mixed-number sorting, no f64 conversion.
    pub(crate) fn rank_i64(&self, scores: &[i64]) -> Result<Vec<usize>, Error> {
        let keys: Vec<u32> = scores
            .iter()
            .flat_map(|&score| {
                let key = (score as u64) ^ (1u64 << 63);
                [4, (key >> 32) as u32, key as u32, 0, 0]
            })
            .collect();
        self.sort_encoded(&keys, &[0], scores.len(), true)
    }
    fn sort_encoded(
        &self,
        keys: &[u32],
        text: &[u32],
        rows: usize,
        desc: bool,
    ) -> Result<Vec<usize>, Error> {
        if rows < 2 {
            return Ok((0..rows).collect());
        }
        let limits = self.device().limits();
        let max_words = (limits.max_storage_buffer_binding_size as usize)
            .min(limits.max_buffer_size as usize)
            / 4;
        if rows > max_words / 5 || rows > u32::MAX as usize / 2 {
            return Err(Error::runtime("GPU sort keys exceed buffer limits"));
        }
        if rows.div_ceil(64) > limits.max_compute_workgroups_per_dimension as usize {
            return Err(Error::runtime("GPU sort exceeds dispatch limits"));
        }
        let keys = self.storage_buffer(bytemuck::cast_slice(keys))?;
        let text = self.storage_buffer(bytemuck::cast_slice(text))?;
        let indices: Vec<u32> = (0..rows as u32).collect();
        let mut source = self.storage_buffer(bytemuck::cast_slice(&indices))?;
        let mut target = self.storage_buffer(&vec![0u8; indices.len() * 4])?;
        let kernel = self.compile_wgsl(include_str!("shaders/sort.wgsl"), "main")?;
        let mut width = 1u32;
        while (width as usize) < rows {
            let values = [width, desc as u32, 0, 0];
            let params = self.checked(|device, _| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("sort parameters"),
                    contents: bytemuck::cast_slice(&values),
                    usage: wgpu::BufferUsages::UNIFORM,
                })
            })?;
            let buffers = [&source, &target, &keys, &text, &params];
            let group = self.checked(|device, _| {
                let entries: Vec<_> = buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect();
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("stable merge sort"),
                    layout: &kernel.bind_group_layout(0),
                    entries: &entries,
                })
            })?;
            self.dispatch(&kernel, &[&group], [(rows as u32).div_ceil(64), 1, 1])?;
            self.sort_dispatches
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            std::mem::swap(&mut source, &mut target);
            width *= 2;
        }
        let bytes = self.read_buffer(&source)?;
        Ok(bytes
            .chunks_exact(4)
            .map(|v| u32::from_ne_bytes(v.try_into().unwrap()) as usize)
            .collect())
    }
}

impl GpuCompute {
    /// Successfully submitted binary64 group sum dispatches.
    pub fn sum_dispatches(&self) -> u64 {
        self.sum_dispatches
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Source-order IEEE-754 binary64 sums, including on devices without native f64.
    pub fn group_sums(
        &self,
        ids: &[u32],
        values: &[f64],
        groups: usize,
    ) -> Result<Vec<f64>, Error> {
        if ids.len() != values.len() || ids.iter().any(|&id| id as usize >= groups) {
            return Err(Error::runtime("GPU sum invalid groups or value count"));
        }
        if groups == 0 {
            return Ok(Vec::new());
        }
        let limits = self.device().limits();
        let max_bytes =
            (limits.max_storage_buffer_binding_size as usize).min(limits.max_buffer_size as usize);
        if groups > max_bytes / 8
            || groups.div_ceil(64) > limits.max_compute_workgroups_per_dimension as usize
        {
            return Err(Error::runtime("GPU sum groups exceed device limits"));
        }
        let chunk_rows = (max_bytes / 12).min(4096);
        if chunk_rows == 0 {
            return Err(Error::runtime("GPU limits cannot hold sum input"));
        }
        if ids.is_empty() {
            return Ok(vec![0.0; groups]);
        }
        let kernel = self.compile_wgsl(include_str!("shaders/sum.wgsl"), "main")?;
        let output = self.storage_buffer(&vec![0u8; groups * 8])?;
        for (id_chunk, value_chunk) in ids.chunks(chunk_rows).zip(values.chunks(chunk_rows)) {
            let words: Vec<u32> = id_chunk
                .iter()
                .zip(value_chunk)
                .flat_map(|(&id, &value)| {
                    let bits = value.to_bits();
                    [id, bits as u32, (bits >> 32) as u32]
                })
                .collect();
            let input = self.storage_buffer(bytemuck::cast_slice(&words))?;
            let bind = self.checked(|device, _| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("binary64 group sum"),
                    layout: &kernel.bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: input.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: output.as_entire_binding(),
                        },
                    ],
                })
            })?;
            self.dispatch(&kernel, &[&bind], [groups.div_ceil(64) as u32, 1, 1])?;
            self.sum_dispatches
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let bytes = self.read_buffer(&output)?;
        Ok(bytes
            .chunks_exact(8)
            .map(|b| {
                let lo = u32::from_ne_bytes(b[..4].try_into().unwrap()) as u64;
                let hi = u32::from_ne_bytes(b[4..].try_into().unwrap()) as u64;
                f64::from_bits(lo | (hi << 32))
            })
            .collect())
    }
}

impl GpuCompute {
    /// Submitted graph edge membership dispatches.
    pub fn graph_dispatches(&self) -> u64 {
        self.graph_dispatches
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    pub(crate) fn graph_edge_mask(
        &self,
        sources: &[u32],
        frontier: &[u32],
    ) -> Result<Vec<bool>, Error> {
        if sources.is_empty() || frontier.is_empty() {
            return Ok(vec![false; sources.len()]);
        }
        let mut frontier = frontier.to_vec();
        frontier.sort_unstable();
        frontier.dedup();
        let limits = self.device().limits();
        let max_words = (limits.max_storage_buffer_binding_size as usize)
            .min(limits.max_buffer_size as usize)
            / 4;
        if max_words == 0 {
            return Err(Error::runtime("GPU limits cannot hold graph input"));
        }
        let chunk = max_words.min(limits.max_compute_workgroups_per_dimension as usize * 64);
        if chunk == 0 {
            return Err(Error::runtime("GPU limits cannot dispatch graph input"));
        }
        let kernel = self.compile_wgsl(include_str!("shaders/graph.wgsl"), "main")?;
        let mut result = vec![false; sources.len()];
        for frontier_chunk in frontier.chunks(max_words) {
            let frontier_buffer = self.storage_buffer(bytemuck::cast_slice(frontier_chunk))?;
            for (tile, keys) in sources.chunks(chunk).enumerate() {
                let source = self.storage_buffer(bytemuck::cast_slice(keys))?;
                let output = self.storage_buffer(&vec![0u8; keys.len() * 4])?;
                let buffers = [&source, &frontier_buffer, &output];
                let group = self.checked(|device, _| {
                    let entries: Vec<_> = buffers
                        .iter()
                        .enumerate()
                        .map(|(i, b)| wgpu::BindGroupEntry {
                            binding: i as u32,
                            resource: b.as_entire_binding(),
                        })
                        .collect();
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("graph frontier"),
                        layout: &kernel.bind_group_layout(0),
                        entries: &entries,
                    })
                })?;
                self.dispatch(&kernel, &[&group], [keys.len().div_ceil(64) as u32, 1, 1])?;
                self.graph_dispatches
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                for (i, bytes) in self.read_buffer(&output)?.chunks_exact(4).enumerate() {
                    result[tile * chunk + i] |= u32::from_ne_bytes(bytes.try_into().unwrap()) != 0;
                }
            }
        }
        Ok(result)
    }
}

/// Internal resident unsigned binary64 integer scoring accumulator.
pub(crate) struct GpuWeightAccumulator<'a> {
    gpu: &'a GpuCompute,
    kernel: Option<ComputeKernel>,
    chunks: Vec<wgpu::Buffer>,
    chunk_rows: usize,
    rows: usize,
}
impl GpuCompute {
    pub fn weight_dispatches(&self) -> u64 {
        self.weight_dispatches
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    pub(crate) fn weight_accumulator(
        &self,
        rows: usize,
    ) -> Result<GpuWeightAccumulator<'_>, Error> {
        let limits = self.device().limits();
        let chunk_rows = ((limits.max_storage_buffer_binding_size as usize)
            .min(limits.max_buffer_size as usize)
            / 8)
        .min(limits.max_compute_workgroups_per_dimension as usize * 64);
        if rows > 0
            && (chunk_rows == 0
                || limits.max_storage_buffer_binding_size < 16
                || limits.max_buffer_size < 16)
        {
            return Err(Error::runtime("GPU limits cannot hold scoring accumulator"));
        }
        let mut chunks = Vec::new();
        let kernel = if rows == 0 {
            None
        } else {
            for start in (0..rows).step_by(chunk_rows) {
                chunks.push(self.storage_buffer(&vec![0u8; (rows - start).min(chunk_rows) * 8])?);
            }
            Some(self.compile_wgsl(include_str!("shaders/weights.wgsl"), "main")?)
        };
        Ok(GpuWeightAccumulator {
            gpu: self,
            kernel,
            chunks,
            chunk_rows,
            rows,
        })
    }
}
impl GpuWeightAccumulator<'_> {
    #[cfg(test)]
    fn add(&mut self, mask: &[bool], weight: u32) -> Result<(), Error> {
        if mask.len() != self.rows {
            return Err(Error::runtime("GPU scoring mask length mismatch"));
        }
        if self.rows == 0 {
            return Ok(());
        }
        for (tile, values) in mask.chunks(self.chunk_rows).enumerate() {
            let words: Vec<u32> = values.iter().map(|&hit| u32::from(hit)).collect();
            let input = self.gpu.storage_buffer(bytemuck::cast_slice(&words))?;
            self.add_buffer(&input, tile * self.chunk_rows, values.len(), weight)?;
        }
        Ok(())
    }
    fn add_buffer(
        &mut self,
        mask: &wgpu::Buffer,
        start: usize,
        count: usize,
        weight: u32,
    ) -> Result<(), Error> {
        if start.checked_add(count).is_none_or(|end| end > self.rows)
            || count > mask.size() as usize / 4
        {
            return Err(Error::runtime("GPU scoring mask range mismatch"));
        }
        let Some(kernel) = &self.kernel else {
            return Ok(());
        };
        if weight == 0 {
            return Ok(());
        }
        for (tile, output) in self.chunks.iter().enumerate() {
            let tile_start = tile * self.chunk_rows;
            let begin = start.max(tile_start);
            let end = (start + count).min(tile_start + output.size() as usize / 8);
            if begin >= end {
                continue;
            }
            let params = [
                weight,
                (begin - start) as u32,
                (begin - tile_start) as u32,
                (end - begin) as u32,
            ];
            let params = self.gpu.storage_buffer(bytemuck::cast_slice(&params))?;
            let buffers = [mask, &params, output];
            let group = self.gpu.checked(|device, _| {
                let entries: Vec<_> = buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect();
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("resident score weights"),
                    layout: &kernel.bind_group_layout(0),
                    entries: &entries,
                })
            })?;
            self.gpu
                .dispatch(kernel, &[&group], [(end - begin).div_ceil(64) as u32, 1, 1])?;
            self.gpu
                .weight_dispatches
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    }
    pub(crate) fn finish(self) -> Result<Vec<i64>, Error> {
        let mut result = Vec::with_capacity(self.rows);
        for buffer in &self.chunks {
            for bytes in self.gpu.read_buffer(buffer)?.chunks_exact(8) {
                let low = u32::from_ne_bytes(bytes[..4].try_into().unwrap()) as u64;
                let high = u32::from_ne_bytes(bytes[4..].try_into().unwrap()) as u64;
                let score = i64::try_from(low | (high << 32))
                    .map_err(|_| Error::runtime("GPU scoring overflow"))?;
                result.push(score);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod weight_tests {
    use super::*;
    #[test]
    #[ignore = "requires a hardware GPU"]
    fn resident_weights_carry_across_limbs_tiles_and_masks() {
        let mut options = GpuOptions::default();
        options.limits.max_storage_buffer_binding_size = 16;
        let gpu = GpuCompute::with_options(options).unwrap();
        let mut accumulator = gpu.weight_accumulator(5).unwrap();
        accumulator
            .add(&[true, false, true, false, true], u32::MAX)
            .unwrap();
        accumulator
            .add(&[true, true, true, false, false], 3)
            .unwrap();
        accumulator
            .add(&[false, true, true, true, false], 2)
            .unwrap();
        assert!(accumulator.add(&[false], 1).is_err());
        assert_eq!(
            accumulator.finish().unwrap(),
            [
                u32::MAX as i64 + 3,
                5,
                u32::MAX as i64 + 5,
                2,
                u32::MAX as i64
            ]
        );
        assert_eq!(gpu.weight_dispatches(), 9);
        assert!(
            gpu.weight_accumulator(0)
                .unwrap()
                .finish()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    #[ignore = "requires a hardware GPU"]
    fn resident_text_masks_cross_accumulator_tile_boundaries() {
        let mut options = GpuOptions::default();
        options.limits.max_storage_buffer_binding_size = 64;
        let gpu = GpuCompute::with_options(options).unwrap();
        let texts = [
            Some("a"),
            Some("ba"),
            None,
            Some(""),
            Some("a a"),
            Some("b"),
            Some("a"),
            Some("ba"),
            Some("a"),
            Some("b"),
            Some(""),
            Some("a"),
            Some("ba"),
            Some("a a"),
            None,
            Some("a"),
            Some("b"),
        ];
        let mut accumulator = gpu.weight_accumulator(texts.len()).unwrap();
        gpu.text_accumulate(&texts, "a", 0, &mut accumulator, 1)
            .unwrap();
        gpu.text_accumulate(&texts, "a", 3, &mut accumulator, 2)
            .unwrap();
        assert_eq!(
            accumulator.finish().unwrap(),
            [3, 1, 0, 0, 3, 0, 3, 1, 3, 0, 0, 3, 1, 3, 0, 3, 0]
        );
        assert!(gpu.weight_dispatches() > 6);
        let mut wrong_length = gpu.weight_accumulator(1).unwrap();
        assert!(
            gpu.text_accumulate(&texts, "a", 0, &mut wrong_length, 1)
                .is_err()
        );
    }
    #[test]
    #[ignore = "requires a hardware GPU"]
    fn integer_ranking_is_exact_above_f64_precision_and_stable_on_ties() {
        let gpu = GpuCompute::new().unwrap();
        let scores = [
            i64::MAX - 1,
            i64::MAX,
            9_007_199_254_740_992,
            9_007_199_254_740_993,
            -1,
            0,
            i64::MIN,
            i64::MAX,
            -2,
        ];
        let mut expected: Vec<_> = (0..scores.len()).collect();
        expected.sort_by_key(|&i| std::cmp::Reverse(scores[i]));
        assert_eq!(gpu.rank_i64(&scores).unwrap(), expected);
        assert!(gpu.sort_dispatches() > 0);
        assert!(gpu.rank_i64(&[]).unwrap().is_empty());
        assert_eq!(gpu.rank_i64(&[42]).unwrap(), [0]);
    }
    #[test]
    #[ignore = "requires a hardware GPU"]
    fn reciprocal_rank_weights_and_group_sums_match_binary64_bits() {
        let gpu = GpuCompute::new().unwrap();
        let mut denominators = vec![
            1u32,
            2,
            3,
            4,
            7,
            8,
            9,
            60,
            61,
            62,
            255,
            256,
            257,
            65535,
            65536,
            65537,
            u32::MAX - 1,
            u32::MAX,
        ];
        let mut state = 0x71488a13u32;
        for _ in 0..2048 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            denominators.push(state.max(1));
        }
        let ids: Vec<u32> = (0..denominators.len() as u32).collect();
        let scores = gpu.group_rrf(&ids, &denominators, ids.len()).unwrap();
        for (&n, score) in denominators.iter().zip(scores) {
            assert_eq!(
                score.to_bits(),
                (1.0 / n as f64).to_bits(),
                "denominator {n}"
            );
        }
        let ids: Vec<u32> = (0..5003).map(|i| (i % 7) as u32).collect();
        let values: Vec<u32> = (0..5003).map(|i| (61 + i % 503) as u32).collect();
        let mut expected = vec![0.0f64; 7];
        for (&id, &denominator) in ids.iter().zip(&values) {
            expected[id as usize] += 1.0 / denominator as f64;
        }
        let actual = gpu.group_rrf(&ids, &values, 7).unwrap();
        assert_eq!(
            actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        assert!(gpu.rrf_dispatches() >= 3);
        assert!(gpu.group_rrf(&[0], &[0], 1).is_err());
        assert!(gpu.group_rrf(&[1], &[61], 1).is_err());
        let mut options = GpuOptions::default();
        options.limits.max_storage_buffer_binding_size = 16;
        let small = GpuCompute::with_options(options).unwrap();
        let actual = small.group_rrf(&[0, 1, 0], &[61, 62, 63], 2).unwrap();
        assert_eq!(actual[0].to_bits(), (1.0f64 / 61.0 + 1.0 / 63.0).to_bits());
        assert_eq!(actual[1].to_bits(), (1.0f64 / 62.0).to_bits());
        assert_eq!(small.rrf_dispatches(), 3);
    }
}

impl GpuCompute {
    pub fn rrf_dispatches(&self) -> u64 {
        self.rrf_dispatches
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    pub(crate) fn group_rrf(
        &self,
        ids: &[u32],
        values: &[u32],
        groups: usize,
    ) -> Result<Vec<f64>, Error> {
        if ids.len() != values.len() || ids.iter().any(|&id| id as usize >= groups) {
            return Err(Error::runtime("GPU RRF invalid groups or value count"));
        }
        if groups == 0 {
            return Ok(Vec::new());
        }
        if values.contains(&0) {
            return Err(Error::runtime("GPU RRF denominator must be positive"));
        }
        let limits = self.device().limits();
        let max_bytes =
            (limits.max_storage_buffer_binding_size as usize).min(limits.max_buffer_size as usize);
        if groups > max_bytes / 8
            || groups.div_ceil(64) > limits.max_compute_workgroups_per_dimension as usize
        {
            return Err(Error::runtime("GPU RRF groups exceed device limits"));
        }
        let chunk_rows = (max_bytes / 12).min(4096);
        if chunk_rows == 0 {
            return Err(Error::runtime("GPU limits cannot hold RRF input"));
        }
        if ids.is_empty() {
            return Ok(vec![0.0; groups]);
        }
        let kernel = self.compile_wgsl(include_str!("shaders/sum.wgsl"), "rrf")?;
        let output = self.storage_buffer(&vec![0u8; groups * 8])?;
        for (id_chunk, value_chunk) in ids.chunks(chunk_rows).zip(values.chunks(chunk_rows)) {
            let words: Vec<u32> = id_chunk
                .iter()
                .zip(value_chunk)
                .flat_map(|(&id, &value)| [id, value, 0])
                .collect();
            let input = self.storage_buffer(bytemuck::cast_slice(&words))?;
            let bind = self.checked(|device, _| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("binary64 rank fusion"),
                    layout: &kernel.bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: input.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: output.as_entire_binding(),
                        },
                    ],
                })
            })?;
            self.dispatch(&kernel, &[&bind], [groups.div_ceil(64) as u32, 1, 1])?;
            self.rrf_dispatches
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let bytes = self.read_buffer(&output)?;
        Ok(bytes
            .chunks_exact(8)
            .map(|b| {
                let lo = u32::from_ne_bytes(b[..4].try_into().unwrap()) as u64;
                let hi = u32::from_ne_bytes(b[4..].try_into().unwrap()) as u64;
                f64::from_bits(lo | (hi << 32))
            })
            .collect())
    }
}

impl GpuCompute {
    pub fn regex_dispatches(&self) -> u64 {
        self.regex_dispatches
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl GpuCompute {
    pub(crate) fn walk_graph(
        &self,
        edges: &[[u32; 4]],
        seeds: &[u32],
        nodes: usize,
        identities: usize,
        depth: u32,
        graph: bool,
    ) -> Result<Vec<usize>, Error> {
        if edges.is_empty() || seeds.is_empty() {
            return Ok(Vec::new());
        }
        if seeds.iter().any(|&id| id as usize >= nodes)
            || edges.iter().any(|edge| {
                edge[0] as usize >= nodes
                    || edge[1] as usize >= nodes
                    || edge[2] as usize >= identities
            })
        {
            return Err(Error::runtime("GPU graph walk identity out of bounds"));
        }
        let max_bytes = (self.device().limits().max_storage_buffer_binding_size as usize)
            .min(self.device().limits().max_buffer_size as usize);
        let state_words = nodes
            .checked_mul(4)
            .and_then(|n| n.checked_add(identities))
            .ok_or_else(|| Error::runtime("GPU walk size overflow"))?;
        let output_words = if graph { 301 } else { nodes + 1 };
        if edges.len() > max_bytes / 16
            || state_words > max_bytes / 4
            || output_words > max_bytes / 4
            || seeds.len() + 4 > max_bytes / 4
        {
            return Err(Error::runtime("GPU graph walk exceeds buffer limits"));
        }
        let kernel = self.compile_wgsl(include_str!("shaders/walk.wgsl"), "main")?;
        let edge_buffer = self.storage_buffer(bytemuck::cast_slice(edges))?;
        let mut params = vec![nodes as u32, depth, graph as u32, 0];
        params.extend(seeds);
        let params = self.storage_buffer(bytemuck::cast_slice(&params))?;
        let state = self.storage_buffer(&vec![0u8; state_words * 4])?;
        let output = self.storage_buffer(&vec![0u8; output_words * 4])?;
        let buffers = [&edge_buffer, &params, &state, &output];
        let group = self.checked(|device, _| {
            let entries: Vec<_> = buffers
                .iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("resident graph walk"),
                layout: &kernel.bind_group_layout(0),
                entries: &entries,
            })
        })?;
        self.dispatch(&kernel, &[&group], [1, 1, 1])?;
        self.graph_dispatches
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let words: Vec<u32> = self
            .read_buffer(&output)?
            .chunks_exact(4)
            .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
            .collect();
        Ok(if graph {
            words[1..1 + words[0] as usize]
                .iter()
                .map(|&i| i as usize)
                .collect()
        } else {
            words[1..]
                .iter()
                .enumerate()
                .filter_map(|(i, &hit)| (hit != 0).then_some(i))
                .collect()
        })
    }
}

#[cfg(test)]
mod walk_tests {
    use super::*;
    #[test]
    #[ignore = "requires a hardware GPU"]
    fn resident_walk_preserves_cycles_duplicates_and_reports_limits() {
        let mut options = GpuOptions::default();
        options.limits.max_storage_buffer_binding_size = 64;
        let gpu = GpuCompute::with_options(options).unwrap();
        let edges = [[0, 1, 0, 0], [1, 2, 1, 0], [2, 0, 2, 0], [0, 1, 0, 0]];
        assert_eq!(gpu.walk_graph(&edges, &[0], 3, 3, 1, false).unwrap(), [1]);
        assert_eq!(
            gpu.walk_graph(&edges, &[0], 3, 3, 3, false).unwrap(),
            [1, 2]
        );
        assert!(
            gpu.walk_graph(&edges, &[0], 3, 3, 3, true)
                .unwrap_err()
                .to_string()
                .contains("exceeds buffer limits")
        );
        assert!(gpu.walk_graph(&edges, &[3], 3, 3, 3, false).is_err());
        assert!(
            gpu.walk_graph(&edges, &[], 3, 3, 3, true)
                .unwrap()
                .is_empty()
        );
        let gpu = GpuCompute::new().unwrap();
        assert_eq!(
            gpu.walk_graph(&edges, &[0], 3, 3, 3, true).unwrap(),
            [0, 1, 2]
        );
    }
}

impl GpuCompute {
    pub(crate) fn match_dfs(
        &self,
        edges: &[[u32; 4]],
        eligible: &[u32],
        start: u32,
        min_depth: u32,
        max_depth: u32,
        limit: usize,
    ) -> Result<Vec<(usize, usize)>, Error> {
        if edges.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        if max_depth > 3
            || min_depth == 0
            || min_depth > max_depth
            || start as usize >= eligible.len()
            || edges
                .iter()
                .any(|e| e[0] as usize >= eligible.len() || e[1] as usize >= eligible.len())
        {
            return Err(Error::runtime("GPU match invalid traversal parameters"));
        }
        let max_bytes = (self.device().limits().max_storage_buffer_binding_size as usize)
            .min(self.device().limits().max_buffer_size as usize);
        let stack_words = edges
            .len()
            .checked_mul(max_depth as usize)
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_mul(6))
            .ok_or_else(|| Error::runtime("GPU match stack size overflow"))?;
        let output_words = limit
            .checked_mul(2)
            .and_then(|n| n.checked_add(2))
            .ok_or_else(|| Error::runtime("GPU match output size overflow"))?;
        if edges.len() > max_bytes / 16
            || stack_words > max_bytes / 4
            || eligible.len() + 4 > max_bytes / 4
            || output_words > max_bytes / 4
        {
            return Err(Error::runtime("GPU match exceeds buffer limits"));
        }
        let kernel = self.compile_wgsl(include_str!("shaders/match.wgsl"), "main")?;
        let edges = self.storage_buffer(bytemuck::cast_slice(edges))?;
        let mut params = vec![start, min_depth, max_depth, limit as u32];
        params.extend(eligible);
        let params = self.storage_buffer(bytemuck::cast_slice(&params))?;
        let stack = self.storage_buffer(&vec![0u8; stack_words * 4])?;
        let output = self.storage_buffer(&vec![0u8; output_words * 4])?;
        let buffers = [&edges, &params, &stack, &output];
        let group = self.checked(|device, _| {
            let entries: Vec<_> = buffers
                .iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("match DFS"),
                layout: &kernel.bind_group_layout(0),
                entries: &entries,
            })
        })?;
        self.dispatch(&kernel, &[&group], [1, 1, 1])?;
        self.graph_dispatches
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let words: Vec<u32> = self
            .read_buffer(&output)?
            .chunks_exact(4)
            .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
            .collect();
        if words[1] != 0 {
            return Err(Error::runtime("GPU match stack overflow"));
        }
        Ok(words[2..2 + words[0] as usize * 2]
            .chunks_exact(2)
            .map(|p| (p[0] as usize, p[1] as usize))
            .collect())
    }
}

#[cfg(test)]
mod match_tests {
    use super::*;
    #[test]
    #[ignore = "requires a hardware GPU"]
    fn dfs_preserves_lifo_paths_unresolved_nodes_depth_and_limits() {
        let gpu = GpuCompute::new().unwrap();
        let edges = [
            [0, 1, 0, 0],
            [0, 2, 0, 0],
            [1, 3, 0, 0],
            [2, 3, 0, 0],
            [3, 0, 0, 0],
            [0, 1, 0, 0],
        ];
        assert_eq!(
            gpu.match_dfs(&edges, &[1, 1, 1, 1], 0, 1, 3, 300).unwrap(),
            [(1, 5), (3, 2), (2, 1), (3, 3), (1, 0), (3, 2)]
        );
        assert_eq!(
            gpu.match_dfs(&edges, &[1, 0, 1, 1], 0, 1, 3, 300).unwrap(),
            [(3, 2), (2, 1), (3, 3), (3, 2)]
        );
        assert_eq!(
            gpu.match_dfs(&edges, &[1, 1, 1, 1], 0, 2, 3, 300).unwrap(),
            [(3, 2), (3, 3), (3, 2)]
        );
        assert_eq!(
            gpu.match_dfs(&edges, &[1, 1, 1, 1], 0, 1, 3, 2).unwrap(),
            [(1, 5), (3, 2)]
        );
        assert!(
            gpu.match_dfs(&edges, &[1, 1, 1, 1], 0, 1, 3, 0)
                .unwrap()
                .is_empty()
        );
        assert!(gpu.match_dfs(&edges, &[1, 1, 1, 1], 0, 1, 4, 300).is_err());
        let mut options = GpuOptions::default();
        options.limits.max_storage_buffer_binding_size = 256;
        let small = GpuCompute::with_options(options).unwrap();
        assert!(
            small
                .match_dfs(&edges, &[1, 1, 1, 1], 0, 1, 3, 3)
                .unwrap_err()
                .to_string()
                .contains("exceeds buffer limits")
        );
        assert_eq!(
            small.match_dfs(&edges, &[1, 1, 1, 1], 0, 1, 1, 3).unwrap(),
            [(1, 5), (2, 1), (1, 0)]
        );
    }
}
