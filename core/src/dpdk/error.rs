//! This module handles DPDK errors thrown as a C-int return type
//! It uses the internal `rte_errno` variable to obtain the errorcode.
//! After that, it returns the string representation of the error.
use std::error::Error;
use std::ffi::CStr;
use std::ffi::c_int;
use std::fmt;
use std::ptr::NonNull;

use super::rte_strerror;
use super::_rte_errno;

///
#[derive(Debug)]
pub struct DPDKError(String);

impl DPDKError {
    #[inline]
    pub fn new() -> Self {
        DPDKError(Self::from_global_errno_message())
    }

    #[inline]
    pub fn new_from_error_code(errno: c_int) -> Self {
        DPDKError(Self::get_error_message(errno))
    }

    #[inline]
    pub fn get_error_message(errno: c_int) -> String {
        unsafe {
            CStr::from_ptr(rte_strerror(errno))
                .to_str()
                .unwrap()
                .into()
        }
    }

    #[inline]
    pub fn from_global_errno_message() -> String {
        let errno = unsafe { _rte_errno() };
        Self::get_error_message(errno)
    }
}

impl Error for DPDKError {}

impl fmt::Display for DPDKError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DPDKError: {}", self.0)
    }
}


pub(crate) trait IntoResult {
    type Ok;

    fn into_result(self) -> Result<Self::Ok, DPDKError>;
}

impl IntoResult for c_int {
    type Ok = i32;

    #[inline]
    fn into_result(self) -> Result<Self::Ok, DPDKError> {
        if self >= 0{
            Ok(self as i32)
        } else {
            Err(DPDKError::new())
        }
    }
}

impl<T> IntoResult for *const T {
    type Ok = *const T;

    #[inline]
    fn into_result(self) -> Result<Self::Ok, DPDKError> {
        if self.is_null() {
            Err(DPDKError::new())
        } else {
            Ok(self)
        }
    }
}

impl<T> IntoResult for *mut T {
    type Ok = NonNull<T>;

    #[inline]
    fn into_result(self) -> Result<Self::Ok, DPDKError> {
        NonNull::new(self).ok_or_else(|| DPDKError::new())
    }
}