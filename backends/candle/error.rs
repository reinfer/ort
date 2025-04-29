use std::ffi::{CString, c_char};

#[derive(Debug, Clone)]
pub struct Error {
	pub code: ort_sys::OrtErrorCode,
	message: CString
}

impl Error {
	pub fn new(code: ort_sys::OrtErrorCode, message: impl Into<String>) -> Self {
		Self {
			code,
			message: CString::new(message.into()).unwrap()
		}
	}

	pub fn into_sys(self) -> *mut ort_sys::OrtStatus {
		(Box::leak(Box::new(self)) as *mut Error).cast()
	}

	pub fn new_sys(code: ort_sys::OrtErrorCode, message: impl Into<String>) -> *mut ort_sys::OrtStatus {
		Self::new(code, message).into_sys()
	}

	#[inline]
	pub fn message(&self) -> &str {
		self.message.as_c_str().to_str().unwrap()
	}

	#[inline]
	pub fn message_ptr(&self) -> *const c_char {
		self.message.as_ptr()
	}

	pub unsafe fn cast_from_sys<'e>(ptr: *const ort_sys::OrtStatus) -> &'e Error {
		unsafe { &*ptr.cast::<Error>() }
	}

	pub unsafe fn consume_sys(ptr: *mut ort_sys::OrtStatus) -> Box<Error> {
		Box::from_raw(ptr.cast::<Error>())
	}
}
