use alloc::{ffi::CString, string::String, vec::Vec};
use core::{
	ffi::c_char,
	marker::PhantomData,
	ptr::{self, NonNull},
	slice
};

use crate::{AsPointer, char_p_to_string, error::Result, memory::Allocator, ortsys};

/// Container for model metadata, including name & producer information.
pub struct ModelMetadata<'s> {
	metadata_ptr: NonNull<ort_sys::OrtModelMetadata>,
	allocator: Allocator,
	_p: PhantomData<&'s ()>
}

impl ModelMetadata<'_> {
	pub(crate) fn new(metadata_ptr: NonNull<ort_sys::OrtModelMetadata>) -> Self {
		ModelMetadata {
			metadata_ptr,
			allocator: Allocator::default(),
			_p: PhantomData
		}
	}

	/// Gets the model description, returning an error if no description is present.
	pub fn description(&self) -> Result<String> {
		let mut str_bytes: *mut c_char = ptr::null_mut();
		ortsys![unsafe ModelMetadataGetDescription(self.metadata_ptr.as_ptr(), self.allocator.ptr().cast_mut(), &mut str_bytes)?; nonNull(str_bytes)];

		let value = match char_p_to_string(str_bytes) {
			Ok(value) => value,
			Err(e) => {
				unsafe { self.allocator.free(str_bytes) };
				return Err(e);
			}
		};
		unsafe { self.allocator.free(str_bytes) };
		Ok(value)
	}

	/// Gets the description of the graph.
	pub fn graph_description(&self) -> Result<String> {
		let mut str_bytes: *mut c_char = ptr::null_mut();
		ortsys![unsafe ModelMetadataGetGraphDescription(self.metadata_ptr.as_ptr(), self.allocator.ptr().cast_mut(), &mut str_bytes)?; nonNull(str_bytes)];

		let value = match char_p_to_string(str_bytes) {
			Ok(value) => value,
			Err(e) => {
				unsafe { self.allocator.free(str_bytes) };
				return Err(e);
			}
		};
		unsafe { self.allocator.free(str_bytes) };
		Ok(value)
	}

	/// Gets the model producer name, returning an error if no producer name is present.
	pub fn producer(&self) -> Result<String> {
		let mut str_bytes: *mut c_char = ptr::null_mut();
		ortsys![unsafe ModelMetadataGetProducerName(self.metadata_ptr.as_ptr(), self.allocator.ptr().cast_mut(), &mut str_bytes)?; nonNull(str_bytes)];

		let value = match char_p_to_string(str_bytes) {
			Ok(value) => value,
			Err(e) => {
				unsafe { self.allocator.free(str_bytes) };
				return Err(e);
			}
		};
		unsafe { self.allocator.free(str_bytes) };
		Ok(value)
	}

	/// Gets the model name, returning an error if no name is present.
	pub fn name(&self) -> Result<String> {
		let mut str_bytes: *mut c_char = ptr::null_mut();
		ortsys![unsafe ModelMetadataGetGraphName(self.metadata_ptr.as_ptr(), self.allocator.ptr().cast_mut(), &mut str_bytes)?; nonNull(str_bytes)];

		let value = match char_p_to_string(str_bytes) {
			Ok(value) => value,
			Err(e) => {
				unsafe { self.allocator.free(str_bytes) };
				return Err(e);
			}
		};
		unsafe { self.allocator.free(str_bytes) };
		Ok(value)
	}

	/// Returns the model's domain, returning an error if no name is present.
	pub fn domain(&self) -> Result<String> {
		let mut str_bytes: *mut c_char = ptr::null_mut();
		ortsys![unsafe ModelMetadataGetDomain(self.metadata_ptr.as_ptr(), self.allocator.ptr().cast_mut(), &mut str_bytes)?; nonNull(str_bytes)];

		let value = match char_p_to_string(str_bytes) {
			Ok(value) => value,
			Err(e) => {
				unsafe { self.allocator.free(str_bytes) };
				return Err(e);
			}
		};
		unsafe { self.allocator.free(str_bytes) };
		Ok(value)
	}

	/// Gets the model version, returning an error if no version is present.
	pub fn version(&self) -> Result<i64> {
		let mut ver = 0i64;
		ortsys![unsafe ModelMetadataGetVersion(self.metadata_ptr.as_ptr(), &mut ver)?];
		Ok(ver)
	}

	/// Fetch the value of a custom metadata key. Returns `Ok(None)` if the key is not found.
	pub fn custom(&self, key: &str) -> Result<Option<String>> {
		let mut str_bytes: *mut c_char = ptr::null_mut();
		let key_str = CString::new(key)?;
		ortsys![unsafe ModelMetadataLookupCustomMetadataMap(self.metadata_ptr.as_ptr(), self.allocator.ptr().cast_mut(), key_str.as_ptr(), &mut str_bytes)?];
		if !str_bytes.is_null() {
			let value = match char_p_to_string(str_bytes) {
				Ok(value) => value,
				Err(e) => {
					unsafe { self.allocator.free(str_bytes) };
					return Err(e);
				}
			};
			unsafe { self.allocator.free(str_bytes) };
			Ok(Some(value))
		} else {
			Ok(None)
		}
	}

	pub fn custom_keys(&self) -> Result<Vec<String>> {
		let mut keys: *mut *mut c_char = ptr::null_mut();
		let mut key_len = 0;
		ortsys![unsafe ModelMetadataGetCustomMetadataMapKeys(self.metadata_ptr.as_ptr(), self.allocator.ptr().cast_mut(), &mut keys, &mut key_len)?];
		if key_len != 0 && !keys.is_null() {
			let res = unsafe { slice::from_raw_parts(keys, key_len as usize) }
				.iter()
				.map(|c| {
					let res = char_p_to_string(*c);
					unsafe { self.allocator.free(*c) };
					res
				})
				.collect();
			unsafe { self.allocator.free(keys) };
			res
		} else {
			Ok(Vec::new())
		}
	}
}

impl AsPointer for ModelMetadata<'_> {
	type Sys = ort_sys::OrtModelMetadata;

	fn ptr(&self) -> *const Self::Sys {
		self.metadata_ptr.as_ptr()
	}
}

impl Drop for ModelMetadata<'_> {
	fn drop(&mut self) {
		ortsys![unsafe ReleaseModelMetadata(self.metadata_ptr.as_ptr())];
	}
}
