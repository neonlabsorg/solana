

pub fn cast(ptr: *const u8) {
    #[cfg(target_arch = "bpf")]
    {
        extern "C" {
            fn sol_cast(ptr: *const u8) -> u64;
        }

        unsafe {
            sol_cast(ptr);
        }
    }
}
