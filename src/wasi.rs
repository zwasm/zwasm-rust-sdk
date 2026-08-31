use std::{ffi::CString, os::raw::c_char};

use zwasm_sys as sys;

use crate::error::{non_null, Error};

/// WASI 0.1 host setup, wrapping `zwasm_wasi_config_t`.
///
/// Build one, then hand it to
/// [`Store::set_wasi`](crate::store::Store::set_wasi), which consumes it. After
/// that, imports of `wasi_snapshot_preview1.*` resolve against this host.
///
/// This is a zwasm extension rather than part of the wasm-c-api, so it is not
/// portable to other runtimes.
#[derive(Debug)]
pub struct WasiConfig {
    pub(crate) ptr: *mut sys::zwasm_wasi_config_t,
}

impl WasiConfig {
    /// Creates a config with the defaults: the three standard streams wired to the
    /// host process, no args, no envs, no preopens.
    pub fn new() -> Result<Self, Error> {
        let ptr = non_null(
            unsafe { sys::zwasm_wasi_config_new() },
            "failed to create WASI config",
        )?;
        Ok(WasiConfig { ptr })
    }

    /// Routes the guest's stdin, stdout and stderr to the host process.
    ///
    /// This is already the default, and the C API keeps the call for parity, so
    /// there is normally no reason to make it.
    pub fn inherit_stdio(&mut self) {
        unsafe { sys::zwasm_wasi_config_inherit_stdio(self.ptr) };
    }

    /// Copies the host process's environment into the config, replacing whatever
    /// [`WasiConfig::set_envs`] had put there.
    ///
    /// This is a snapshot. Later changes to the host environment are not picked up.
    pub fn inherit_env(&mut self) -> Result<(), Error> {
        let result = unsafe { sys::zwasm_wasi_config_inherit_env(self.ptr) };
        if result {
            Ok(())
        } else {
            Err(Error::Message("failed to inherit env".to_string()))
        }
    }

    /// Sets the guest's argv, replacing any previous value.
    ///
    /// By convention the first entry is the program name. The strings are copied.
    /// Fails when any of them contains an interior null byte.
    pub fn set_args(&mut self, args: &[&str]) -> Result<(), Error> {
        let c_args = to_cstrings(args)?;
        let c_arg_ptrs: Vec<*const c_char> = c_args.iter().map(|s| s.as_ptr()).collect();
        unsafe { sys::zwasm_wasi_config_set_args(self.ptr, c_arg_ptrs.len(), c_arg_ptrs.as_ptr()) };
        Ok(())
    }

    /// Sets the guest's environment, replacing any previous value.
    ///
    /// The strings are copied. Fails when any of them contains an interior null
    /// byte.
    pub fn set_envs(&mut self, envs: &[(&str, &str)]) -> Result<(), Error> {
        let (keys, vals): (Vec<&str>, Vec<&str>) = envs.iter().cloned().unzip();
        let c_keys = to_cstrings(&keys)?;
        let c_vals = to_cstrings(&vals)?;
        let c_key_ptrs: Vec<*const c_char> = c_keys.iter().map(|s| s.as_ptr()).collect();
        let c_val_ptrs: Vec<*const c_char> = c_vals.iter().map(|s| s.as_ptr()).collect();
        unsafe {
            sys::zwasm_wasi_config_set_envs(
                self.ptr,
                c_key_ptrs.len(),
                c_key_ptrs.as_ptr(),
                c_val_ptrs.as_ptr(),
            )
        };
        Ok(())
    }

    /// Queues `host_path` to appear in the guest as `guest_path`.
    ///
    /// Preopens get file descriptors 3, 4 and so on, in the order they are queued.
    /// Calling this only records the request; the directory is opened during
    /// instantiation, so an unopenable path surfaces as a failure from
    /// [`Instance::new`](crate::instance::Instance::new) rather than here.
    pub fn preopen_dir(&mut self, host_path: &str, guest_path: &str) -> Result<(), Error> {
        let c_host_path = CString::new(host_path)
            .map_err(|_| Error::Message("host path contains an interior null byte".to_string()))?;
        let c_guest_path = CString::new(guest_path)
            .map_err(|_| Error::Message("guest path contains an interior null byte".to_string()))?;
        let result = unsafe {
            sys::zwasm_wasi_config_preopen_dir(
                self.ptr,
                c_host_path.as_ptr(),
                c_guest_path.as_ptr(),
            )
        };
        if result {
            Ok(())
        } else {
            Err(Error::Message("failed to preopen directory".to_string()))
        }
    }
}

impl Default for WasiConfig {
    /// Creates a config, panicking on failure.
    ///
    /// Use [`WasiConfig::new`] to handle the allocation failure instead.
    fn default() -> Self {
        Self::new().expect("failed to create default WASI config")
    }
}

impl Drop for WasiConfig {
    fn drop(&mut self) {
        unsafe { sys::zwasm_wasi_config_delete(self.ptr) };
    }
}

/// Converts borrowed strings for the C API, rejecting interior null bytes.
fn to_cstrings(strs: &[&str]) -> Result<Vec<CString>, Error> {
    strs.iter()
        .map(|s| CString::new(*s))
        .collect::<Result<_, _>>()
        .map_err(|_| Error::Message("string contains an interior null byte".to_string()))
}
