use alloc::{
	format,
	string::{String, ToString}
};

use super::{ArbitrarilyConfigurableExecutionProvider, ExecutionProviderOptions};
use crate::{
	error::{Error, Result},
	execution_providers::{ExecutionProvider, ExecutionProviderDispatch},
	session::builder::SessionBuilder
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebGPUPreferredLayout {
	NCHW,
	NHWC
}

impl WebGPUPreferredLayout {
	#[must_use]
	pub fn as_str(&self) -> &'static str {
		match self {
			WebGPUPreferredLayout::NCHW => "NCHW",
			WebGPUPreferredLayout::NHWC => "NHWC"
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebGPUDawnBackendType {
	Vulkan,
	D3D12
}

impl WebGPUDawnBackendType {
	#[must_use]
	pub fn as_str(&self) -> &'static str {
		match self {
			WebGPUDawnBackendType::Vulkan => "Vulkan",
			WebGPUDawnBackendType::D3D12 => "D3D12"
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebGPUBufferCacheMode {
	Disabled,
	LazyRelease,
	Simple,
	Bucket
}

impl WebGPUBufferCacheMode {
	#[must_use]
	pub fn as_str(&self) -> &'static str {
		match self {
			WebGPUBufferCacheMode::Disabled => "disabled",
			WebGPUBufferCacheMode::LazyRelease => "lazyRelease",
			WebGPUBufferCacheMode::Simple => "simple",
			WebGPUBufferCacheMode::Bucket => "bucket"
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebGPUValidationMode {
	Disabled,
	WgpuOnly,
	Basic,
	Full
}

impl WebGPUValidationMode {
	#[must_use]
	pub fn as_str(&self) -> &'static str {
		match self {
			WebGPUValidationMode::Disabled => "disabled",
			WebGPUValidationMode::WgpuOnly => "wgpuOnly",
			WebGPUValidationMode::Basic => "basic",
			WebGPUValidationMode::Full => "full"
		}
	}
}

#[derive(Debug, Default, Clone)]
pub struct WebGPUExecutionProvider {
	options: ExecutionProviderOptions
}

impl WebGPUExecutionProvider {
	#[must_use]
	pub fn with_preferred_layout(mut self, layout: WebGPUPreferredLayout) -> Self {
		self.options.set("WebGPU:preferredLayout", layout.as_str());
		self
	}

	#[must_use]
	pub fn with_enable_graph_capture(mut self, enable: bool) -> Self {
		self.options.set("WebGPU:enableGraphCapture", if enable { "1" } else { "0" });
		self
	}

	#[must_use]
	pub fn with_dawn_proc_table(mut self, table: String) -> Self {
		self.options.set("WebGPU:dawnProcTable", table);
		self
	}

	#[must_use]
	pub fn with_dawn_backend_type(mut self, backend_type: WebGPUDawnBackendType) -> Self {
		self.options.set("WebGPU:dawnBackendType", backend_type.as_str());
		self
	}

	#[must_use]
	pub fn with_device_id(mut self, id: i32) -> Self {
		self.options.set("WebGPU:deviceId", id.to_string());
		self
	}

	#[must_use]
	pub fn with_storage_buffer_cache_mode(mut self, mode: WebGPUBufferCacheMode) -> Self {
		self.options.set("WebGPU:storageBufferCacheMode", mode.as_str());
		self
	}

	#[must_use]
	pub fn with_uniform_buffer_cache_mode(mut self, mode: WebGPUBufferCacheMode) -> Self {
		self.options.set("WebGPU:uniformBufferCacheMode", mode.as_str());
		self
	}

	#[must_use]
	pub fn with_query_resolve_buffer_cache_mode(mut self, mode: WebGPUBufferCacheMode) -> Self {
		self.options.set("WebGPU:queryResolveBufferCacheMode", mode.as_str());
		self
	}

	#[must_use]
	pub fn with_default_buffer_cache_mode(mut self, mode: WebGPUBufferCacheMode) -> Self {
		self.options.set("WebGPU:defaultBufferCacheMode", mode.as_str());
		self
	}

	#[must_use]
	pub fn with_validation_mode(mut self, mode: WebGPUValidationMode) -> Self {
		self.options.set("WebGPU:validationMode", mode.as_str());
		self
	}

	#[must_use]
	pub fn with_force_cpu_node_names(mut self, names: String) -> Self {
		self.options.set("WebGPU:forceCpuNodeNames", names);
		self
	}

	#[must_use]
	pub fn with_enable_pix_capture(mut self, enable: bool) -> Self {
		self.options.set("WebGPU:enablePIXCapture", if enable { "1" } else { "0" });
		self
	}

	#[must_use]
	pub fn build(self) -> ExecutionProviderDispatch {
		self.into()
	}
}

impl ArbitrarilyConfigurableExecutionProvider for WebGPUExecutionProvider {
	fn with_arbitrary_config(mut self, key: impl ToString, value: impl ToString) -> Self {
		self.options.set(key.to_string(), value.to_string());
		self
	}
}

impl From<WebGPUExecutionProvider> for ExecutionProviderDispatch {
	fn from(value: WebGPUExecutionProvider) -> Self {
		ExecutionProviderDispatch::new(value)
	}
}

impl ExecutionProvider for WebGPUExecutionProvider {
	fn as_str(&self) -> &'static str {
		"WebGpuExecutionProvider"
	}

	fn supported_by_platform(&self) -> bool {
		cfg!(any(target_os = "windows", target_os = "linux", target_arch = "wasm32"))
	}

	#[allow(unused, unreachable_code)]
	fn register(&self, session_builder: &mut SessionBuilder) -> Result<()> {
		#[cfg(any(feature = "load-dynamic", feature = "webgpu"))]
		{
			use crate::AsPointer;

			let ffi_options = self.options.to_ffi();
			crate::ortsys![unsafe SessionOptionsAppendExecutionProvider(
				session_builder.ptr_mut(),
				c"WebGPU".as_ptr().cast::<core::ffi::c_char>(),
				ffi_options.key_ptrs(),
				ffi_options.value_ptrs(),
				ffi_options.len(),
			)?];
			return Ok(());
		}

		Err(Error::new(format!("`{}` was not registered because its corresponding Cargo feature is not enabled.", self.as_str())))
	}
}
