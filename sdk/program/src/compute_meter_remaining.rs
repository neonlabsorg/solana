

pub fn compute_meter_remaining( remaining :&mut u64) {
    // #[cfg(not(target_arch = "bpf"))]{
    //     return 0
    // }

    #[cfg(target_arch = "bpf")]
    {
        extern "C" {
            fn sol_compute_meter_remaining(remaining: *mut u8) -> u64;
        }

        // let mut remaining: u64 = 0;
        unsafe {
            sol_compute_meter_remaining( remaining as *mut _ as *mut u8);
        }
        // remaining
    }
}
