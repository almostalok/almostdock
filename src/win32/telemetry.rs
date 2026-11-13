use std::mem::size_of;
use windows_sys::Win32::Foundation::{BOOL, FILETIME, HANDLE};
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};

#[link(name = "kernel32")]
extern "system" {
    fn GetSystemTimes(
        lpIdleTime: *mut FILETIME,
        lpKernelTime: *mut FILETIME,
        lpUserTime: *mut FILETIME,
    ) -> BOOL;

    fn K32EmptyWorkingSet(hProcess: HANDLE) -> BOOL;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryTelemetry {
    pub private_usage_bytes: usize,
    pub working_set_bytes: usize,
    pub peak_working_set_bytes: usize,
}

impl MemoryTelemetry {
    pub fn private_usage_mb(&self) -> f64 {
        self.private_usage_bytes as f64 / (1024.0 * 1024.0)
    }

    pub fn working_set_mb(&self) -> f64 {
        self.working_set_bytes as f64 / (1024.0 * 1024.0)
    }
}

pub fn get_process_memory() -> MemoryTelemetry {
    let mut pmc: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    pmc.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;

    let handle: HANDLE = unsafe { GetCurrentProcess() };
    let success = unsafe {
        K32GetProcessMemoryInfo(
            handle,
            &mut pmc as *mut _ as *mut _,
            size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        )
    };

    if success != 0 {
        MemoryTelemetry {
            private_usage_bytes: pmc.PrivateUsage,
            working_set_bytes: pmc.WorkingSetSize,
            peak_working_set_bytes: pmc.PeakWorkingSetSize,
        }
    } else {
        MemoryTelemetry::default()
    }
}

pub fn trim_working_set() {
    let handle = unsafe { GetCurrentProcess() };
    unsafe {
        // 1. Flush memory pages using PSAPI EmptyWorkingSet
        K32EmptyWorkingSet(handle);

        // 2. Set min/max working set to (size_t)-1 to purge standby memory
        SetProcessWorkingSetSize(handle, usize::MAX, usize::MAX);
    }
}

#[derive(Debug, Default)]
pub struct SystemCpuSampler {
    prev_idle: u64,
    prev_kernel: u64,
    prev_user: u64,
}

impl SystemCpuSampler {
    pub fn new() -> Self {
        let mut s = Self::default();
        s.sample();
        s
    }

    fn filetime_to_u64(ft: FILETIME) -> u64 {
        ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
    }

    pub fn sample(&mut self) -> f32 {
        let mut idle: FILETIME = unsafe { std::mem::zeroed() };
        let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
        let mut user: FILETIME = unsafe { std::mem::zeroed() };

        let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
        if ok == 0 {
            return 0.0;
        }

        let idle_u64 = Self::filetime_to_u64(idle);
        let kernel_u64 = Self::filetime_to_u64(kernel);
        let user_u64 = Self::filetime_to_u64(user);

        let idle_diff = idle_u64.saturating_sub(self.prev_idle);
        let kernel_diff = kernel_u64.saturating_sub(self.prev_kernel);
        let user_diff = user_u64.saturating_sub(self.prev_user);

        self.prev_idle = idle_u64;
        self.prev_kernel = kernel_u64;
        self.prev_user = user_u64;

        let total = kernel_diff + user_diff;
        if total == 0 {
            0.0
        } else {
            let active = total.saturating_sub(idle_diff);
            ((active as f64 / total as f64) * 100.0) as f32
        }
    }
}
