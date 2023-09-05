/*
 This Source Code Form is subject to the terms of the Mozilla Public
 License, v. 2.0. If a copy of the MPL was not distributed with this
 file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::{
    ffi::{c_int, c_void, CStr, CString},
    marker::PhantomData,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::{null, null_mut, NonNull},
};

use crate::{
    api, error::FilterError, ffi, ApiRef, AudioFormat, AudioInfo, ColorFamily, Filter,
    FilterExtern, Frame, FrameContext, FunctionRef, MapMut, SampleType, VideoFormat, VideoInfo,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct CoreRef<'c> {
    handle: NonNull<ffi::VSCore>,
    marker: PhantomData<&'c Core>,
}

impl<'c> CoreRef<'c> {
    #[must_use]
    pub unsafe fn from_ptr(ptr: *const ffi::VSCore) -> Self {
        Self {
            handle: NonNull::new_unchecked(ptr.cast_mut()),
            marker: PhantomData,
        }
    }
}

impl<'c> Deref for CoreRef<'c> {
    type Target = Core;

    fn deref(&self) -> &'c Self::Target {
        unsafe { &*(self as *const CoreRef<'c>).cast() }
    }
}

impl<'c> DerefMut for CoreRef<'c> {
    fn deref_mut(&mut self) -> &'c mut Self::Target {
        unsafe { &mut *(self as *mut CoreRef<'c>).cast() }
    }
}

#[derive(PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct Core {
    handle: NonNull<ffi::VSCore>,
}

impl Core {
    #[must_use]
    pub fn new() -> Self {
        Self::new_with(0)
    }

    fn new_with(flags: i32) -> Self {
        let core = unsafe { (api().createCore)(flags) };
        Self {
            // Safety: `core` is always a valid pointer to a `VSCore` instance.
            handle: unsafe { NonNull::new_unchecked(core) },
        }
    }

    #[must_use]
    pub fn as_ptr(&self) -> *const ffi::VSCore {
        self.handle.as_ptr()
    }

    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut ffi::VSCore {
        self.handle.as_ptr()
    }

    pub fn set_max_cache_size(&mut self, size: i64) {
        unsafe {
            (api().setMaxCacheSize)(size, self.as_mut_ptr());
        }
    }

    pub fn set_thread_count(&mut self, count: i32) {
        unsafe {
            (api().setThreadCount)(count, self.as_mut_ptr());
        }
    }

    #[must_use]
    pub fn get_info(&self) -> ffi::VSCoreInfo {
        unsafe {
            let mut info = MaybeUninit::uninit();
            (api().getCoreInfo)(self.as_ptr().cast_mut(), info.as_mut_ptr());
            info.assume_init()
        }
    }

    /// # Errors
    ///
    /// Return error from underlying API
    pub fn create_video_filter<F: Filter>(
        &mut self,
        mut out: MapMut<'_>,
        name: impl Into<Vec<u8>>,
        info: &VideoInfo,
        filter: Box<F>,
        dependencies: &[ffi::VSFilterDependency],
    ) -> Result<(), FilterError> {
        unsafe {
            let name = CString::new(name).map_err(|_| FilterError::InvalidName)?;
            (api().createVideoFilter)(
                out.as_mut_ptr(),
                name.as_ptr(),
                info,
                F::filter_get_frame,
                Some(F::filter_free),
                F::FILTER_MODE.into(),
                dependencies.as_ptr(),
                dependencies
                    .len()
                    .try_into()
                    .map_err(|_| FilterError::TooMuchDependency)?,
                Box::into_raw(filter).cast(),
                self.as_mut_ptr(),
            );
        }

        if let Some(e) = out.get_error() {
            return Err(FilterError::Internal(
                String::from_utf8_lossy(e.as_bytes()).into(),
            ));
        }

        Ok(())
    }

    /// # Errors
    ///
    /// Return error from underlying API
    pub fn create_audio_filter<F: Filter>(
        &mut self,
        mut out: MapMut<'_>,
        name: impl Into<Vec<u8>>,
        info: &AudioInfo,
        filter: F,
        dependencies: &[ffi::VSFilterDependency],
    ) -> Result<(), CString> {
        let filter = Box::new(filter);
        unsafe {
            let name = CString::new(name)
                .map_err(|_| CString::from_vec_unchecked(b"Invalid name".to_vec()))?;
            (api().createAudioFilter)(
                out.as_mut_ptr(),
                name.as_ptr(),
                info,
                F::filter_get_frame,
                Some(F::filter_free),
                F::FILTER_MODE.into(),
                dependencies.as_ptr(),
                dependencies.len().try_into().map_err(|_| {
                    CString::from_vec_unchecked(
                        b"dependencies len is larger than i32::MAX".to_vec(),
                    )
                })?,
                Box::into_raw(filter).cast(),
                self.as_mut_ptr(),
            );
        }

        if let Some(e) = out.get_error() {
            return Err(e);
        }
        Ok(())
    }

    #[must_use]
    pub fn new_video_frame(
        &self,
        format: &VideoFormat,
        width: i32,
        height: i32,
        prop_src: Option<&Frame>,
    ) -> Frame {
        unsafe {
            let ptr = (api().newVideoFrame)(
                format,
                width,
                height,
                prop_src.map_or(null_mut(), |f| f.as_ptr()),
                self.as_ptr().cast_mut(),
            );
            Frame::from_ptr(ptr)
        }
    }

    #[must_use]
    pub fn new_video_frame2(
        &self,
        format: &VideoFormat,
        width: i32,
        height: i32,
        plane_src: &[*const ffi::VSFrame],
        planes: &[i32],
        prop_src: Option<&Frame>,
    ) -> Frame {
        unsafe {
            let ptr = (api().newVideoFrame2)(
                format,
                width,
                height,
                plane_src.as_ptr(),
                planes.as_ptr(),
                prop_src.map_or(null_mut(), |f| f.as_ptr()),
                self.as_ptr().cast_mut(),
            );
            Frame::from_ptr(ptr)
        }
    }

    #[must_use]
    pub fn new_audio_frame(
        &self,
        format: &AudioFormat,
        num_samples: i32,
        prop_src: Option<&Frame>,
    ) -> Frame {
        unsafe {
            let ptr = (api().newAudioFrame)(
                format,
                num_samples,
                prop_src.map_or(null_mut(), |f| f.as_ptr()),
                self.as_ptr().cast_mut(),
            );
            Frame::from_ptr(ptr)
        }
    }

    #[must_use]
    pub fn new_audio_frame2(
        &self,
        format: &AudioFormat,
        num_samples: i32,
        channel_src: &[*const ffi::VSFrame],
        channels: &[i32],
        prop_src: Option<&Frame>,
    ) -> Frame {
        unsafe {
            let ptr = (api().newAudioFrame2)(
                format,
                num_samples,
                channel_src.as_ptr(),
                channels.as_ptr(),
                prop_src.map_or(null_mut(), |f| f.as_ptr()),
                self.as_ptr().cast_mut(),
            );
            Frame::from_ptr(ptr)
        }
    }

    #[must_use]
    pub fn copy_frame(&self, frame: &Frame) -> Frame {
        unsafe { Frame::from_ptr((api().copyFrame)(frame.as_ptr(), self.as_ptr().cast_mut())) }
    }

    #[must_use]
    pub fn query_video_format(
        &self,
        color_family: ColorFamily,
        sample_type: SampleType,
        bits_per_sample: i32,
        subsampling_w: i32,
        subsampling_h: i32,
    ) -> VideoFormat {
        unsafe {
            let mut format = MaybeUninit::uninit();
            (api().queryVideoFormat)(
                format.as_mut_ptr(),
                color_family.into(),
                sample_type.into(),
                bits_per_sample,
                subsampling_w,
                subsampling_h,
                self.as_ptr().cast_mut(),
            );
            format.assume_init()
        }
    }

    #[must_use]
    pub fn query_audio_format(
        &self,
        sample_type: SampleType,
        bits_per_sample: i32,
        channel_layout: u64,
    ) -> AudioFormat {
        unsafe {
            let mut format = MaybeUninit::uninit();
            (api().queryAudioFormat)(
                format.as_mut_ptr(),
                sample_type.into(),
                bits_per_sample,
                channel_layout,
                self.as_ptr().cast_mut(),
            );
            format.assume_init()
        }
    }

    #[must_use]
    pub fn query_video_format_id(
        &self,
        color_family: ColorFamily,
        sample_type: SampleType,
        bits_per_sample: i32,
        subsampling_w: i32,
        subsampling_h: i32,
    ) -> u32 {
        unsafe {
            (api().queryVideoFormatID)(
                color_family.into(),
                sample_type.into(),
                bits_per_sample,
                subsampling_w,
                subsampling_h,
                self.as_ptr().cast_mut(),
            )
        }
    }

    #[must_use]
    pub fn get_video_format_by_id(&self, id: u32) -> VideoFormat {
        unsafe {
            let mut format = MaybeUninit::uninit();
            (api().getVideoFormatByID)(format.as_mut_ptr(), id, self.as_ptr().cast_mut());
            format.assume_init()
        }
    }

    pub fn create_function<T>(
        &mut self,
        func: ffi::VSPublicFunction,
        data: Box<T>,
        free: ffi::VSFreeFunctionData,
    ) -> FunctionRef {
        unsafe {
            FunctionRef::from_ptr((api().createFunction)(
                func,
                Box::into_raw(data).cast(),
                free,
                self.as_mut_ptr(),
            ))
        }
    }

    pub fn log(&mut self, level: ffi::VSMessageType, msg: &CStr) {
        unsafe {
            (api().logMessage)(level, msg.as_ptr(), self.as_mut_ptr());
        }
    }
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        unsafe {
            (api().freeCore)(self.handle.as_ptr());
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct CoreBuilder {
    flags: i32,
    api: Option<ApiRef>,
    max_cache_size: Option<i64>,
    thread_count: Option<i32>,
}

impl CoreBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn build(self) -> Core {
        let mut core = Core::new_with(self.flags);
        if let Some(size) = self.max_cache_size {
            core.set_max_cache_size(size);
        }
        if let Some(count) = self.thread_count {
            core.set_thread_count(count);
        }
        core
    }

    pub fn enable_graph_inspection(&mut self) -> &mut Self {
        self.flags |= ffi::VSCoreCreationFlags::ccfEnableGraphInspection as i32;
        self
    }

    pub fn disable_auto_loading(&mut self) -> &mut Self {
        self.flags |= ffi::VSCoreCreationFlags::ccfDisableAutoLoading as i32;
        self
    }

    pub fn disable_library_unloading(&mut self) -> &mut Self {
        self.flags |= ffi::VSCoreCreationFlags::ccfDisableLibraryUnloading as i32;
        self
    }

    pub fn max_cache_size(&mut self, size: i64) -> &mut Self {
        self.max_cache_size = Some(size);
        self
    }

    pub fn thread_count(&mut self, count: i32) -> &mut Self {
        self.thread_count = Some(count);
        self
    }

    pub fn api(&mut self, api: ApiRef) -> &mut Self {
        self.api = Some(api);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api() {
        let core = Core::new();
        let info = core.get_info();
        println!("{info:?}");
    }

    #[test]
    fn builder() {
        let core = CoreBuilder::new()
            .enable_graph_inspection()
            .disable_auto_loading()
            .disable_library_unloading()
            .max_cache_size(1024)
            .thread_count(4)
            .build();
        assert_eq!(core.get_info().maxFramebufferSize, 1024);
        assert_eq!(core.get_info().numThreads, 4);
    }
}
