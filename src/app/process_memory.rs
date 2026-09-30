#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessMemoryMetrics {
    /// Actual private commit charge allocated by UltraViewer (heap, stack, piece table, UI buffers).
    pub private_bytes: u64,
    /// Clean file-backed memory pages held in RAM by the Windows kernel as standby cache.
    pub os_cache_bytes: u64,
    /// Total resident working set (private + OS cache).
    pub total_working_set: u64,
}

#[cfg(target_os = "windows")]
mod win_mem {
    use std::ffi::c_void;
    use super::ProcessMemoryMetrics;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCountersEx {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
        PrivateUsage: usize,
    }

    #[link(name = "psapi")]
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessMemoryInfo(
            process: *mut c_void,
            ppsmx: *mut ProcessMemoryCountersEx,
            cb: u32,
        ) -> i32;
        fn SetProcessWorkingSetSize(
            process: *mut c_void,
            minimum_working_set_size: usize,
            maximum_working_set_size: usize,
        ) -> i32;
    }

    pub fn query_memory() -> ProcessMemoryMetrics {
        unsafe {
            let proc = GetCurrentProcess();
            let mut counters: ProcessMemoryCountersEx = std::mem::zeroed();
            counters.cb = std::mem::size_of::<ProcessMemoryCountersEx>() as u32;

            if GetProcessMemoryInfo(proc, &mut counters, counters.cb) != 0 {
                let working_set = counters.WorkingSetSize as u64;
                let private_bytes = if counters.PrivateUsage > 0 {
                    counters.PrivateUsage as u64
                } else {
                    counters.PagefileUsage as u64
                };
                let os_cache_bytes = working_set.saturating_sub(private_bytes);

                ProcessMemoryMetrics {
                    private_bytes,
                    os_cache_bytes,
                    total_working_set: working_set,
                }
            } else {
                ProcessMemoryMetrics::default()
            }
        }
    }

    pub fn trim_working_set() {
        unsafe {
            let proc = GetCurrentProcess();
            // Trimming working set tells Windows to immediately move clean file-backed pages to standby list
            SetProcessWorkingSetSize(proc, usize::MAX, usize::MAX);
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod non_win_mem {
    use super::ProcessMemoryMetrics;

    pub fn query_memory() -> ProcessMemoryMetrics {
        ProcessMemoryMetrics::default()
    }

    pub fn trim_working_set() {}
}

pub fn query_memory() -> ProcessMemoryMetrics {
    #[cfg(target_os = "windows")]
    {
        win_mem::query_memory()
    }
    #[cfg(not(target_os = "windows"))]
    {
        non_win_mem::query_memory()
    }
}

pub fn trim_working_set() {
    #[cfg(target_os = "windows")]
    win_mem::trim_working_set();
}
