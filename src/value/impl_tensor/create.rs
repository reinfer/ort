use std::{
	any::Any,
	ffi,
	fmt::Debug,
	marker::PhantomData,
	ptr::{self, NonNull},
	sync::Arc
};

#[cfg(feature = "ndarray")]
use ndarray::{ArcArray, Array, ArrayView, ArrayViewMut, CowArray, Dimension};

use super::{Tensor, TensorRef, TensorRefMut, calculate_tensor_size};
use crate::{
	AsPointer,
	error::{Error, ErrorCode, Result, assert_non_null_pointer},
	memory::{AllocationDevice, Allocator, AllocatorType, MemoryInfo, MemoryType},
	ortsys,
	tensor::{PrimitiveTensorElementType, TensorElementType, Utf8Data},
	value::{Value, ValueInner, ValueType}
};

impl Tensor<String> {
	/// Construct a [`Tensor`] from an array of strings.
	///
	/// Just like numeric tensors, string tensors can be created from:
	/// - (with feature `ndarray`) a shared reference to a [`ndarray::CowArray`] (`&CowArray<'_, T, D>`);
	/// - (with feature `ndarray`) a mutable/exclusive reference to an [`ndarray::ArcArray`] (`&mut ArcArray<T, D>`);
	/// - (with feature `ndarray`) an owned [`ndarray::Array`];
	/// - (with feature `ndarray`) a borrowed view of another array, as an [`ndarray::ArrayView`] (`ArrayView<'_, T,
	///   D>`);
	/// - a tuple of `(dimensions, data)` where:
	///   * `dimensions` is one of `Vec<I>`, `[I]` or `&[I]`, where `I` is `i64` or `usize`;
	///   * and `data` is one of `Vec<T>`, `Box<[T]>`, `Arc<Box<[T]>>`, or `&[T]`.
	///
	/// ```
	/// # use ort::{session::Session, value::Tensor};
	/// # fn main() -> ort::Result<()> {
	/// // Create a string tensor from a raw data vector
	/// let data = vec!["hello", "world"];
	/// let value = Tensor::from_string_array(([data.len()], data.into_boxed_slice()))?;
	///
	/// // Create a string tensor from an `ndarray::Array`
	/// #[cfg(feature = "ndarray")]
	/// let value = Tensor::from_string_array(ndarray::Array::from_shape_vec((1,), vec!["document".to_owned()]).unwrap())?;
	/// # 	Ok(())
	/// # }
	/// ```
	///
	/// Note that string data will *always* be copied, no matter what form the data is provided in.
	pub fn from_string_array<T: Utf8Data>(input: impl TensorArrayData<T>) -> Result<Tensor<String>> {
		let mut value_ptr: *mut ort_sys::OrtValue = ptr::null_mut();

		let (shape, data, _guard) = input.ref_parts()?;
		let shape_ptr: *const i64 = shape.as_ptr();
		let shape_len = shape.len();

		// create tensor without data -- data is filled in later
		ortsys![
			unsafe CreateTensorAsOrtValue(Allocator::default().ptr_mut(), shape_ptr, shape_len, TensorElementType::String.into(), &mut value_ptr)?;
			nonNull(value_ptr)
		];

		// create null-terminated copies of each string, as per `FillStringTensor` docs
		let null_terminated_copies: Vec<ffi::CString> = data
			.iter()
			.map(|elt| {
				let slice = elt.as_utf8_bytes();
				ffi::CString::new(slice)
			})
			.collect::<Result<Vec<_>, _>>()
			.map_err(Error::wrap)?;

		let string_pointers = null_terminated_copies.iter().map(|cstring| cstring.as_ptr()).collect::<Vec<_>>();

		ortsys![unsafe FillStringTensor(value_ptr, string_pointers.as_ptr(), string_pointers.len())?];

		Ok(Value {
			inner: Arc::new(ValueInner {
				ptr: unsafe { NonNull::new_unchecked(value_ptr) },
				dtype: ValueType::Tensor {
					ty: TensorElementType::String,
					dimensions: shape,
					dimension_symbols: vec![None; shape_len]
				},
				memory_info: MemoryInfo::from_value(value_ptr),
				drop: true,
				_backing: None
			}),
			_markers: PhantomData
		})
	}
}

impl<T: PrimitiveTensorElementType + Debug> Tensor<T> {
	/// Construct a tensor in a given allocator with a given shape and datatype. The data contained in the
	/// value will be zero-allocated on the allocation device.
	///
	/// This can be used to create a tensor with data on a certain device. For example, to create a tensor with pinned
	/// (CPU) memory for use with CUDA:
	/// ```no_run
	/// # use ort::{memory::{Allocator, MemoryInfo, MemoryType, AllocationDevice, AllocatorType}, session::Session, value::Tensor};
	/// # fn main() -> ort::Result<()> {
	/// # let session = Session::builder()?.commit_from_file("tests/data/upsample.onnx")?;
	/// let allocator = Allocator::new(
	/// 	&session,
	/// 	MemoryInfo::new(AllocationDevice::CUDA_PINNED, 0, AllocatorType::Device, MemoryType::CPUInput)?
	/// )?;
	///
	/// let mut img_input = Tensor::<f32>::new(&allocator, [1, 128, 128, 3])?;
	/// # Ok(())
	/// # }
	/// ```
	pub fn new(allocator: &Allocator, shape: impl ToDimensions) -> Result<Tensor<T>> {
		let mut value_ptr: *mut ort_sys::OrtValue = ptr::null_mut();

		let shape = shape.to_dimensions(None)?;
		let shape_ptr: *const i64 = shape.as_ptr();
		let shape_len = shape.len();

		ortsys![
			unsafe CreateTensorAsOrtValue(
				allocator.ptr().cast_mut(),
				shape_ptr,
				shape_len,
				T::into_tensor_element_type().into(),
				&mut value_ptr
			)?;
			nonNull(value_ptr)
		];

		Ok(Value {
			inner: Arc::new(ValueInner {
				ptr: unsafe { NonNull::new_unchecked(value_ptr) },
				dtype: ValueType::Tensor {
					ty: T::into_tensor_element_type(),
					dimensions: shape,
					dimension_symbols: vec![None; shape_len]
				},
				drop: true,
				memory_info: MemoryInfo::from_value(value_ptr),
				_backing: None
			}),
			_markers: PhantomData
		})
	}

	/// Construct a tensor from an array of data.
	///
	/// Tensors can be created from:
	/// - (with feature `ndarray`) a shared reference to a [`ndarray::CowArray`] (`&CowArray<'_, T, D>`);
	/// - (with feature `ndarray`) a mutable/exclusive reference to an [`ndarray::ArcArray`] (`&mut ArcArray<T, D>`);
	/// - (with feature `ndarray`) an owned [`ndarray::Array`];
	/// - (with feature `ndarray`) a borrowed view of another array, as an [`ndarray::ArrayView`] (`ArrayView<'_, T,
	///   D>`);
	/// - a tuple of `(dimensions, data)` where:
	///   * `dimensions` is one of `Vec<I>`, `[I]` or `&[I]`, where `I` is `i64` or `usize`;
	///   * and `data` is one of `Vec<T>`, `Box<[T]>`, `Arc<Box<[T]>>`, or `&[T]`.
	///
	/// ```
	/// # use ort::value::Tensor;
	/// # fn main() -> ort::Result<()> {
	/// // Create a tensor from a raw data vector
	/// let tensor = Tensor::from_array(([1usize, 2, 3], vec![1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0].into_boxed_slice()))?;
	///
	/// // Create a tensor from an `ndarray::Array`
	/// #[cfg(feature = "ndarray")]
	/// let tensor = Tensor::from_array(ndarray::Array4::<f32>::zeros((1, 16, 16, 3)))?;
	/// # 	Ok(())
	/// # }
	/// ```
	///
	/// Creating string tensors requires a separate method; see [`DynTensor::from_string_array`].
	///
	/// Note that data provided in an `ndarray` may be copied in some circumstances:
	/// - `&CowArray<'_, T, D>` will always be copied regardless of whether it is uniquely owned or borrowed.
	/// - `&mut ArcArray<T, D>` and `Array<T, D>` will be copied only if the data is not in a contiguous layout (which
	///   is the case after most reshape operations)
	/// - `ArrayView<'_, T, D>` will always be copied.
	///
	/// Raw data provided as a `Arc<Box<[T]>>`, `Box<[T]>`, or `Vec<T>` will never be copied. Raw data is expected to be
	/// in standard, contigous layout.
	pub fn from_array(input: impl OwnedTensorArrayData<T>) -> Result<Tensor<T>> {
		let memory_info = MemoryInfo::new(AllocationDevice::CPU, 0, AllocatorType::Arena, MemoryType::CPUInput)?;

		let mut value_ptr: *mut ort_sys::OrtValue = ptr::null_mut();

		// f16 and bf16 are repr(transparent) to u16, so memory layout should be identical to onnxruntime
		let TensorArrayDataParts { shape, ptr, num_elements, guard } = input.into_parts()?;
		let shape_ptr: *const i64 = shape.as_ptr();
		let shape_len = shape.len();

		let tensor_values_ptr: *mut std::ffi::c_void = ptr.cast();
		assert_non_null_pointer(tensor_values_ptr, "TensorValues")?;

		ortsys![
			unsafe CreateTensorWithDataAsOrtValue(
				memory_info.ptr(),
				tensor_values_ptr,
				num_elements * std::mem::size_of::<T>(),
				shape_ptr,
				shape_len,
				T::into_tensor_element_type().into(),
				&mut value_ptr
			)?;
			nonNull(value_ptr)
		];

		Ok(Value {
			inner: Arc::new(ValueInner {
				ptr: unsafe { NonNull::new_unchecked(value_ptr) },
				dtype: ValueType::Tensor {
					ty: T::into_tensor_element_type(),
					dimensions: shape,
					dimension_symbols: vec![None; shape_len]
				},
				drop: true,
				memory_info: Some(memory_info),
				_backing: Some(guard)
			}),
			_markers: PhantomData
		})
	}
}

impl<'a, T: PrimitiveTensorElementType + Debug> TensorRef<'a, T> {
	pub fn from_array_view(input: impl TensorArrayData<T> + 'a) -> Result<TensorRef<'a, T>> {
		let memory_info = MemoryInfo::new(AllocationDevice::CPU, 0, AllocatorType::Arena, MemoryType::CPUInput)?;

		let mut value_ptr: *mut ort_sys::OrtValue = ptr::null_mut();

		// f16 and bf16 are repr(transparent) to u16, so memory layout should be identical to onnxruntime
		let (shape, data, guard) = input.ref_parts()?;
		let num_elements = calculate_tensor_size(&shape);
		let shape_ptr: *const i64 = shape.as_ptr();
		let shape_len = shape.len();

		let tensor_values_ptr: *mut std::ffi::c_void = data.as_ptr() as *mut _;
		assert_non_null_pointer(tensor_values_ptr, "TensorValues")?;

		ortsys![
			unsafe CreateTensorWithDataAsOrtValue(
				memory_info.ptr(),
				tensor_values_ptr,
				num_elements * std::mem::size_of::<T>(),
				shape_ptr,
				shape_len,
				T::into_tensor_element_type().into(),
				&mut value_ptr
			)?;
			nonNull(value_ptr)
		];

		let mut tensor = TensorRef::new(Tensor {
			inner: Arc::new(ValueInner {
				ptr: unsafe { NonNull::new_unchecked(value_ptr) },
				dtype: ValueType::Tensor {
					ty: T::into_tensor_element_type(),
					dimensions: shape,
					dimension_symbols: vec![None; shape_len]
				},
				drop: true,
				memory_info: Some(memory_info),
				_backing: guard
			}),
			_markers: PhantomData
		});
		tensor.upgradable = false;
		Ok(tensor)
	}
}

impl<'a, T: PrimitiveTensorElementType + Debug> TensorRefMut<'a, T> {
	pub fn from_array_view_mut(mut input: impl TensorArrayDataMut<T>) -> Result<TensorRefMut<'a, T>> {
		let memory_info = MemoryInfo::new(AllocationDevice::CPU, 0, AllocatorType::Arena, MemoryType::CPUInput)?;

		let mut value_ptr: *mut ort_sys::OrtValue = ptr::null_mut();

		// f16 and bf16 are repr(transparent) to u16, so memory layout should be identical to onnxruntime
		let (shape, data, guard) = input.ref_parts_mut()?;
		let num_elements = calculate_tensor_size(&shape);
		let shape_ptr: *const i64 = shape.as_ptr();
		let shape_len = shape.len();

		let tensor_values_ptr: *mut std::ffi::c_void = data.as_ptr() as *mut _;
		assert_non_null_pointer(tensor_values_ptr, "TensorValues")?;

		ortsys![
			unsafe CreateTensorWithDataAsOrtValue(
				memory_info.ptr(),
				tensor_values_ptr,
				num_elements * std::mem::size_of::<T>(),
				shape_ptr,
				shape_len,
				T::into_tensor_element_type().into(),
				&mut value_ptr
			)?;
			nonNull(value_ptr)
		];

		let mut tensor = TensorRefMut::new(Tensor {
			inner: Arc::new(ValueInner {
				ptr: unsafe { NonNull::new_unchecked(value_ptr) },
				dtype: ValueType::Tensor {
					ty: T::into_tensor_element_type(),
					dimensions: shape,
					dimension_symbols: vec![None; shape_len]
				},
				drop: true,
				memory_info: Some(memory_info),
				_backing: guard
			}),
			_markers: PhantomData
		});
		tensor.upgradable = false;
		Ok(tensor)
	}

	/// Create a mutable tensor view from a raw pointer and shape.
	///
	/// The length of data is determined by `T` and the given shape, so the given buffer must be at least
	/// `shape.iter().product() * std::mem::size_of::<T>()` bytes.
	///
	/// This function can be used to create data from raw device memory, e.g. to directly provide data to an execution
	/// provider. For instance, to create a tensor from a raw CUDA buffer using [`cudarc`](https://docs.rs/cudarc):
	/// ```ignore
	/// let device = CudaDevice::new(0)?;
	/// let device_data = device.htod_sync_copy(&input_data)?;
	///
	/// let tensor: TensorRefMut<'_, f32> = unsafe {
	/// 	TensorRefMut::from_raw(
	/// 		MemoryInfo::new(AllocationDevice::CUDA, 0, AllocatorType::Device, MemoryType::Default)?,
	/// 		(*device_data.device_ptr() as usize as *mut ()).cast(),
	/// 		vec![1, 3, 512, 512]
	/// 	)?
	/// };
	/// ```
	///
	/// # Safety
	/// - The pointer must be valid for the device description provided by `MemoryInfo`.
	/// - The returned tensor must outlive the data described by the data pointer.
	pub unsafe fn from_raw(info: MemoryInfo, data: *mut ort_sys::c_void, shape: Vec<i64>) -> Result<TensorRefMut<'a, T>> {
		let mut value_ptr: *mut ort_sys::OrtValue = ptr::null_mut();

		// f16 and bf16 are repr(transparent) to u16, so memory layout should be identical to onnxruntime
		let shape_ptr: *const i64 = shape.as_ptr();
		let shape_len = shape.len();

		let data_len = shape.iter().product::<i64>() as usize * std::mem::size_of::<T>();

		ortsys![
			unsafe CreateTensorWithDataAsOrtValue(
				info.ptr(),
				data,
				data_len,
				shape_ptr,
				shape_len,
				T::into_tensor_element_type().into(),
				&mut value_ptr
			)?;
			nonNull(value_ptr)
		];

		let mut tensor = TensorRefMut::new(Value {
			inner: Arc::new(ValueInner {
				ptr: unsafe { NonNull::new_unchecked(value_ptr) },
				dtype: ValueType::Tensor {
					ty: T::into_tensor_element_type(),
					dimensions: shape,
					dimension_symbols: vec![None; shape_len]
				},
				drop: true,
				memory_info: Some(info),
				_backing: None
			}),
			_markers: PhantomData
		});
		tensor.upgradable = false;
		Ok(tensor)
	}
}

pub trait TensorArrayData<I> {
	fn ref_parts(&self) -> Result<(Vec<i64>, &[I], Option<Box<dyn Any>>)>;
}

pub trait TensorArrayDataMut<I>: TensorArrayData<I> {
	fn ref_parts_mut(&mut self) -> Result<(Vec<i64>, &mut [I], Option<Box<dyn Any>>)>;
}

pub trait OwnedTensorArrayData<I> {
	fn into_parts(self) -> Result<TensorArrayDataParts<I>>;
}

pub struct TensorArrayDataParts<I> {
	pub shape: Vec<i64>,
	pub ptr: *mut I,
	pub num_elements: usize,
	pub guard: Box<dyn Any>
}

pub trait ToDimensions {
	fn to_dimensions(&self, expected_size: Option<usize>) -> Result<Vec<i64>>;
}

macro_rules! impl_to_dimensions {
	(@inner) => {
		fn to_dimensions(&self, expected_size: Option<usize>) -> Result<Vec<i64>> {
			let v: Vec<i64> = self
				.iter()
				.enumerate()
				.map(|(i, c)| {
					if *c >= 1 {
						Ok(*c as i64)
					} else {
						Err(Error::new_with_code(
							ErrorCode::InvalidArgument,
							format!("Invalid dimension at {}; all dimensions must be >= 1 when creating a tensor from raw data", i)
						))
					}
				})
				.collect::<Result<_>>()?;
			let sum = calculate_tensor_size(&v);
			if let Some(expected_size) = expected_size {
				if sum != expected_size {
					Err(Error::new_with_code(
						ErrorCode::InvalidArgument,
						format!("Cannot create a tensor from raw data; shape {:?} ({}) is larger than the length of the data provided ({})", v, sum, expected_size)
					))
				} else {
					Ok(v)
				}
			} else {
				Ok(v)
			}
		}
	};
	($(for $t:ty),+) => {
		$(impl ToDimensions for $t {
			impl_to_dimensions!(@inner);
		})+
	};
	(<N> $(for $t:ty),+) => {
		$(impl<const N: usize> ToDimensions for $t {
			impl_to_dimensions!(@inner);
		})+
	};
}

impl ToDimensions for () {
	fn to_dimensions(&self, expected_size: Option<usize>) -> Result<Vec<i64>> {
		match expected_size {
			Some(1) | None => Ok(vec![]),
			Some(_) => Err(Error::new_with_code(ErrorCode::InvalidArgument, "Expected data to have a length of exactly 1 for scalar shape"))
		}
	}
}
impl_to_dimensions!(for &[usize], for &[i32], for &[i64], for Vec<usize>, for Vec<i32>, for Vec<i64>);
impl_to_dimensions!(<N> for [usize; N], for [i32; N], for [i64; N]);

#[cfg(feature = "ndarray")]
#[cfg_attr(docsrs, doc(cfg(feature = "ndarray")))]
impl<T: Clone + 'static, D: Dimension + 'static> TensorArrayData<T> for &CowArray<'_, T, D> {
	fn ref_parts(&self) -> Result<(Vec<i64>, &[T], Option<Box<dyn Any>>)> {
		let shape: Vec<i64> = self.shape().iter().map(|d| *d as i64).collect();
		let data = self
			.as_slice()
			.ok_or_else(|| Error::new("Array has a non-contiguous layout and cannot be used to construct a Tensor"))?;
		Ok((shape, data, None))
	}
}

#[cfg(feature = "ndarray")]
#[cfg_attr(docsrs, doc(cfg(feature = "ndarray")))]
impl<T: Clone + 'static, D: Dimension + 'static> TensorArrayData<T> for ArcArray<T, D> {
	fn ref_parts(&self) -> Result<(Vec<i64>, &[T], Option<Box<dyn Any>>)> {
		let shape: Vec<i64> = self.shape().iter().map(|d| *d as i64).collect();
		let data = self
			.as_slice()
			.ok_or_else(|| Error::new("Array has a non-contiguous layout and cannot be used to construct a Tensor"))?;
		Ok((shape, data, Some(Box::new(self.clone()))))
	}
}

#[cfg(feature = "ndarray")]
#[cfg_attr(docsrs, doc(cfg(feature = "ndarray")))]
impl<T: Clone + 'static, D: Dimension + 'static> TensorArrayData<T> for &Array<T, D> {
	fn ref_parts(&self) -> Result<(Vec<i64>, &[T], Option<Box<dyn Any>>)> {
		let shape: Vec<i64> = self.shape().iter().map(|d| *d as i64).collect();
		let data = self
			.as_slice()
			.ok_or_else(|| Error::new("Array has a non-contiguous layout and cannot be used to construct a Tensor"))?;
		Ok((shape, data, None))
	}
}

#[cfg(feature = "ndarray")]
#[cfg_attr(docsrs, doc(cfg(feature = "ndarray")))]
impl<T: Clone + 'static, D: Dimension + 'static> OwnedTensorArrayData<T> for Array<T, D> {
	fn into_parts(self) -> Result<TensorArrayDataParts<T>> {
		if self.is_standard_layout() {
			// We can avoid the copy here and use the data as is
			let mut guard = Box::new(self);
			let shape: Vec<i64> = guard.shape().iter().map(|d| *d as i64).collect();
			let ptr = guard.as_mut_ptr();
			let num_elements = guard.len();
			Ok(TensorArrayDataParts { shape, ptr, num_elements, guard })
		} else {
			// Need to do a copy here to get data in to standard layout
			let mut contiguous_array = self.as_standard_layout().into_owned();
			let shape: Vec<i64> = contiguous_array.shape().iter().map(|d| *d as i64).collect();
			let ptr = contiguous_array.as_mut_ptr();
			let num_elements: usize = contiguous_array.len();
			let guard = Box::new(contiguous_array);
			Ok(TensorArrayDataParts { shape, ptr, num_elements, guard })
		}
	}
}

#[cfg(feature = "ndarray")]
#[cfg_attr(docsrs, doc(cfg(feature = "ndarray")))]
impl<T: Clone + 'static, D: Dimension + 'static> TensorArrayData<T> for ArrayView<'_, T, D> {
	fn ref_parts(&self) -> Result<(Vec<i64>, &[T], Option<Box<dyn Any>>)> {
		let shape: Vec<i64> = self.shape().iter().map(|d| *d as i64).collect();
		let data = self
			.as_slice()
			.ok_or_else(|| Error::new("Array has a non-contiguous layout and cannot be used to construct a Tensor"))?;
		Ok((shape, data, None))
	}
}

#[cfg(feature = "ndarray")]
#[cfg_attr(docsrs, doc(cfg(feature = "ndarray")))]
impl<T: Clone + 'static, D: Dimension + 'static> TensorArrayData<T> for ArrayViewMut<'_, T, D> {
	fn ref_parts(&self) -> Result<(Vec<i64>, &[T], Option<Box<dyn Any>>)> {
		let shape: Vec<i64> = self.shape().iter().map(|d| *d as i64).collect();
		let data = self
			.as_slice()
			.ok_or_else(|| Error::new("Array has a non-contiguous layout and cannot be used to construct a Tensor"))?;
		Ok((shape, data, None))
	}
}

#[cfg(feature = "ndarray")]
#[cfg_attr(docsrs, doc(cfg(feature = "ndarray")))]
impl<T: Clone + 'static, D: Dimension + 'static> TensorArrayDataMut<T> for ArrayViewMut<'_, T, D> {
	fn ref_parts_mut(&mut self) -> Result<(Vec<i64>, &mut [T], Option<Box<dyn Any>>)> {
		let shape: Vec<i64> = self.shape().iter().map(|d| *d as i64).collect();
		let data = self
			.as_slice_mut()
			.ok_or_else(|| Error::new("Array has a non-contiguous layout and cannot be used to construct a Tensor"))?;
		Ok((shape, data, None))
	}
}

impl<T: Clone + 'static, D: ToDimensions> TensorArrayData<T> for (D, &[T]) {
	fn ref_parts(&self) -> Result<(Vec<i64>, &[T], Option<Box<dyn Any>>)> {
		let shape = self.0.to_dimensions(Some(self.1.len()))?;
		Ok((shape, self.1, None))
	}
}

impl<T: Clone + 'static, D: ToDimensions> TensorArrayData<T> for (D, &mut [T]) {
	fn ref_parts(&self) -> Result<(Vec<i64>, &[T], Option<Box<dyn Any>>)> {
		let shape = self.0.to_dimensions(Some(self.1.len()))?;
		Ok((shape, self.1, None))
	}
}

impl<T: Clone + 'static, D: ToDimensions> TensorArrayDataMut<T> for (D, &mut [T]) {
	fn ref_parts_mut(&mut self) -> Result<(Vec<i64>, &mut [T], Option<Box<dyn Any>>)> {
		let shape = self.0.to_dimensions(Some(self.1.len()))?;
		Ok((shape, self.1, None))
	}
}

impl<T: Clone + 'static, D: ToDimensions> OwnedTensorArrayData<T> for (D, Vec<T>) {
	fn into_parts(mut self) -> Result<TensorArrayDataParts<T>> {
		let shape = self.0.to_dimensions(Some(self.1.len()))?;
		let ptr = self.1.as_mut_ptr();
		let num_elements: usize = self.1.len();
		Ok(TensorArrayDataParts {
			shape,
			ptr,
			num_elements,
			guard: Box::new(self.1)
		})
	}
}

impl<T: Clone + 'static, D: ToDimensions> TensorArrayData<T> for (D, Box<[T]>) {
	fn ref_parts(&self) -> Result<(Vec<i64>, &[T], Option<Box<dyn Any>>)> {
		let shape = self.0.to_dimensions(Some(self.1.len()))?;
		let data = &*self.1;
		Ok((shape, data, None))
	}
}

impl<T: Clone + 'static, D: ToDimensions> OwnedTensorArrayData<T> for (D, Box<[T]>) {
	fn into_parts(mut self) -> Result<TensorArrayDataParts<T>> {
		let shape = self.0.to_dimensions(Some(self.1.len()))?;
		let ptr = self.1.as_mut_ptr();
		let num_elements: usize = self.1.len();
		Ok(TensorArrayDataParts {
			shape,
			ptr,
			num_elements,
			guard: Box::new(self.1)
		})
	}
}

impl<T: Clone + 'static, D: ToDimensions> TensorArrayData<T> for (D, Arc<Box<[T]>>) {
	fn ref_parts(&self) -> Result<(Vec<i64>, &[T], Option<Box<dyn Any>>)> {
		let shape = self.0.to_dimensions(Some(self.1.len()))?;
		let data = &*self.1;
		Ok((shape, data, Some(Box::new(self.1.clone()))))
	}
}
